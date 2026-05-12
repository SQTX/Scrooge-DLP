// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! `GET /api/v1/dashboard/config` — zwraca preferencje dashboardu z `manager.yaml`.
//!
//! Endpoint **bez auth** — frontend musi znać konfigurację (refresh interval,
//! session timeouts) przed loginem, żeby login screen miał spójne UX. Wartości
//! są niewrażliwe (timeouty + mode), więc info-disclosure jest pomijalne.

use axum::{extract::State, Json};
use scrooge_common::config::{AutoRefreshConfig, DashboardConfig, SessionConfig};
use serde::Serialize;
use utoipa::ToSchema;

use crate::state::AppState;

#[derive(Debug, Serialize, ToSchema)]
pub struct DashboardConfigDto {
    pub auto_refresh: AutoRefreshDto,
    pub session: SessionDto,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AutoRefreshDto {
    pub enabled: bool,
    pub interval_secs: u32,
    /// `"always"` | `"active_only"` | `"off"`
    pub mode: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SessionDto {
    pub idle_timeout_secs: u32,
    pub absolute_timeout_secs: u32,
    pub warn_before_secs: u32,
    pub extend_on_activity: bool,
}

impl From<&DashboardConfig> for DashboardConfigDto {
    fn from(c: &DashboardConfig) -> Self {
        Self {
            auto_refresh: (&c.auto_refresh).into(),
            session: (&c.session).into(),
        }
    }
}

impl From<&AutoRefreshConfig> for AutoRefreshDto {
    fn from(c: &AutoRefreshConfig) -> Self {
        Self {
            enabled: c.enabled,
            interval_secs: c.interval_secs,
            mode: c.mode.clone(),
        }
    }
}

impl From<&SessionConfig> for SessionDto {
    fn from(c: &SessionConfig) -> Self {
        Self {
            idle_timeout_secs: c.idle_timeout_secs,
            absolute_timeout_secs: c.absolute_timeout_secs,
            warn_before_secs: c.warn_before_secs,
            extend_on_activity: c.extend_on_activity,
        }
    }
}

/// `GET /api/v1/dashboard/config` — zwraca preferencje dashboardu (no auth).
#[utoipa::path(
    get,
    path = "/api/v1/dashboard/config",
    responses(
        (status = 200, description = "Dashboard preferencje", body = DashboardConfigDto),
    ),
    tag = "dashboard"
)]
pub async fn config(State(state): State<AppState>) -> Json<DashboardConfigDto> {
    Json(state.dashboard_config().into())
}
