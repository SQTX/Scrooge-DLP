// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 2 as
// published by the Free Software Foundation.

//! `scrooge-agent-linux` — implementacja `PlatformAgent` dla Linuksa.
//!
//! MVP (Krok 7): stuby logujące „start" i zwracające `Ok(())`. Pełne moduły
//! dochodzą w Milestones 3–7:
//! - `mod_clipscreen` przez `arboard` (Milestone 3)
//! - `mod_devctl` przez `tokio-udev` (Milestone 3)
//! - `mod_filemon` przez `notify` (Milestone 4)
//! - `mod_netinsp` przez eBPF / netfilter (Milestone 6)
//! - `mod_response` (kill, quarantine, lock) (Milestone 7)

use async_trait::async_trait;
use scrooge_agent_core::{EventSender, PlatformAgent, PlatformError, ResponseAction};

/// Implementacja `PlatformAgent` dla Linuksa.
///
/// Konstruktor nic nie zapisuje — wszystko ląduje w `init()`.
#[derive(Debug, Default)]
pub struct LinuxAgent;

impl LinuxAgent {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl PlatformAgent for LinuxAgent {
    async fn init(&self) -> Result<(), PlatformError> {
        tracing::info!(platform = "linux", "LinuxAgent::init (stub)");
        Ok(())
    }

    async fn start_filemon(&self, _tx: EventSender) -> Result<(), PlatformError> {
        tracing::info!(platform = "linux", module = "filemon", "started (stub)");
        Ok(())
    }

    async fn start_devctl(&self, _tx: EventSender) -> Result<(), PlatformError> {
        tracing::info!(platform = "linux", module = "devctl", "started (stub)");
        Ok(())
    }

    async fn start_clipscreen(&self, _tx: EventSender) -> Result<(), PlatformError> {
        tracing::info!(platform = "linux", module = "clipscreen", "started (stub)");
        Ok(())
    }

    async fn start_netinsp(&self, _tx: EventSender) -> Result<(), PlatformError> {
        tracing::info!(platform = "linux", module = "netinsp", "started (stub)");
        Ok(())
    }

    async fn execute_action(&self, action: ResponseAction) -> Result<(), PlatformError> {
        tracing::info!(platform = "linux", ?action, "execute_action (stub)");
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), PlatformError> {
        tracing::info!(platform = "linux", "LinuxAgent::shutdown (stub)");
        Ok(())
    }
}
