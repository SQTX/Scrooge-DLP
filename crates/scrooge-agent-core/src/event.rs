// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! Eventy emitowane przez warstwę platformową + `ResponseAction` (akcje
//! egzekucyjne wywoływane przez orchestrator po stronie agenta).

use std::path::PathBuf;

use scrooge_proto::v1::Event;
use tokio::sync::mpsc;

/// Kanał event'ów — moduły platformowe wpychają tu `Event` (proto), core
/// agenta odbiera i wysyła do managera.
pub type EventSender = mpsc::Sender<Event>;

/// Akcje response wywoływane przez `mod_response` po stronie agenta.
///
/// Implementacja akcji jest platform-specific (`PlatformAgent::execute_action`),
/// bo np. quarantine wymaga AES-GCM + przesunięcia pliku, a `KillProcess` używa
/// natywnego API per OS (`kill(2)` / `TerminateProcess`).
#[derive(Debug, Clone)]
pub enum ResponseAction {
    /// Tylko zaloguj (np. dla `LOG_ONLY` policy).
    LogOnly,

    /// Przenieś plik do zaszyfrowanego quarantine directory.
    Quarantine { path: PathBuf },

    /// Zabij proces.
    KillProcess { pid: u32 },

    /// Zablokuj sesję użytkownika (LockWorkstation/CGSession).
    LockWorkstation,

    /// Pokaż notyfikację desktop'ową użytkownikowi.
    NotifyUser { title: String, message: String },

    /// Wyczyść schowek systemowy.
    ClearClipboard,
}
