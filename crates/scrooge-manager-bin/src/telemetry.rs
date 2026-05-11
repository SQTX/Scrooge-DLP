// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 2 as
// published by the Free Software Foundation.

//! Inicjalizacja `tracing` dla managera.
//!
//! Format kontroluje `--log-format` (lub `LOG_FORMAT` env var):
//! - `json` (default, produkcja) — strukturalne logi do stdout.
//! - `pretty` (dev) — ludzko-czytelne logi z kolorami.
//!
//! Poziom kontroluje `RUST_LOG` env var (domyślnie `info`).

use clap::ValueEnum;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum LogFormat {
    /// JSON output (production).
    Json,
    /// Human-readable, kolorowy output (dev).
    Pretty,
}

/// Inicjalizuje globalny `tracing` subscriber. Wywołać raz na początku `main`.
pub(crate) fn init(format: LogFormat) {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    match format {
        LogFormat::Json => {
            tracing_subscriber::fmt()
                .json()
                .with_env_filter(env_filter)
                .with_current_span(true)
                .with_target(true)
                .init();
        },
        LogFormat::Pretty => {
            tracing_subscriber::fmt()
                .pretty()
                .with_env_filter(env_filter)
                .init();
        },
    }
}
