// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! Wazuh-flow install API (Sub-faza 1B):
//!
//! - `POST /api/v1/agents/install` — admin generuje single-use enrollment
//!   token (24h TTL, max_uses=1) i dostaje one-liner do skopiowania na
//!   endpoincie.
//! - `GET /api/v1/install.sh?token=XYZ` — server-rendered bash, wstrzykuje
//!   gRPC endpoint, root CA i token; pobiera `.deb`/`.rpm` z GitHub Releases
//!   i podpina agenta pod systemd. Public (bez Bearer) — autoryzacja idzie
//!   przez sam enrollment token.

use std::path::PathBuf;

use axum::{
    extract::{Query, State},
    http::header,
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::Claims,
    error::{ApiError, ApiResult},
    state::{AppState, InstallConfig},
};

/// Walidowane target_os. Renderowanie installer'a per OS dochodzi w
/// kolejnych krokach Sub-fazy 1B/1C.
const SUPPORTED_OS: &[&str] = &["linux", "macos", "windows"];

/// Stały TTL token'a install. Krótsze niż domyślne CLI (30 dni), bo zakładamy
/// że admin wcisnął „Add agent" i zaraz robi instalację.
const INSTALL_TOKEN_TTL_HOURS: i64 = 24;

/// Base URL dla `.deb`/`.rpm` na GitHub Releases. Hardcoded — repo jest
/// publiczne, więc nie wystawiamy tego jako pole configu.
const RELEASES_BASE_URL: &str = "https://github.com/SQTX/Scrooge-DLP/releases/download";

/// Package version (workspace `Cargo.toml`) + revision `1` z `cargo-deb`
/// defaults. Wstrzykiwany do nazwy pliku `.deb`/`.rpm` w renderowanym
/// skrypcie (`scrooge-agent_{PACKAGE_VERSION}_amd64.deb`).
const PACKAGE_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "-1");

/// Template Linux installer'a — Debian/Ubuntu + RHEL-family. Renderowane
/// per request przez podmianę `{{PLACEHOLDER}}` tokenów.
const LINUX_TEMPLATE: &str = include_str!("install_linux.sh.tpl");

// ────────────────────────────────────────────────────────────────────────────
// POST /api/v1/agents/install
// ────────────────────────────────────────────────────────────────────────────

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
    /// Gotowy one-liner do skopiowania na endpoincie. `None` gdy
    /// `manager.server.public_rest_base_url` nie jest ustawiony.
    pub install_command: Option<String>,
}

/// `POST /api/v1/agents/install` — admin issuuje single-use token i dostaje
/// gotowy one-liner do skopiowania na endpoincie.
#[utoipa::path(
    post,
    path = "/api/v1/agents/install",
    request_body = InstallRequest,
    responses(
        (status = 200, description = "Token + one-liner",   body = InstallResponse),
        (status = 400, description = "Invalid target_os",   body = crate::error::ErrorBody),
        (status = 401, description = "Not authenticated",   body = crate::error::ErrorBody),
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

    let install_command = state
        .install_config()
        .public_rest_base_url
        .as_ref()
        .map(|base| format!("curl -fsSL '{base}/api/v1/install.sh?token={raw}' | sudo bash"));

    tracing::info!(
        user = %claims.username,
        target_os = %os,
        expires_at = %expires_at,
        "install enrollment token issued"
    );

    Ok(Json(InstallResponse {
        token: raw,
        expires_at,
        install_command,
    }))
}

// ────────────────────────────────────────────────────────────────────────────
// GET /api/v1/install.sh
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct InstallScriptQuery {
    pub token: String,
}

/// `GET /api/v1/install.sh?token=XYZ` — server-rendered bash installer.
///
/// Endpoint **nie inkrementuje** `uses_count` — robi to gRPC `Enroll`. Tutaj
/// tylko sanity-check że token istnieje i nie wygasł, żeby nie wystawiać
/// skryptu który i tak będzie odrzucony przy enrollment.
#[utoipa::path(
    get,
    path = "/api/v1/install.sh",
    params(
        ("token" = String, Query, description = "Single-use enrollment token (UUID v4) z POST /agents/install"),
    ),
    responses(
        (status = 200, description = "Bash installer", content_type = "text/x-shellscript"),
        (status = 401, description = "Unknown/expired token", body = crate::error::ErrorBody),
        (status = 503, description = "Manager nie ma kompletnego install configu", body = crate::error::ErrorBody),
    ),
    tag = "install"
)]
pub async fn script(
    State(state): State<AppState>,
    Query(q): Query<InstallScriptQuery>,
) -> ApiResult<Response> {
    let install = state.install_config();
    let ctx = ScriptContext::from_install_config(install)?;

    // Sanity check tokena — czy istnieje i nie expired. uses_count
    // przekraczać może tylko Enroll.
    let token_hash = sha256_hex(&q.token);
    let row: Option<(Option<DateTime<Utc>>,)> =
        sqlx::query_as("SELECT expires_at FROM enrollment_tokens WHERE token_hash = $1")
            .bind(&token_hash)
            .fetch_optional(state.pool())
            .await?;
    let row = row.ok_or(ApiError::InvalidToken)?;
    if let Some(expires_at) = row.0 {
        if expires_at <= Utc::now() {
            return Err(ApiError::InvalidToken);
        }
    }

    let ca_pem = std::fs::read_to_string(&ctx.ca_cert_path).map_err(|e| {
        ApiError::Internal(format!(
            "cannot read CA cert at {}: {e}",
            ctx.ca_cert_path.display()
        ))
    })?;

    let body = render_install_script(LINUX_TEMPLATE, &ctx, &q.token, ca_pem.trim_end());

    tracing::info!(
        target_os = "linux",
        release_tag = %ctx.release_tag,
        "rendered install.sh"
    );

    Ok((
        [(header::CONTENT_TYPE, "text/x-shellscript; charset=utf-8")],
        body,
    )
        .into_response())
}

// ────────────────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────────────────

/// Validated install config — wszystkie wymagane pola obecne.
#[derive(Debug)]
struct ScriptContext {
    manager_endpoint: String,
    install_base_url: String,
    release_tag: String,
    ca_cert_path: PathBuf,
}

impl ScriptContext {
    fn from_install_config(cfg: &InstallConfig) -> ApiResult<Self> {
        let missing: Vec<&str> = [
            (
                "server.public_grpc_endpoint",
                cfg.public_grpc_endpoint.as_ref(),
            ),
            (
                "server.public_rest_base_url",
                cfg.public_rest_base_url.as_ref(),
            ),
            ("server.agent_release_tag", cfg.agent_release_tag.as_ref()),
        ]
        .iter()
        .filter_map(|(name, value)| if value.is_none() { Some(*name) } else { None })
        .collect();

        if !missing.is_empty() {
            return Err(ApiError::ServiceUnavailable(format!(
                "install API requires {} to be set in manager.yaml",
                missing.join(", "),
            )));
        }

        Ok(Self {
            manager_endpoint: cfg.public_grpc_endpoint.clone().unwrap(),
            install_base_url: cfg.public_rest_base_url.clone().unwrap(),
            release_tag: cfg.agent_release_tag.clone().unwrap(),
            ca_cert_path: PathBuf::from(&cfg.ca_cert_path),
        })
    }
}

/// Pure-function renderowanie. Wydzielone do unit testu — nie dotyka I/O.
fn render_install_script(template: &str, ctx: &ScriptContext, token: &str, ca_pem: &str) -> String {
    template
        .replace("{{MANAGER_ENDPOINT}}", &ctx.manager_endpoint)
        .replace("{{INSTALL_BASE_URL}}", &ctx.install_base_url)
        .replace("{{ENROLLMENT_TOKEN}}", token)
        .replace("{{RELEASE_TAG}}", &ctx.release_tag)
        .replace("{{PACKAGE_VERSION}}", PACKAGE_VERSION)
        .replace("{{RELEASES_BASE}}", RELEASES_BASE_URL)
        .replace("{{CA_PEM}}", ca_pem)
}

fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_ctx() -> ScriptContext {
        ScriptContext {
            manager_endpoint: "mgr.example.com:5443".to_string(),
            install_base_url: "https://mgr.example.com".to_string(),
            release_tag: "v0.1.0-rc3".to_string(),
            ca_cert_path: PathBuf::from("/tmp/ca.pem"),
        }
    }

    #[test]
    fn renders_all_placeholders() {
        let out = render_install_script(
            LINUX_TEMPLATE,
            &dummy_ctx(),
            "TEST-TOKEN-1234",
            "-----BEGIN CERTIFICATE-----\nfake\n-----END CERTIFICATE-----",
        );
        assert!(!out.contains("{{"), "leftover placeholder: {out}");
        assert!(out.contains("mgr.example.com:5443"));
        assert!(out.contains("https://mgr.example.com"));
        assert!(out.contains("TEST-TOKEN-1234"));
        assert!(out.contains("v0.1.0-rc3"));
        assert!(out.contains("-----BEGIN CERTIFICATE-----"));
        assert!(out.contains(PACKAGE_VERSION));
    }

    #[test]
    fn from_install_config_reports_missing_fields() {
        let cfg = InstallConfig {
            public_grpc_endpoint: None,
            public_rest_base_url: Some("https://x".to_string()),
            agent_release_tag: None,
            ca_cert_path: "/tmp/ca.pem".to_string(),
        };
        let err = ScriptContext::from_install_config(&cfg).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("public_grpc_endpoint"));
        assert!(msg.contains("agent_release_tag"));
        assert!(!msg.contains("public_rest_base_url"));
    }
}
