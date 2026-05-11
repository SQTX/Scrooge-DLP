// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 2 as
// published by the Free Software Foundation.

//! `scrooge-agent-macos` — implementacja `PlatformAgent` dla macOS.
//!
//! MVP (Krok 7): stuby logujące „start" i zwracające `Ok(())`. Pełne moduły
//! dochodzą w Milestones 3–7:
//! - `mod_clipscreen` przez `arboard` + IOKit hot-keys (Milestone 3)
//! - `mod_devctl` przez IOKit (Milestone 3)
//! - `mod_filemon` przez FSEvents (`notify`) (Milestone 4)
//! - `mod_netinsp` przez NetworkExtension (Milestone 6, wymaga entitlements)
//! - `mod_response` (Milestone 7)

use async_trait::async_trait;
use scrooge_agent_core::{EventSender, PlatformAgent, PlatformError, ResponseAction};

/// Implementacja `PlatformAgent` dla macOS.
#[derive(Debug, Default)]
pub struct MacosAgent;

impl MacosAgent {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl PlatformAgent for MacosAgent {
    async fn init(&self) -> Result<(), PlatformError> {
        tracing::info!(platform = "macos", "MacosAgent::init (stub)");
        Ok(())
    }

    async fn start_filemon(&self, _tx: EventSender) -> Result<(), PlatformError> {
        tracing::info!(platform = "macos", module = "filemon", "started (stub)");
        Ok(())
    }

    async fn start_devctl(&self, _tx: EventSender) -> Result<(), PlatformError> {
        tracing::info!(platform = "macos", module = "devctl", "started (stub)");
        Ok(())
    }

    async fn start_clipscreen(&self, _tx: EventSender) -> Result<(), PlatformError> {
        tracing::info!(platform = "macos", module = "clipscreen", "started (stub)");
        Ok(())
    }

    async fn start_netinsp(&self, _tx: EventSender) -> Result<(), PlatformError> {
        tracing::info!(platform = "macos", module = "netinsp", "started (stub)");
        Ok(())
    }

    async fn execute_action(&self, action: ResponseAction) -> Result<(), PlatformError> {
        tracing::info!(platform = "macos", ?action, "execute_action (stub)");
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), PlatformError> {
        tracing::info!(platform = "macos", "MacosAgent::shutdown (stub)");
        Ok(())
    }
}
