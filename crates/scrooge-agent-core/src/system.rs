// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! Cross-platform zbieranie podstawowych informacji o systemie. Używane przy
//! enrollment do wypełnienia `AgentInfo` (hostname, OS, arch, agent version).

use crate::error::PlatformError;

/// Informacje o systemie wysyłane do managera w `AgentInfo`.
#[derive(Debug, Clone)]
pub struct SystemInfo {
    pub hostname: String,
    /// Stała wartość per platforma: `"linux"`, `"macos"`, `"windows"`.
    pub os: String,
    /// Wersja OS (np. `"Ubuntu 22.04"`, `"macOS 14.5"`). Może być pusty.
    pub os_version: String,
    /// Architektura procesora (`"x86_64"`, `"aarch64"`, …).
    pub arch: String,
    /// Wersja agenta — zwykle `env!("CARGO_PKG_VERSION")` z binarki.
    pub agent_version: String,
}

/// Zbiera informacje o systemie cross-platform.
///
/// - `hostname` przez crate `hostname` (POSIX `gethostname` / Windows API).
/// - `os` / `arch` przez `std::env::consts::OS` / `std::env::consts::ARCH`.
/// - `os_version` przez `os_info` (parsuje `/etc/os-release` na Linux,
///   `sw_vers` na macOS, registry na Windows).
pub fn collect_system_info(agent_version: &str) -> Result<SystemInfo, PlatformError> {
    let hostname = hostname::get()
        .map_err(PlatformError::Io)?
        .to_string_lossy()
        .into_owned();

    let os_info = os_info::get();
    let os_version = format!("{} {}", os_info.os_type(), os_info.version());

    Ok(SystemInfo {
        hostname,
        os: std::env::consts::OS.to_string(),
        os_version,
        arch: std::env::consts::ARCH.to_string(),
        agent_version: agent_version.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_returns_expected_fields() {
        let info = collect_system_info("0.1.0-test").unwrap();
        assert!(!info.hostname.is_empty(), "hostname should not be empty");
        assert!(
            matches!(info.os.as_str(), "linux" | "macos" | "windows"),
            "os = {:?}",
            info.os
        );
        assert!(!info.arch.is_empty());
        assert_eq!(info.agent_version, "0.1.0-test");
    }
}
