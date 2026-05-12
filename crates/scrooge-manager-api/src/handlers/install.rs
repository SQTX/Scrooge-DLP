// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! `POST /api/v1/agents/install` — admin generuje single-use enrollment
//! token (24h TTL, max_uses=1) potrzebny do automatycznej instalacji agenta.
//!
//! Sub-faza 1B krok 1. Endpoint zwraca raw token + expiry. Pełen one-liner
//! z `curl … | sudo bash` doklejemy w kroku 3 (`GET /install.sh`) — wtedy
//! response rozszerzymy o `install_command`.

use axum::{extract::State, Json};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::Claims,
    error::{ApiError, ApiResult},
    state::AppState,
};

/// Walidowane target_os. Renderowanie installer'a per OS dochodzi w
/// kolejnych krokach Sub-fazy 1B/1C.
const SUPPORTED_OS: &[&str] = &["linux", "macos", "windows"];

/// Stały TTL token'a install. Krótsze niż domyślne CLI (30 dni), bo zakładamy
/// że admin wcisnął „Add agent" i zaraz robi instalację.
const INSTALL_TOKEN_TTL_HOURS: i64 = 24;

#[derive(Debug, Deserialize, ToSchema)]
pub struct InstallRequest {
    /// `linux` | `macos` | `windows`. Renderowanie installer'a per OS dochodzi
    /// w kolejnych krokach — w MVP token jest agnostic.
    pub target_os: String,
    /// Opcjonalny opis (np. `laptop-jan-marketing`). Trafia do
    /// `enrollment_tokens.description` jako pomoc audytowa.
    pub description: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InstallResponse {
    /// Raw enrollment token (UUID v4). Pokazywany tylko raz — w DB trzymamy
    /// wyłącznie SHA-256 hash.
    pub token: String,
    /// Wygaśnięcie (RFC 3339).
    pub expires_at: DateTime<Utc>,
}

/// `POST /api/v1/agents/install` — admin issuuje single-use token, krok 1
/// Wazuh-flow. Endpoint nie renderuje jeszcze `install_command` — to czeka
/// na `/install.sh` w kolejnym kroku.
#[utoipa::path(
    post,
    path = "/api/v1/agents/install",
    request_body = InstallRequest,
    responses(
        (status = 200, description = "Token + expiry",     body = InstallResponse),
        (status = 400, description = "Invalid target_os",  body = crate::error::ErrorBody),
        (status = 401, description = "Not authenticated",  body = crate::error::ErrorBody),
        (status = 403, description = "Caller is not admin", body = crate::error::ErrorBody),
    ),
    security(("bearer_auth" = [])),
    tag = "install"
)]
pub async fn create(
    State(state): State<AppState>,
    claims: Claims,
    Json(req): Json<InstallRequest>,
) -> ApiResult<Json<InstallResponse>> {
    claims.require_admin()?;

    let os = req.target_os.to_lowercase();
    if !SUPPORTED_OS.contains(&os.as_str()) {
        return Err(ApiError::BadRequest(format!(
            "target_os must be one of: linux, macos, windows (got: {os})"
        )));
    }

    let created_by: Uuid = claims
        .sub
        .parse()
        .map_err(|_| ApiError::Internal("sub claim is not a UUID".to_string()))?;

    let description = req
        .description
        .unwrap_or_else(|| format!("install API ({os}) by {}", claims.username));

    let raw = Uuid::new_v4().to_string();
    let hash = sha256_hex(&raw);
    let expires_at = Utc::now() + Duration::hours(INSTALL_TOKEN_TTL_HOURS);
    let max_uses: i32 = 1;

    sqlx::query(
        "INSERT INTO enrollment_tokens \
         (token_hash, description, expires_at, max_uses, created_by) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&hash)
    .bind(&description)
    .bind(expires_at)
    .bind(max_uses)
    .bind(created_by)
    .execute(state.pool())
    .await?;

    tracing::info!(
        user = %claims.username,
        target_os = %os,
        expires_at = %expires_at,
        "install enrollment token issued"
    );

    Ok(Json(InstallResponse {
        token: raw,
        expires_at,
    }))
}

fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}
