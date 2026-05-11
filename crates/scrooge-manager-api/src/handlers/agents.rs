// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! `GET /api/v1/agents` — lista zarejestrowanych agentów (wymaga JWT).

use axum::{extract::State, Json};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{auth::Claims, error::ApiResult, state::AppState};

#[derive(Debug, Serialize, FromRow, ToSchema)]
pub struct AgentDto {
    pub id: Uuid,
    pub hostname: String,
    pub os: String,
    pub os_version: Option<String>,
    pub arch: Option<String>,
    pub agent_version: Option<String>,
    pub status: String,
    pub enrolled_at: DateTime<Utc>,
    pub last_seen: Option<DateTime<Utc>>,
}

/// `GET /api/v1/agents` — zwraca wszystkich zarejestrowanych agentów.
///
/// W Kroku 6 brak paginacji/filtru — dodajemy w kolejnych iteracjach.
#[utoipa::path(
    get,
    path = "/api/v1/agents",
    responses(
        (status = 200, description = "Lista agentów", body = [AgentDto]),
        (status = 401, description = "Brak/nieprawidłowy token", body = crate::error::ErrorBody),
    ),
    security(("bearer_auth" = [])),
    tag = "agents"
)]
pub async fn list(State(state): State<AppState>, claims: Claims) -> ApiResult<Json<Vec<AgentDto>>> {
    tracing::debug!(user = %claims.username, "listing agents");

    let agents: Vec<AgentDto> = sqlx::query_as(
        r"
        SELECT id, hostname, os, os_version, arch, agent_version,
               status, enrolled_at, last_seen
        FROM agents
        ORDER BY enrolled_at DESC
        ",
    )
    .fetch_all(state.pool())
    .await?;

    Ok(Json(agents))
}
