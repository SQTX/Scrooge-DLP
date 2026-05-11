// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! Typy błędów warstwy platformowej.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlatformError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("platform feature not supported on this OS: {0}")]
    NotSupported(&'static str),

    #[error("platform feature not yet implemented: {0}")]
    NotImplemented(&'static str),

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("event channel closed")]
    ChannelClosed,

    #[error("other: {0}")]
    Other(String),
}
