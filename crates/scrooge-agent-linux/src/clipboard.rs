// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! `mod_clipscreen` (Linux) — polling clipboard'u przez `arboard`.
//!
//! Linux X11/Wayland nie ma push-event'ów dla zmiany schowka — używamy
//! polling co `POLL_INTERVAL`. Hash poprzedniej treści zapobiega
//! re-emit'owaniu tego samego eventu gdy user wielokrotnie skopiuje
//! tę samą rzecz.
//!
//! Wymaga DISPLAY / WAYLAND_DISPLAY w env (agent uruchomiony w sesji
//! użytkownika — `systemd --user`, nie system unit). Gdy brak — task
//! loguje warn i kontynuuje (no-op). Production deployment story to
//! późniejsza iteracja.

use std::sync::Arc;
use std::time::Duration;

use arboard::Clipboard;
use scrooge_agent_core::{
    modules::classifier::ClassifierRegistry,
    EventSender,
};
use scrooge_proto::v1::{event::Details, ClipboardEvent, Event, EventType, Severity};
use sha2::{Digest, Sha256};
use tokio::task::JoinHandle;

/// Default polling interval — 500ms = rozsądny kompromis (latency vs CPU).
pub const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Spawn polling task. Zwraca `JoinHandle` żeby orchestrator (agent-bin)
/// mógł abort'ować przy shutdown.
///
/// Agent_id zostaje pusty w emitted `Event.agent_id` — manager wyczyta
/// z mTLS Subject CN (jak w heartbeat / PolicyAck). Zachowanie spójne z
/// resztą gRPC flow.
pub fn spawn(tx: EventSender, classifiers: Arc<ClassifierRegistry>) -> JoinHandle<()> {
    tokio::spawn(async move { run(tx, classifiers).await })
}

async fn run(tx: EventSender, classifiers: Arc<ClassifierRegistry>) {
    let mut clipboard = match Clipboard::new() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "clipboard: nie można otworzyć (brak DISPLAY / WAYLAND_DISPLAY?). \
                 Moduł nie wystartuje — agent musi działać w sesji użytkownika."
            );
            return;
        },
    };

    let mut prev_hash: Option<[u8; 32]> = None;
    let mut ticker = tokio::time::interval(POLL_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        ticker.tick().await;

        // arboard::get_text() jest blocking — ale to <1ms, trzymamy się
        // current task (alternative: tokio::task::spawn_blocking).
        let text = match clipboard.get_text() {
            Ok(t) => t,
            Err(arboard::Error::ContentNotAvailable) => continue,
            Err(e) => {
                // Częste przy lock screen — wyłączamy spam logu.
                tracing::trace!(error = %e, "clipboard read failed");
                continue;
            },
        };

        if text.is_empty() {
            continue;
        }

        let hash = hash_text(&text);
        if prev_hash == Some(hash) {
            continue; // ta sama zawartość co poprzedni tick — skip
        }
        prev_hash = Some(hash);

        let matches = classifiers.scan_all(&text);
        if matches.is_empty() {
            continue; // nic ciekawego — nie generujemy eventu dla każdego copy
        }

        // Emit jeden Event per detected classifier-hit. Każdy match → osobny
        // event (łatwiejszy filtering w dashboardzie, każdy ma własny last4).
        for m in matches {
            let event = build_event(&text, hash, &m);
            if let Err(e) = tx.send(event).await {
                tracing::error!(error = %e, "clipboard: event channel closed, exiting task");
                return;
            }
        }
    }
}

fn hash_text(text: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(text.as_bytes());
    let out = h.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    arr
}

fn build_event(
    text: &str,
    text_hash: [u8; 32],
    m: &scrooge_agent_core::modules::classifier::Match,
) -> Event {
    Event {
        event_id: uuid::Uuid::new_v4().to_string(),
        timestamp_ns: chrono::Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or(0),
        agent_id: String::new(), // manager wyczyta z mTLS Subject CN
        r#type: EventType::ClipboardCopy as i32,
        severity: Severity::Medium as i32, // default — polityki mogą podnieść
        direction: 0,
        user_name: std::env::var("USER").unwrap_or_default(),
        process_id: 0, // X11 nie ujawnia source process'u clipboard'u
        process_name: String::new(),
        details: Some(Details::Clipboard(ClipboardEvent {
            content_size: u64::try_from(text.len()).unwrap_or(u64::MAX),
            // content_preview ma cap 256 chars per proto comment;
            // używamy m.redacted (np. "**** **** **** 9010") — bezpieczne,
            // krótkie, deterministyczne.
            content_preview: m.redacted.clone(),
            content_hash: text_hash.to_vec(),
            mime_type: "text/plain".to_string(),
            source_process_name: String::new(),
            was_cleared: false,
        })),
        matched_policy_id: String::new(),
        matched_rule_id: String::new(),
        action_taken: 0,
        matches: vec![scrooge_proto::v1::Match {
            classifier_id: m.classifier.clone(),
            match_count: 1,
            sample_excerpt: m.redacted.clone(),
        }],
        forward_to_siem: false,
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Testy
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_text_deterministic() {
        assert_eq!(hash_text("4532015112830366"), hash_text("4532015112830366"));
        assert_ne!(hash_text("a"), hash_text("b"));
    }

    #[test]
    fn build_event_populates_classifier_match() {
        let m = scrooge_agent_core::modules::classifier::Match {
            classifier: "credit_card".to_string(),
            start: 0,
            end: 16,
            redacted: "**** **** **** 0366".to_string(),
            last4: "0366".to_string(),
        };
        let evt = build_event("4532015112830366", [0u8; 32], &m);
        assert_eq!(evt.r#type, EventType::ClipboardCopy as i32);
        assert_eq!(evt.matches.len(), 1);
        assert_eq!(evt.matches[0].classifier_id, "credit_card");
        if let Some(Details::Clipboard(c)) = evt.details {
            assert_eq!(c.content_preview, "**** **** **** 0366");
            assert_eq!(c.content_size, 16);
        } else {
            panic!("expected Clipboard details");
        }
    }
}
