// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! Persistent event queue na agencie — sqlite WAL.
//!
//! Agent zapisuje każdy wyemitowany `Event` lokalnie zanim spróbuje go
//! wysłać do managera. Gdy manager jest offline lub gRPC fail, eventy
//! zostają w `events_pending` z `sent_at = NULL` i są retried przy
//! następnej iteracji `event_uploader`'a.
//!
//! Schema:
//! ```sql
//! CREATE TABLE events_pending (
//!     id          INTEGER PRIMARY KEY AUTOINCREMENT,
//!     created_at  INTEGER NOT NULL,   -- unix epoch seconds UTC
//!     sent_at     INTEGER,            -- NULL = jeszcze nie wysłany
//!     payload     BLOB NOT NULL       -- prost-encoded Event
//! );
//! ```
//!
//! Retencja: `purge_sent_older_than()` usuwa wpisy gdzie
//! `sent_at IS NOT NULL AND sent_at < cutoff` — wywoływane okresowo
//! przez agent-bin (np. raz dziennie). Failed eventy (sent_at = NULL)
//! NIE są nigdy purge'owane — admin musi je obsłużyć ręcznie.

use std::path::Path;

use prost::Message;
use scrooge_proto::v1::Event;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    Row, SqlitePool,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum QueueError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] sqlx::Error),
    #[error("prost encode: {0}")]
    Encode(#[from] prost::EncodeError),
    #[error("prost decode: {0}")]
    Decode(#[from] prost::DecodeError),
}

/// Pojedynczy wpis fetched z kolejki — `id` potrzebny do `mark_sent`.
#[derive(Debug, Clone)]
pub struct PendingEvent {
    pub id: i64,
    pub event: Event,
}

/// Wrapper nad SqlitePool z WAL mode i schemą `events_pending`.
#[derive(Debug, Clone)]
pub struct EventQueue {
    pool: SqlitePool,
}

impl EventQueue {
    /// Otwórz (lub stwórz) bazę pod `path`. WAL mode dla concurrent reads.
    pub async fn open(path: &Path) -> Result<Self, QueueError> {
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal);

        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(opts)
            .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS events_pending (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at  INTEGER NOT NULL,
                sent_at     INTEGER,
                payload     BLOB NOT NULL
            )",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS events_pending_unsent_idx
                ON events_pending (id) WHERE sent_at IS NULL",
        )
        .execute(&pool)
        .await?;

        Ok(Self { pool })
    }

    /// Zapisz event do kolejki — zwraca przydzielony `id`.
    pub async fn enqueue(&self, event: &Event) -> Result<i64, QueueError> {
        let payload = event.encode_to_vec();
        let now = chrono::Utc::now().timestamp();
        let row = sqlx::query(
            "INSERT INTO events_pending (created_at, payload) VALUES (?, ?)
                 RETURNING id",
        )
        .bind(now)
        .bind(&payload)
        .fetch_one(&self.pool)
        .await?;
        let id: i64 = row.get("id");
        Ok(id)
    }

    /// Pobierz batch unsent eventów (FIFO po id). `limit` = max events.
    pub async fn take_batch(&self, limit: u32) -> Result<Vec<PendingEvent>, QueueError> {
        let rows = sqlx::query(
            "SELECT id, payload FROM events_pending
                 WHERE sent_at IS NULL
                 ORDER BY id ASC
                 LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let id: i64 = r.get("id");
            let payload: Vec<u8> = r.get("payload");
            let event = Event::decode(payload.as_slice())?;
            out.push(PendingEvent { id, event });
        }
        Ok(out)
    }

    /// Oznacz batch eventów jako wysłany (`sent_at = NOW`).
    pub async fn mark_sent(&self, ids: &[i64]) -> Result<(), QueueError> {
        if ids.is_empty() {
            return Ok(());
        }
        let now = chrono::Utc::now().timestamp();
        // sqlx nie ma natywnego array bindowania w sqlite — generujemy
        // `(?,?,?...)` ręcznie. Liczba ids jest ograniczona przez `take_batch`
        // limit, więc query rozsądnej długości.
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "UPDATE events_pending SET sent_at = ? WHERE id IN ({placeholders})"
        );
        let mut q = sqlx::query(&sql).bind(now);
        for id in ids {
            q = q.bind(id);
        }
        q.execute(&self.pool).await?;
        Ok(())
    }

    /// Usuń wpisy `sent_at IS NOT NULL AND sent_at < cutoff_unix_seconds`.
    /// Pending events NIE są dotykane — admin decyduje co z nimi.
    pub async fn purge_sent_older_than(
        &self,
        cutoff_unix_seconds: i64,
    ) -> Result<u64, QueueError> {
        let res = sqlx::query(
            "DELETE FROM events_pending
                 WHERE sent_at IS NOT NULL AND sent_at < ?",
        )
        .bind(cutoff_unix_seconds)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected())
    }

    /// Liczba unsent eventów — heartbeat exportuje to jako `events_in_queue`.
    pub async fn pending_count(&self) -> Result<u64, QueueError> {
        let row = sqlx::query("SELECT COUNT(*) as cnt FROM events_pending WHERE sent_at IS NULL")
            .fetch_one(&self.pool)
            .await?;
        let cnt: i64 = row.get("cnt");
        // `COUNT(*)` zwraca >= 0 — cast safe, ale clippy się czepia bo i64→u64.
        // `try_into` z fallback'iem zera na wszelki wypadek.
        Ok(u64::try_from(cnt).unwrap_or(0))
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Testy
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use scrooge_proto::v1::{event::Details, ClipboardEvent, EventType, Severity};
    use tempfile::TempDir;

    fn sample_event(tag: &str) -> Event {
        Event {
            event_id: format!("test-{tag}"),
            timestamp_ns: 0,
            agent_id: String::new(),
            r#type: EventType::ClipboardCopy as i32,
            severity: Severity::Medium as i32,
            direction: 0,
            user_name: String::new(),
            process_id: 0,
            process_name: String::new(),
            details: Some(Details::Clipboard(ClipboardEvent {
                content_size: 16,
                content_preview: "**** **** **** 0366".to_string(),
                content_hash: vec![],
                mime_type: "text/plain".to_string(),
                source_process_name: String::new(),
                was_cleared: false,
            })),
            matched_policy_id: String::new(),
            matched_rule_id: String::new(),
            action_taken: 0,
            matches: vec![],
            forward_to_siem: false,
        }
    }

    async fn open_temp_queue() -> (TempDir, EventQueue) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("events.db");
        let q = EventQueue::open(&path).await.unwrap();
        (dir, q)
    }

    #[tokio::test]
    async fn enqueue_take_roundtrip() {
        let (_dir, q) = open_temp_queue().await;
        let id = q.enqueue(&sample_event("a")).await.unwrap();
        assert!(id > 0);

        let batch = q.take_batch(10).await.unwrap();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].id, id);
        assert_eq!(batch[0].event.event_id, "test-a");
    }

    #[tokio::test]
    async fn mark_sent_excludes_from_take_batch() {
        let (_dir, q) = open_temp_queue().await;
        let id1 = q.enqueue(&sample_event("a")).await.unwrap();
        let id2 = q.enqueue(&sample_event("b")).await.unwrap();

        q.mark_sent(&[id1]).await.unwrap();

        let batch = q.take_batch(10).await.unwrap();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].id, id2);
    }

    #[tokio::test]
    async fn take_batch_respects_limit() {
        let (_dir, q) = open_temp_queue().await;
        for i in 0..5 {
            q.enqueue(&sample_event(&i.to_string())).await.unwrap();
        }
        let batch = q.take_batch(3).await.unwrap();
        assert_eq!(batch.len(), 3);
    }

    #[tokio::test]
    async fn purge_removes_only_sent() {
        let (_dir, q) = open_temp_queue().await;
        let id_pending = q.enqueue(&sample_event("pending")).await.unwrap();
        let id_sent = q.enqueue(&sample_event("sent")).await.unwrap();
        q.mark_sent(&[id_sent]).await.unwrap();

        // Cutoff w przyszłości (i64::MAX) — wszystko sent powinno zostać usunięte.
        let removed = q.purge_sent_older_than(i64::MAX).await.unwrap();
        assert_eq!(removed, 1);

        let batch = q.take_batch(10).await.unwrap();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].id, id_pending);
    }

    #[tokio::test]
    async fn pending_count_correct() {
        let (_dir, q) = open_temp_queue().await;
        assert_eq!(q.pending_count().await.unwrap(), 0);
        q.enqueue(&sample_event("a")).await.unwrap();
        q.enqueue(&sample_event("b")).await.unwrap();
        assert_eq!(q.pending_count().await.unwrap(), 2);
        let batch = q.take_batch(1).await.unwrap();
        q.mark_sent(&[batch[0].id]).await.unwrap();
        assert_eq!(q.pending_count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn fifo_order_preserved() {
        let (_dir, q) = open_temp_queue().await;
        q.enqueue(&sample_event("first")).await.unwrap();
        q.enqueue(&sample_event("second")).await.unwrap();
        q.enqueue(&sample_event("third")).await.unwrap();

        let batch = q.take_batch(10).await.unwrap();
        assert_eq!(batch[0].event.event_id, "test-first");
        assert_eq!(batch[1].event.event_id, "test-second");
        assert_eq!(batch[2].event.event_id, "test-third");
    }
}
