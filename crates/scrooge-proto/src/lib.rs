// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 2 as
// published by the Free Software Foundation.

//! `scrooge-proto` — typy wygenerowane z `proto/scrooge.proto`.
//!
//! Schemat kompilowany w `build.rs` przez `tonic-build`. Ten crate
//! reeksportuje pakiet `scrooge.v1` zarówno jako moduł [`v1`],
//! jak i przez top-level reexport (`pub use v1::*`).

/// Wygenerowane typy z pakietu protobuf `scrooge.v1`.
pub mod v1 {
    // Generowany kod nie spełnia naszych pedantic lintów - tłumimy je
    // wyłącznie w tym module, reszta crate'a trzyma się reguł workspace.
    #![allow(
        clippy::all,
        clippy::pedantic,
        clippy::nursery,
        missing_docs,
        missing_debug_implementations,
        unreachable_pub
    )]

    tonic::include_proto!("scrooge.v1");
}

pub use v1::*;
