// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 2 as
// published by the Free Software Foundation.

//! `scrooge-agent-core` — typy i kontrakty współdzielone przez wszystkie
//! implementacje agenta (Linux / macOS / Windows).
//!
//! Centralnym elementem jest trait [`PlatformAgent`] — interfejs platformowy,
//! który implementuje każdy crate `scrooge-agent-{linux,macos,windows}`.
//! Binarka `scrooge-agent-bin` wybiera implementację przez `cfg(target_os)`.

pub mod error;
pub mod event;
pub mod modules;
pub mod platform;
pub mod queue;
pub mod system;

pub use error::PlatformError;
pub use event::{EventSender, ResponseAction};
pub use platform::PlatformAgent;
pub use system::{collect_system_info, SystemInfo};
