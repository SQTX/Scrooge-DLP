// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! Shared application state przekazywany do handler'ów axum.

use std::sync::Arc;

use jsonwebtoken::{DecodingKey, EncodingKey};
use scrooge_common::config::{AuthConfig, DashboardConfig};
use sqlx::PgPool;

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
}

pub(crate) struct AuthMaterial {
    pub(crate) encoding: EncodingKey,
    pub(crate) decoding: DecodingKey,
    pub(crate) access_ttl_secs: u64,
    pub(crate) refresh_ttl_secs: u64,
}

impl AppState {
    #[must_use]
    pub fn new(pool: PgPool, auth_cfg: &AuthConfig, dashboard: DashboardConfig) -> Self {
        let secret = auth_cfg.jwt_secret.as_bytes();
        let auth = AuthMaterial {
            encoding: EncodingKey::from_secret(secret),
            decoding: DecodingKey::from_secret(secret),
            access_ttl_secs: auth_cfg.access_token_ttl_secs,
            refresh_ttl_secs: auth_cfg.refresh_token_ttl_secs,
        };
        Self {
            inner: Arc::new(Inner {
                pool,
                auth,
                dashboard,
            }),
        }
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
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Nie eksponujemy klucza JWT w Debug.
        f.debug_struct("AppState")
            .field("pool", &"<PgPool>")
            .finish_non_exhaustive()
    }
}
