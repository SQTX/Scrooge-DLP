// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! Endpointy dla agentów:
//! - `GET /api/v1/agents` — lista zarejestrowanych
//! - `POST /api/v1/agents/{id}/command` — wyślij komendę przez gRPC Stream

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::Claims,
    error::{ApiError, ApiResult},
    state::{AgentCommandRequest, AppState},
};

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

// ────────────────────────────────────────────────────────────────────────────
// POST /api/v1/agents/{id}/command — wyślij komendę do agenta (Sub-faza 2E)
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct SendCommandRequest {
    /// `refresh_policies` | `collect_diagnostics` | `update_tags`.
    pub command_type: String,
    /// Opcjonalny base64-encoded payload (zależny od command_type).
    #[serde(default)]
    pub payload_b64: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SendCommandResponse {
    /// UUID wygenerowany po stronie managera — używany w przyszłym
    /// `CommandResult.command_id` do korelacji.
    pub command_id: String,
    /// Slug command_type, echo z requestu (validated).
    pub command_type: String,
}

/// `POST /api/v1/agents/{id}/command` — fire-and-forget command do agenta.
/// Aktualnie nie czeka na `CommandResult` (TODO: Sub-faza 3 — bidirectional
/// command pattern z await na response, lub WebSocket dla live update).
#[utoipa::path(
    post,
    path = "/api/v1/agents/{id}/command",
    params(("id" = Uuid, Path, description = "Agent UUID")),
    request_body = SendCommandRequest,
    responses(
        (status = 202, description = "Command zaakceptowany, wysłany przez bridge", body = SendCommandResponse),
        (status = 400, description = "Nieznany command_type lub zły base64", body = crate::error::ErrorBody),
        (status = 404, description = "Agent nie istnieje", body = crate::error::ErrorBody),
        (status = 503, description = "Bridge channel niedostępny (manager-bin nie podał)", body = crate::error::ErrorBody),
    ),
    security(("bearer_auth" = [])),
    tag = "agents"
)]
pub async fn send_command(
    State(state): State<AppState>,
    claims: Claims,
    Path(agent_id): Path<Uuid>,
    Json(req): Json<SendCommandRequest>,
) -> ApiResult<(axum::http::StatusCode, Json<SendCommandResponse>)> {
    claims.require_admin()?;

    // Walidacja command_type — mapuje na proto::v1::CommandType jako i32.
    // Wartości muszą się zgadzać z proto enum w scrooge-proto.
    let command_type_i32: i32 = match req.command_type.as_str() {
        "refresh_policies" => 1,
        "collect_diagnostics" => 2,
        "update_tags" => 3,
        other => {
            return Err(ApiError::BadRequest(format!(
                "unknown command_type `{other}` — \
                 expected: refresh_policies / collect_diagnostics / update_tags"
            )));
        },
    };

    // Sprawdź czy agent istnieje w DB (nie zapobiega disconnected = manager-bin
    // bridge wykryje "not connected" w registry — ale podstawowy 404 here).
    let exists: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM agents WHERE id = $1")
        .bind(agent_id)
        .fetch_optional(state.pool())
        .await?;
    if exists.is_none() {
        return Err(ApiError::NotFound);
    }

    // Decode payload base64 (jeśli podany).
    let payload = if let Some(b64) = req.payload_b64.as_ref() {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| ApiError::BadRequest(format!("payload_b64 invalid: {e}")))?
    } else {
        Vec::new()
    };

    let command_id = Uuid::new_v4().to_string();

    let bridge = state
        .command_tx()
        .ok_or_else(|| ApiError::ServiceUnavailable("command bridge unavailable".to_string()))?;

    bridge
        .send(AgentCommandRequest {
            agent_id,
            command_type: command_type_i32,
            payload,
            command_id: command_id.clone(),
        })
        .await
        .map_err(|_| ApiError::ServiceUnavailable("command bridge closed".to_string()))?;

    tracing::info!(
        user = %claims.username,
        %agent_id,
        command_type = %req.command_type,
        %command_id,
        "command sent to bridge"
    );

    Ok((
        axum::http::StatusCode::ACCEPTED,
        Json(SendCommandResponse {
            command_id,
            command_type: req.command_type,
        }),
    ))
}
