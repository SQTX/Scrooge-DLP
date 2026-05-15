// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 2 as
// published by the Free Software Foundation.

//! Konfiguracja agenta i managera w YAML.
//!
//! - [`AgentConfig`] — czytane przez agent z `/etc/scrooge/agent.yaml`
//!   (Linux/macOS) lub `%PROGRAMDATA%\ScroogeDLP\agent.yaml` (Windows).
//! - [`ManagerConfig`] — czytane przez manager z `/etc/scrooge/manager.yaml`
//!   (lub `MANAGER_CONFIG` env var).
//!
//! `load()` używa blokującego `std::fs` — config czytany jest raz przy starcie,
//! nie ma sensu zaciągać tokio do tej operacji.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::ConfigError;

// ──────────────────────────────────────────────────────────────────────────
// Agent
// ──────────────────────────────────────────────────────────────────────────

/// Pełna konfiguracja agenta.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub manager: AgentManagerConfig,
    pub agent: AgentRuntimeConfig,
}

/// Sekcja `manager:` — endpoint do którego agent się łączy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentManagerConfig {
    /// `host:port` gRPC endpoint managera (np. "192.168.64.1:5443").
    pub endpoint: String,

    /// Enrollment token. Wymagany przy pierwszym uruchomieniu; po
    /// enrollment może być usunięty (agent ma już swój cert).
    #[serde(default)]
    pub enrollment_token: Option<String>,

    /// Czy weryfikować TLS chain managera (default: true).
    #[serde(default = "default_true")]
    pub verify_tls: bool,

    /// Ścieżka do PEM z root CA managera (pinning).
    pub ca_cert_path: String,
}

/// Sekcja `agent:` — runtime agenta.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRuntimeConfig {
    /// Katalog danych (SQLite event queue, quarantine, cert, key).
    pub data_dir: String,

    #[serde(default = "default_log_level")]
    pub log_level: String,

    #[serde(default = "default_heartbeat_interval_secs")]
    pub heartbeat_interval_secs: u32,
}

impl AgentConfig {
    /// Wczytuje konfigurację z pliku YAML i waliduje.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let contents = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.display().to_string(),
            source,
        })?;
        let config: Self = serde_yaml::from_str(&contents)?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if self.manager.endpoint.is_empty() {
            return Err(ConfigError::MissingField("manager.endpoint"));
        }
        if !self.manager.endpoint.contains(':') {
            return Err(ConfigError::InvalidValue {
                field: "manager.endpoint",
                message: "expected `host:port`".to_string(),
            });
        }
        if self.agent.data_dir.is_empty() {
            return Err(ConfigError::MissingField("agent.data_dir"));
        }
        Ok(())
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Manager
// ──────────────────────────────────────────────────────────────────────────

/// Pełna konfiguracja managera.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagerConfig {
    pub server: ManagerServerConfig,
    pub database: DatabaseConfig,
    #[serde(default)]
    pub syslog_forwarder: SyslogForwarderConfig,
    pub auth: AuthConfig,
    #[serde(default)]
    pub dashboard: DashboardConfig,
}

/// Sekcja `server:` — listenery gRPC i REST + TLS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagerServerConfig {
    #[serde(default = "default_grpc_addr")]
    pub grpc_listen_addr: String,

    #[serde(default = "default_rest_addr")]
    pub rest_listen_addr: String,

    pub tls_cert_path: String,
    pub tls_key_path: String,

    pub ca_cert_path: String,
    pub ca_key_path: String,

    /// Publiczny gRPC endpoint (`host:port`) wpisywany w `agent.yaml` przez
    /// `/api/v1/install.sh`. Wymagany tylko gdy używasz install API — bez
    /// niego endpoint zwraca 503 z opisem co dodać do configu. W dev
    /// wystarczy `127.0.0.1:5443`, w produkcji za reverse proxy → publiczny
    /// FQDN agenta.
    #[serde(default)]
    pub public_grpc_endpoint: Option<String>,

    /// Publiczny base URL REST API (`https://manager.example.com`) bez
    /// trailing slash. Wstrzykiwany w one-liner `curl -fsSL {base}/install.sh
    /// | sudo bash`. Wymagany tylko gdy używasz install API.
    #[serde(default)]
    pub public_rest_base_url: Option<String>,

    /// GitHub Release tag używany jako źródło `.deb`/`.rpm` w `/install.sh`
    /// (np. `v0.1.0-rc6`). Wymagany gdy używasz install API. W przyszłości
    /// może domyślnie wskazywać na `latest` (po Sub-fazie 1B).
    #[serde(default)]
    pub agent_release_tag: Option<String>,
}

/// Sekcja `database:` — PostgreSQL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Pełen connection string (np. "postgres://user:pass@localhost:5432/scrooge").
    pub url: String,
    #[serde(default = "default_pool_max")]
    pub pool_max: u32,
}

/// Sekcja `syslog_forwarder:` — opcjonalny forward do Wazuh.
///
/// **Domyślnie wyłączony** zgodnie z briefem.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyslogForwarderConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub destination: Option<SyslogDestination>,
    #[serde(default)]
    pub filters: SyslogFilters,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyslogDestination {
    pub host: String,
    pub port: u16,
    #[serde(default = "default_protocol")]
    pub protocol: String, // "tcp_tls" | "tcp" | "udp"
    #[serde(default)]
    pub tls: Option<SyslogTls>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyslogTls {
    #[serde(default = "default_true")]
    pub verify: bool,
    pub ca_cert_path: Option<String>,
    pub client_cert_path: Option<String>,
    pub client_key_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyslogFilters {
    #[serde(default = "default_min_severity")]
    pub min_severity: String, // "low" | "medium" | "high" | "critical"

    #[serde(default = "default_true")]
    pub respect_per_rule_flag: bool,

    #[serde(default)]
    pub include_event_types: Vec<String>,

    #[serde(default)]
    pub exclude_event_types: Vec<String>,
}

impl Default for SyslogFilters {
    fn default() -> Self {
        Self {
            min_severity: default_min_severity(),
            respect_per_rule_flag: true,
            include_event_types: Vec::new(),
            exclude_event_types: Vec::new(),
        }
    }
}

/// Sekcja `auth:` — JWT signing secret + TTLs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub jwt_secret: String,
    #[serde(default = "default_access_token_ttl_secs")]
    pub access_token_ttl_secs: u64,
    #[serde(default = "default_refresh_token_ttl_secs")]
    pub refresh_token_ttl_secs: u64,
}

/// Sekcja `dashboard:` — preferencje dla embedded web dashboard'u.
/// Pobierane przez frontend przez `GET /api/v1/dashboard/config` (no auth).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DashboardConfig {
    #[serde(default)]
    pub auto_refresh: AutoRefreshConfig,
    #[serde(default)]
    pub session: SessionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoRefreshConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_refresh_interval_secs")]
    pub interval_secs: u32,
    /// `"always"` | `"active_only"` | `"off"`.
    /// `active_only` skipuje refresh gdy user idle > 60s (oszczędność CPU).
    #[serde(default = "default_refresh_mode")]
    pub mode: String,
}

impl Default for AutoRefreshConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_secs: default_refresh_interval_secs(),
            mode: default_refresh_mode(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Maks. sekund bezczynności (mysz/klawiatura/scroll) przed auto-logout.
    /// `0` = wyłączone.
    #[serde(default = "default_idle_timeout_secs")]
    pub idle_timeout_secs: u32,

    /// Absolutny hard-limit — niezależnie od aktywności. `0` = wyłączone.
    #[serde(default = "default_absolute_timeout_secs")]
    pub absolute_timeout_secs: u32,

    /// Ile sekund przed timeoutem pokazać toast „Session ending in Xs".
    #[serde(default = "default_warn_before_secs")]
    pub warn_before_secs: u32,

    /// Czy aktywność użytkownika automatycznie przedłuża session przez
    /// background refresh JWT (przed `access_token_ttl_secs` expiry).
    #[serde(default)]
    pub extend_on_activity: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            idle_timeout_secs: default_idle_timeout_secs(),
            absolute_timeout_secs: default_absolute_timeout_secs(),
            warn_before_secs: default_warn_before_secs(),
            extend_on_activity: false,
        }
    }
}

impl ManagerConfig {
    /// Wczytuje konfigurację z pliku YAML i waliduje.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let contents = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.display().to_string(),
            source,
        })?;
        let config: Self = serde_yaml::from_str(&contents)?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if self.auth.jwt_secret.len() < 32 {
            return Err(ConfigError::InvalidValue {
                field: "auth.jwt_secret",
                message: "must be at least 32 characters".to_string(),
            });
        }
        if self.database.url.is_empty() {
            return Err(ConfigError::MissingField("database.url"));
        }
        Ok(())
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Defaults
// ──────────────────────────────────────────────────────────────────────────

fn default_true() -> bool {
    true
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_heartbeat_interval_secs() -> u32 {
    30
}

fn default_grpc_addr() -> String {
    "0.0.0.0:5443".to_string()
}

fn default_rest_addr() -> String {
    "0.0.0.0:55000".to_string()
}

fn default_pool_max() -> u32 {
    16
}

fn default_protocol() -> String {
    "tcp_tls".to_string()
}

fn default_min_severity() -> String {
    "medium".to_string()
}

fn default_access_token_ttl_secs() -> u64 {
    3600
}

fn default_refresh_token_ttl_secs() -> u64 {
    30 * 24 * 3600
}

fn default_refresh_interval_secs() -> u32 {
    10
}

fn default_refresh_mode() -> String {
    "active_only".to_string()
}

fn default_idle_timeout_secs() -> u32 {
    3600 // 1h
}

fn default_absolute_timeout_secs() -> u32 {
    3600 // 1h (hard logout po godzinie, niezależnie od aktywności)
}

fn default_warn_before_secs() -> u32 {
    60
}

// ──────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_agent_config_with_defaults() {
        let yaml = r#"
manager:
  endpoint: "192.168.64.1:5443"
  enrollment_token: "deadbeef"
  ca_cert_path: "/etc/scrooge/ca.pem"
agent:
  data_dir: "/var/lib/scrooge"
"#;
        let cfg: AgentConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.manager.endpoint, "192.168.64.1:5443");
        assert!(cfg.manager.verify_tls, "verify_tls defaults to true");
        assert_eq!(cfg.agent.log_level, "info");
        assert_eq!(cfg.agent.heartbeat_interval_secs, 30);
        cfg.validate().unwrap();
    }

    #[test]
    fn agent_config_rejects_empty_endpoint() {
        let yaml = r#"
manager:
  endpoint: ""
  ca_cert_path: "/etc/scrooge/ca.pem"
agent:
  data_dir: "/var/lib/scrooge"
"#;
        let cfg: AgentConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(
            cfg.validate(),
            Err(ConfigError::MissingField("manager.endpoint"))
        ));
    }

    #[test]
    fn agent_config_rejects_endpoint_without_port() {
        let yaml = r#"
manager:
  endpoint: "manager.example.com"
  ca_cert_path: "/etc/scrooge/ca.pem"
agent:
  data_dir: "/var/lib/scrooge"
"#;
        let cfg: AgentConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(
            cfg.validate(),
            Err(ConfigError::InvalidValue {
                field: "manager.endpoint",
                ..
            })
        ));
    }

    #[test]
    fn parses_manager_config_with_defaults() {
        let yaml = r#"
server:
  tls_cert_path: "/etc/scrooge/server.pem"
  tls_key_path: "/etc/scrooge/server.key"
  ca_cert_path: "/etc/scrooge/ca.pem"
  ca_key_path: "/etc/scrooge/ca.key"
database:
  url: "postgres://localhost/scrooge"
auth:
  jwt_secret: "this-is-a-very-long-secret-at-least-32"
"#;
        let cfg: ManagerConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.server.grpc_listen_addr, "0.0.0.0:5443");
        assert_eq!(cfg.server.rest_listen_addr, "0.0.0.0:55000");
        assert_eq!(cfg.database.pool_max, 16);
        assert!(!cfg.syslog_forwarder.enabled, "syslog disabled by default");
        assert!(cfg.syslog_forwarder.filters.respect_per_rule_flag);
        assert_eq!(cfg.auth.access_token_ttl_secs, 3600);
        cfg.validate().unwrap();
    }

    #[test]
    fn manager_config_rejects_short_jwt_secret() {
        let yaml = r#"
server:
  tls_cert_path: "x"
  tls_key_path: "x"
  ca_cert_path: "x"
  ca_key_path: "x"
database:
  url: "postgres://x"
auth:
  jwt_secret: "too-short"
"#;
        let cfg: ManagerConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(
            cfg.validate(),
            Err(ConfigError::InvalidValue {
                field: "auth.jwt_secret",
                ..
            })
        ));
    }

    #[test]
    fn parses_full_syslog_forwarder_section() {
        let yaml = r#"
server:
  tls_cert_path: "x"
  tls_key_path: "x"
  ca_cert_path: "x"
  ca_key_path: "x"
database:
  url: "postgres://x"
syslog_forwarder:
  enabled: true
  destination:
    host: "wazuh.internal"
    port: 6514
    protocol: "tcp_tls"
    tls:
      verify: true
      ca_cert_path: "/etc/scrooge/wazuh-ca.pem"
  filters:
    min_severity: "high"
    respect_per_rule_flag: false
    exclude_event_types: ["agent_internal"]
auth:
  jwt_secret: "this-is-a-very-long-secret-at-least-32-chars"
"#;
        let cfg: ManagerConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(cfg.syslog_forwarder.enabled);
        let dest = cfg.syslog_forwarder.destination.as_ref().unwrap();
        assert_eq!(dest.host, "wazuh.internal");
        assert_eq!(dest.port, 6514);
        assert_eq!(cfg.syslog_forwarder.filters.min_severity, "high");
        assert!(!cfg.syslog_forwarder.filters.respect_per_rule_flag);
        assert_eq!(
            cfg.syslog_forwarder.filters.exclude_event_types,
            vec!["agent_internal"]
        );
    }
}
