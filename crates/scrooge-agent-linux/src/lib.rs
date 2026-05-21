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
//! Phase 3 MVP: `mod_clipscreen` produkcyjny (clipboard polling przez
//! `arboard` + classifier). Pozostałe moduły (`mod_filemon`, `mod_devctl`,
//! `mod_netinsp`, `mod_response`) wciąż stuby — wjadą w v0.3.x / v0.4.x.

pub mod clipboard;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use scrooge_agent_core::{
    modules::classifier::ClassifierRegistry, EventSender, PlatformAgent, PlatformError,
    ResponseAction,
};
use tokio::task::JoinHandle;

/// Implementacja `PlatformAgent` dla Linuksa.
///
/// Trzyma `ClassifierRegistry` przekazany przy konstrukcji (agent-bin
/// decyduje co załadować — Phase 3 MVP: tylko Luhn). Tasks spawnowane
/// przez `start_*` są stored w `tasks` żeby `shutdown` mógł je abort'ować.
#[derive(Debug)]
pub struct LinuxAgent {
    classifiers: Arc<ClassifierRegistry>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
}

impl Default for LinuxAgent {
    fn default() -> Self {
        Self::new(Arc::new(ClassifierRegistry::with_defaults()))
    }
}

impl LinuxAgent {
    #[must_use]
    pub fn new(classifiers: Arc<ClassifierRegistry>) -> Self {
        Self {
            classifiers,
            tasks: Mutex::new(Vec::new()),
        }
    }

    fn register_task(&self, h: JoinHandle<()>) {
        if let Ok(mut g) = self.tasks.lock() {
            g.push(h);
        }
    }
}

#[async_trait]
impl PlatformAgent for LinuxAgent {
    async fn init(&self) -> Result<(), PlatformError> {
        tracing::info!(
            platform = "linux",
            classifiers = self.classifiers.len(),
            "LinuxAgent::init"
        );
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

    async fn start_clipscreen(&self, tx: EventSender) -> Result<(), PlatformError> {
        tracing::info!(
            platform = "linux",
            module = "clipscreen",
            poll_ms = u64::try_from(clipboard::POLL_INTERVAL.as_millis()).unwrap_or(u64::MAX),
            "started"
        );
        let h = clipboard::spawn(tx, Arc::clone(&self.classifiers));
        self.register_task(h);
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
        let handles: Vec<JoinHandle<()>> = self
            .tasks
            .lock()
            .map(|mut g| std::mem::take(&mut *g))
            .unwrap_or_default();
        for h in handles {
            h.abort();
        }
        tracing::info!(platform = "linux", "LinuxAgent::shutdown");
        Ok(())
    }
}
