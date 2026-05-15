// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! Shared application state przekazywany do handler'ów axum.

use std::sync::Arc;

use jsonwebtoken::{DecodingKey, EncodingKey};
use scrooge_common::config::{AuthConfig, DashboardConfig, ManagerServerConfig};
use sqlx::PgPool;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Stan współdzielony między handlerami.
///
/// `Arc<Inner>` żeby clone był tani (jeden refcount bump).
#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    pool: PgPool,
    auth: AuthMaterial,
    dashboard: DashboardConfig,
    install: InstallConfig,
    /// Bridge do manager-bin: REST handler `send_command` wpycha tu
    /// żądania, background task w manager-bin odbiera i wysyła przez gRPC
    /// Stream do connected agentów. `None` = manager-bin nie wystawia
    /// bridge'a (np. testy unit'owe AppState).
    command_tx: Option<mpsc::Sender<AgentCommandRequest>>,
}

/// Żądanie wysłania komendy z REST do agenta (manager-bin bridge).
#[derive(Debug)]
pub struct AgentCommandRequest {
    pub agent_id: Uuid,
    /// `proto::v1::CommandType` jako i32 (REFRESH_POLICIES=1 itd.).
    pub command_type: i32,
    pub payload: Vec<u8>,
    /// Wygenerowany przez REST handler — manager-bin wpisuje w Command.command_id.
    pub command_id: String,
}

/// Pola configu managera potrzebne przez install API (Sub-faza 1B).
/// Wszystkie opcjonalne (poza `ca_cert_path`) — `/install.sh` zwraca 503
/// gdy któreś jest puste, żeby nie wystawiać niedziałającego one-linera.
#[derive(Debug, Clone)]
pub struct InstallConfig {
    pub public_grpc_endpoint: Option<String>,
    pub public_rest_base_url: Option<String>,
    pub agent_release_tag: Option<String>,
    pub ca_cert_path: String,
}

pub(crate) struct AuthMaterial {
    pub(crate) encoding: EncodingKey,
    pub(crate) decoding: DecodingKey,
    pub(crate) access_ttl_secs: u64,
    pub(crate) refresh_ttl_secs: u64,
}

impl AppState {
    #[must_use]
    pub fn new(
        pool: PgPool,
        auth_cfg: &AuthConfig,
        dashboard: DashboardConfig,
        server_cfg: &ManagerServerConfig,
        command_tx: Option<mpsc::Sender<AgentCommandRequest>>,
    ) -> Self {
        let secret = auth_cfg.jwt_secret.as_bytes();
        let auth = AuthMaterial {
            encoding: EncodingKey::from_secret(secret),
            decoding: DecodingKey::from_secret(secret),
            access_ttl_secs: auth_cfg.access_token_ttl_secs,
            refresh_ttl_secs: auth_cfg.refresh_token_ttl_secs,
        };
        let install = InstallConfig {
            public_grpc_endpoint: server_cfg.public_grpc_endpoint.clone(),
            public_rest_base_url: server_cfg.public_rest_base_url.clone(),
            agent_release_tag: server_cfg.agent_release_tag.clone(),
            ca_cert_path: server_cfg.ca_cert_path.clone(),
        };
        Self {
            inner: Arc::new(Inner {
                pool,
                auth,
                dashboard,
                install,
                command_tx,
            }),
        }
    }

    /// Bridge do manager-bin do wysyłania komend agentom. `None` gdy
    /// AppState skonstruowany bez bridge (rzadkie).
    #[must_use]
    pub fn command_tx(&self) -> Option<&mpsc::Sender<AgentCommandRequest>> {
        self.inner.command_tx.as_ref()
    }

    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.inner.pool
    }

    pub(crate) fn auth(&self) -> &AuthMaterial {
        &self.inner.auth
    }

    #[must_use]
    pub fn dashboard_config(&self) -> &DashboardConfig {
        &self.inner.dashboard
    }

    #[must_use]
    pub fn install_config(&self) -> &InstallConfig {
        &self.inner.install
    }
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Nie eksponujemy klucza JWT w Debug.
        f.debug_struct("AppState")
            .field("pool", &"<PgPool>")
            .finish_non_exhaustive()
    }
}
