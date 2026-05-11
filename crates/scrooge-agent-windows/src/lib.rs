// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 2 as
// published by the Free Software Foundation.

//! `scrooge-agent-windows` — implementacja `PlatformAgent` dla Windows.
//!
//! **W MVP wszystkie metody zwracają `Err(PlatformError::NotImplemented)`** —
//! crate kompiluje się na każdej platformie (więc workspace check przechodzi),
//! ale uruchomienie na Windows skończy się błędem przy `init()`. Pełna
//! implementacja (WFP, USN Journal, SetupAPI, minifilter driver) w Milestone 8+
//! gdy będzie dostępna maszyna Windows do testów.

use async_trait::async_trait;
use scrooge_agent_core::{EventSender, PlatformAgent, PlatformError, ResponseAction};

/// Stub implementacji `PlatformAgent` dla Windows.
#[derive(Debug, Default)]
pub struct WindowsAgent;

impl WindowsAgent {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl PlatformAgent for WindowsAgent {
    async fn init(&self) -> Result<(), PlatformError> {
        Err(PlatformError::NotImplemented(
            "WindowsAgent (planowane: Milestone 8+)",
        ))
    }

    async fn start_filemon(&self, _tx: EventSender) -> Result<(), PlatformError> {
        Err(PlatformError::NotImplemented("mod_filemon (Windows)"))
    }

    async fn start_devctl(&self, _tx: EventSender) -> Result<(), PlatformError> {
        Err(PlatformError::NotImplemented("mod_devctl (Windows)"))
    }

    async fn start_clipscreen(&self, _tx: EventSender) -> Result<(), PlatformError> {
        Err(PlatformError::NotImplemented("mod_clipscreen (Windows)"))
    }

    async fn start_netinsp(&self, _tx: EventSender) -> Result<(), PlatformError> {
        Err(PlatformError::NotImplemented("mod_netinsp (Windows)"))
    }

    async fn execute_action(&self, _action: ResponseAction) -> Result<(), PlatformError> {
        Err(PlatformError::NotImplemented("mod_response (Windows)"))
    }

    async fn shutdown(&self) -> Result<(), PlatformError> {
        Ok(()) // no-op — nic nie zostało wystartowane
    }
}
