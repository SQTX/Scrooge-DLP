// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 2 as
// published by the Free Software Foundation.

//! `scrooge-common` — typy współdzielone przez agenta i managera ScroogeDLP.
//!
//! Moduły:
//! - [`error`] — wspólne typy błędów (`thiserror`), z aggregatorem [`Error`].
//! - [`config`] — `AgentConfig` i `ManagerConfig` (YAML).
//! - [`ca`] — wewnętrzne root CA (generowanie, persystencja, podpisywanie CSR).
//! - [`crypto`] — helpery kryptograficzne (bcrypt, stałe AES-GCM).

pub mod ca;
pub mod config;
pub mod crypto;
pub mod error;

pub use error::{CaError, ConfigError, CryptoError, Error, Result};
