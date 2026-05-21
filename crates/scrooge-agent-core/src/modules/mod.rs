// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! Moduły DLP — clipboard monitor, classifier itp. Każdy moduł emituje
//! `Event` do wspólnego `EventSender`. Orchestracja (spawn/abort) żyje
//! w `scrooge-agent-bin`.

pub mod classifier;
