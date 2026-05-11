// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! Trait `PlatformAgent` — interfejs implementowany przez każdy crate
//! platformowy (`scrooge-agent-{linux,macos,windows}`).
//!
//! Konwencja:
//! - Metody `start_*` są fire-and-forget — spawnują wewnętrzną task'ę tokio
//!   (która emituje eventy przez `EventSender`) i wracają `Ok(())` od razu.
//! - [`PlatformAgent::shutdown`] zatrzymuje wszystkie zaspawnowane task'i
//!   gracefully.
//! - Wszystkie metody są async (nawet jeśli implementacja jest sync) — żeby
//!   trait był spójny i async-trait friendly.

use async_trait::async_trait;

use crate::{
    error::PlatformError,
    event::{EventSender, ResponseAction},
};

/// Interfejs platformowej warstwy agenta.
///
/// Implementacja per OS:
/// - `scrooge-agent-linux::LinuxAgent` (MVP: stuby logujące „start" / `Ok(())`)
/// - `scrooge-agent-macos::MacosAgent` (MVP: stuby)
/// - `scrooge-agent-windows::WindowsAgent` (MVP: `unimplemented!()` —
///   pełna implementacja w Milestone 8+)
#[async_trait]
pub trait PlatformAgent: Send + Sync + 'static {
    /// Inicjalizacja: tworzy katalogi danych, weryfikuje uprawnienia.
    async fn init(&self) -> Result<(), PlatformError>;

    /// Startuje `mod_filemon` — watch operacji na plikach (create/read/
    /// write/delete/rename/copy/move).
    async fn start_filemon(&self, tx: EventSender) -> Result<(), PlatformError>;

    /// Startuje `mod_devctl` — detekcja podłączenia USB / drukarek / dysków.
    async fn start_devctl(&self, tx: EventSender) -> Result<(), PlatformError>;

    /// Startuje `mod_clipscreen` — schowek i screenshoty.
    async fn start_clipscreen(&self, tx: EventSender) -> Result<(), PlatformError>;

    /// Startuje `mod_netinsp` — metadane połączeń (SNI, IP, transfer).
    async fn start_netinsp(&self, tx: EventSender) -> Result<(), PlatformError>;

    /// Wykonuje akcję response (BLOCK / QUARANTINE / KILL_PROCESS / …).
    async fn execute_action(&self, action: ResponseAction) -> Result<(), PlatformError>;

    /// Graceful shutdown wszystkich wewnętrznych task'i.
    async fn shutdown(&self) -> Result<(), PlatformError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trait musi być object-safe (do dynamic dispatch via `Box<dyn ...>`).
    #[test]
    fn platform_agent_is_object_safe() {
        fn _accept_dyn(_: Box<dyn PlatformAgent>) {}
    }
}
