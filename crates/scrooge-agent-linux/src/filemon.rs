// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! `mod_filemon` (Linux) — file events przez `notify` crate (inotify backend).
//!
//! Watch'uje skonfigurowane paths, emit'uje Event per file operation (create/
//! modify/delete/rename). Dla małych tekstowych plików dodatkowo skanuje
//! content przez ClassifierRegistry — jeśli match (PESEL/IBAN/karta/NIP),
//! event ma `matches: [...]` i `severity=medium` (zamiast `low`).
//!
//! Bez DISPLAY-dependency (inotify działa na headless server). Wymagane:
//! file read perms na watched paths (root system unit OK).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use notify::{Config, Event as NotifyEvent, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use scrooge_agent_core::{
    modules::classifier::ClassifierRegistry,
    EventSender,
};
use scrooge_proto::v1::{event::Details, Event, EventType, FileEvent, Match, Severity};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// Plik większy niż ten próg NIE jest scan'owany content'em (only metadata
/// event). Dla DLP najczęściej liczy się "co wyciekło" w małych dokumentach
/// (xlsx, pdf, txt). Binarki > 5MB zwykle nie zawierają wrażliwych danych
/// w form czytelnej dla regex'a.
pub const CONTENT_SCAN_MAX_BYTES: u64 = 5 * 1024 * 1024;

/// Spawn watcher task. Zwraca JoinHandle dla shutdown.
pub fn spawn(
    paths: Vec<PathBuf>,
    tx: EventSender,
    classifiers: Arc<ClassifierRegistry>,
) -> JoinHandle<()> {
    tokio::spawn(async move { run(paths, tx, classifiers).await })
}

async fn run(
    paths: Vec<PathBuf>,
    tx: EventSender,
    classifiers: Arc<ClassifierRegistry>,
) {
    if paths.is_empty() {
        tracing::debug!("filemon: no watched paths, task idle");
        return;
    }

    // notify sender jest sync (std::mpsc-like). Przerzucamy do tokio
    // przez async channel + osobny blocking thread.
    let (raw_tx, mut raw_rx) = mpsc::channel::<notify::Result<NotifyEvent>>(256);
    let blocking_tx = raw_tx.clone();

    let mut watcher: RecommendedWatcher = match notify::recommended_watcher(move |res| {
        let _ = blocking_tx.blocking_send(res);
    }) {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!(error = %e, "filemon: cannot create watcher (inotify limits?)");
            return;
        },
    };

    if let Err(e) = watcher.configure(Config::default().with_poll_interval(Duration::from_secs(2))) {
        tracing::warn!(error = %e, "filemon: configure");
    }

    for p in &paths {
        if let Err(e) = watcher.watch(p, RecursiveMode::Recursive) {
            tracing::warn!(path = %p.display(), error = %e, "filemon: watch failed (path exists?)");
        } else {
            tracing::info!(path = %p.display(), "filemon: watching");
        }
    }

    // Loop: pobieraj notify events, mapuj na proto Event.
    while let Some(res) = raw_rx.recv().await {
        let nev = match res {
            Ok(e) => e,
            Err(e) => {
                tracing::trace!(error = %e, "filemon: notify error");
                continue;
            },
        };
        for path in &nev.paths {
            let ev = build_event(nev.kind, path, &classifiers);
            if let Err(e) = tx.send(ev).await {
                tracing::error!(error = %e, "filemon: event channel closed");
                return;
            }
        }
    }
}

fn build_event(
    kind: EventKind,
    path: &std::path::Path,
    classifiers: &ClassifierRegistry,
) -> Event {
    let event_type = match kind {
        EventKind::Create(_) => EventType::FileCreate,
        EventKind::Modify(_) => EventType::FileWrite,
        EventKind::Remove(_) => EventType::FileDelete,
        _ => EventType::AgentInternal, // fallback dla rare event kinds
    };

    // Metadata + opcjonalny content scan.
    let (size_bytes, sha, matches) = inspect_file(path, classifiers);
    let severity = if matches.is_empty() {
        Severity::Low
    } else {
        Severity::Medium
    };

    Event {
        event_id: uuid::Uuid::new_v4().to_string(),
        timestamp_ns: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
        agent_id: String::new(),
        r#type: event_type as i32,
        severity: severity as i32,
        direction: 0,
        user_name: std::env::var("USER").unwrap_or_default(),
        process_id: 0,
        process_name: String::new(),
        details: Some(Details::File(FileEvent {
            path: path.to_string_lossy().to_string(),
            old_path: String::new(),
            destination_path: String::new(),
            size_bytes: i64::try_from(size_bytes).unwrap_or(0),
            sha256: sha,
            mime_type: String::new(),
            file_extension: path
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_string)
                .unwrap_or_default(),
        })),
        matched_policy_id: String::new(),
        matched_rule_id: String::new(),
        action_taken: 0,
        matches,
        forward_to_siem: false,
    }
}

/// Pobiera metadata + sha256 + classifier matches dla pliku (jeśli małe +
/// czytelne). Zwraca `(size, sha256_or_empty, matches)`.
fn inspect_file(
    path: &std::path::Path,
    classifiers: &ClassifierRegistry,
) -> (u64, Vec<u8>, Vec<Match>) {
    // Delete event — plik już nie istnieje, brak metadata. Zwracamy zera.
    let Ok(meta) = std::fs::metadata(path) else {
        return (0, Vec::new(), Vec::new());
    };
    if !meta.is_file() {
        return (meta.len(), Vec::new(), Vec::new());
    }

    let size = meta.len();
    if size > CONTENT_SCAN_MAX_BYTES {
        return (size, Vec::new(), Vec::new()); // za duży, tylko metadata
    }

    // Permission denied / I/O error → pomiń content scan, zwróć metadata.
    let Ok(bytes) = std::fs::read(path) else {
        return (size, Vec::new(), Vec::new());
    };

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let sha = hasher.finalize().to_vec();

    // Content scan tylko dla UTF-8 readable plików.
    let matches = std::str::from_utf8(&bytes)
        .ok()
        .map(|text| {
            classifiers
                .scan_all(text)
                .into_iter()
                .map(|m| Match {
                    classifier_id: m.classifier,
                    match_count: 1,
                    sample_excerpt: m.redacted,
                })
                .collect()
        })
        .unwrap_or_default();

    (size, sha, matches)
}

// ──────────────────────────────────────────────────────────────────────────
// Testy
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn inspect_small_text_file_finds_classifier_match() {
        let classifiers = ClassifierRegistry::with_defaults();
        let mut f = NamedTempFile::new().unwrap();
        // Visa test number.
        writeln!(f, "card: 4532015112830366").unwrap();
        let (size, sha, matches) = inspect_file(f.path(), &classifiers);
        assert!(size > 0);
        assert_eq!(sha.len(), 32);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].classifier_id, "credit_card");
    }

    #[test]
    fn inspect_nonexistent_returns_zero() {
        let classifiers = ClassifierRegistry::with_defaults();
        let (size, sha, matches) =
            inspect_file(std::path::Path::new("/nonexistent-path-xyz"), &classifiers);
        assert_eq!(size, 0);
        assert!(sha.is_empty());
        assert!(matches.is_empty());
    }

    #[test]
    fn inspect_binary_no_match() {
        let classifiers = ClassifierRegistry::with_defaults();
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(&[0u8, 1, 2, 3, 255, 254]).unwrap();
        let (size, sha, matches) = inspect_file(f.path(), &classifiers);
        assert_eq!(size, 6);
        assert_eq!(sha.len(), 32);
        assert!(matches.is_empty()); // not UTF-8
    }
}
