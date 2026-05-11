// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 2 as
// published by the Free Software Foundation.

//! Typy błędów `scrooge-common`. Każdy submodul ma swój `*Error`, a [`Error`]
//! jest aggregatorem do użytku przez callers którzy nie chcą rozróżniać domen.

use thiserror::Error;

/// Aggregator błędów `scrooge-common`. Każdy wariant `#[from]` pozwala na
/// implicit konwersję przez `?` z error'ów submodułów.
#[derive(Debug, Error)]
pub enum Error {
    #[error("config: {0}")]
    Config(#[from] ConfigError),

    #[error("CA: {0}")]
    Ca(#[from] CaError),

    #[error("crypto: {0}")]
    Crypto(#[from] CryptoError),
}

/// Wynik z domyślnym `Error` jako error type.
pub type Result<T> = std::result::Result<T, Error>;

/// Błędy modułu [`crate::config`].
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config file `{path}`: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse YAML: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("missing required field `{0}`")]
    MissingField(&'static str),

    #[error("invalid value for `{field}`: {message}")]
    InvalidValue {
        field: &'static str,
        message: String,
    },
}

/// Błędy modułu [`crate::ca`].
#[derive(Debug, Error)]
pub enum CaError {
    #[error("failed to read/write PEM file `{path}`: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("rcgen: {0}")]
    Rcgen(#[from] rcgen::Error),
}

/// Błędy modułu [`crate::crypto`].
#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("bcrypt: {0}")]
    Bcrypt(#[from] bcrypt::BcryptError),
}
