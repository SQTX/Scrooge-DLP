// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! Wazuh-flow install API:
//!
//! - `POST /api/v1/agents/install` (admin) — generuje single-use enrollment
//!   token (24h, max_uses=1) i zwraca gotowy one-liner per target_os.
//! - `GET /api/v1/install.sh?token=…`        — bash installer dla Linuxa
//!   (Debian/Ubuntu/RHEL family). Sub-faza 1B.
//! - `GET /api/v1/install-macos.sh?token=…`  — bash installer dla macOS
//!   (curl-flow, brak code signing — patrz `project_distribution_policy`).
//!   Sub-faza 1C.
//! - `GET /api/v1/install.ps1?token=…`       — PowerShell installer dla
//!   Windows (curl-flow, brak code signing). Sub-faza 1C.
//!
//! Wszystkie `GET /install*` są publiczne — autoryzację zapewnia sam
//! enrollment token (sanity-check, że istnieje i nie wygasł). uses_count
//! inkrementuje wyłącznie gRPC `Enroll`.

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

// ────────────────────────────────────────────────────────────────────────────
// Stałe
// ────────────────────────────────────────────────────────────────────────────

/// Stały TTL token'a install. Krótsze niż domyślne CLI (30 dni), bo zakładamy
/// że admin wcisnął „Add agent" i zaraz robi instalację.
const INSTALL_TOKEN_TTL_HOURS: i64 = 24;

/// Base URL dla `.deb`/`.rpm`/tarball/zip na GitHub Releases. Hardcoded —
/// repo jest publiczne, więc nie wystawiamy tego jako pole configu.
const RELEASES_BASE_URL: &str = "https://github.com/SQTX/Scrooge-DLP/releases/download";

/// Package version (workspace `Cargo.toml`) + revision `1` z `cargo-deb`
/// defaults. Wstrzykiwany do nazwy pliku `.deb`/`.rpm` w renderowanym
/// skrypcie (`scrooge-agent_{PACKAGE_VERSION}_amd64.deb`).
const PACKAGE_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "-1");

const LINUX_TEMPLATE: &str = include_str!("install_linux.sh.tpl");
const MACOS_TEMPLATE: &str = include_str!("install_macos.sh.tpl");
const WINDOWS_TEMPLATE: &str = include_str!("install_windows.ps1.tpl");

// ────────────────────────────────────────────────────────────────────────────
// TargetOs
// ────────────────────────────────────────────────────────────────────────────

/// Walidowany target OS dla install API. Każdy wariant zna swój template,
/// MIME type i format one-liner'a.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TargetOs {
    Linux,
    MacOs,
    Windows,
}

impl TargetOs {
    fn parse(raw: &str) -> Option<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "linux" => Some(Self::Linux),
            "macos" | "darwin" => Some(Self::MacOs),
            "windows" => Some(Self::Windows),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::MacOs => "macos",
            Self::Windows => "windows",
        }
    }

    fn template(self) -> &'static str {
        match self {
            Self::Linux => LINUX_TEMPLATE,
            Self::MacOs => MACOS_TEMPLATE,
            Self::Windows => WINDOWS_TEMPLATE,
        }
    }

    fn content_type(self) -> &'static str {
        match self {
            Self::Linux | Self::MacOs => "text/x-shellscript; charset=utf-8",
            // PowerShell uses `application/x-powershell`; `iwr | iex` ignoruje
            // Content-Type, ale uczciwie deklarujemy.
            Self::Windows => "application/x-powershell; charset=utf-8",
        }
    }

    /// Ścieżka do endpointa renderującego dla tego OS (bez query string).
    fn script_path(self) -> &'static str {
        match self {
            Self::Linux => "/api/v1/install.sh",
            Self::MacOs => "/api/v1/install-macos.sh",
            Self::Windows => "/api/v1/install.ps1",
        }
    }

    /// Renderuje one-liner gotowy do skopiowania. Linux/macOS używają bash'a,
    /// Windows — PowerShell przez `iwr … | iex`.
    ///
    /// **`-k` / disabled cert validation** — jednorazowy akceptowalny ryzyk
    /// przy initial bootstrap. Manager używa cert podpisanego przez nasz
    /// Root CA, ale endpoint nie ma tego CA w trust store. Po enrollment
    /// agent ma już CA wbity (embedded w install.sh) i wszystkie kolejne
    /// połączenia (gRPC) są mTLS z pełną weryfikacją. Identyczny pattern
    /// co Wazuh agent install.
    fn one_liner(self, base: &str, token: &str) -> String {
        let path = self.script_path();
        match self {
            Self::Linux | Self::MacOs => {
                format!("curl -fsSLk '{base}{path}?token={token}' | sudo bash")
            },
            // PowerShell 5.1 (default Win10/11/Server) NIE wspiera
            // `-SkipCertificateCheck` na `iwr` — musimy globalnie wyłączyć
            // walidację w session. PS 6+ ma flag, ale dla kompat. z 5.1
            // używamy ServicePointManager.
            Self::Windows => format!(
                "[Net.ServicePointManager]::ServerCertificateValidationCallback = {{$true}}; \
                 iwr -UseBasicParsing '{base}{path}?token={token}' | iex"
            ),
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// POST /api/v1/agents/install
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct InstallRequest {
    /// `linux` | `macos` | `windows`.
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

    let os = TargetOs::parse(&req.target_os).ok_or_else(|| {
        ApiError::BadRequest(format!(
            "target_os must be one of: linux, macos, windows (got: {})",
            req.target_os
        ))
    })?;

    let created_by: Uuid = claims
        .sub
        .parse()
        .map_err(|_| ApiError::Internal("sub claim is not a UUID".to_string()))?;

    let description = req
        .description
        .unwrap_or_else(|| format!("install API ({}) by {}", os.label(), claims.username));

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
        .map(|base| os.one_liner(base, &raw));

    tracing::info!(
        user = %claims.username,
        target_os = os.label(),
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
// GET handlery per OS — wszystkie wrappy nad `render_for(os, …)`
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct InstallScriptQuery {
    pub token: String,
    /// Phase 3+ — build-from-source mode. Gdy `Some(branch)`, script
    /// klonuje repo @ branch i odpala `cargo build --release` zamiast
    /// pobierać prebuilt `.deb`. Wymaga sieci + deps systemowych.
    /// Wartość = nazwa branch/tag/commit w `SQTX/Scrooge-DLP`.
    #[serde(default)]
    pub r#ref: Option<String>,
}

/// `GET /api/v1/install.sh?token=XYZ` — server-rendered bash installer (Linux).
#[utoipa::path(
    get,
    path = "/api/v1/install.sh",
    params(
        ("token" = String, Query, description = "Single-use enrollment token (UUID v4) z POST /agents/install"),
    ),
    responses(
        (status = 200, description = "Bash installer dla Linux", content_type = "text/x-shellscript"),
        (status = 401, description = "Unknown/expired token", body = crate::error::ErrorBody),
        (status = 503, description = "Manager nie ma kompletnego install configu", body = crate::error::ErrorBody),
    ),
    tag = "install"
)]
pub async fn script(
    State(state): State<AppState>,
    Query(q): Query<InstallScriptQuery>,
) -> ApiResult<Response> {
    render_for(&state, TargetOs::Linux, &q.token, q.r#ref.as_deref()).await
}

/// `GET /api/v1/install-macos.sh?token=XYZ` — bash installer dla macOS
/// (curl-flow, brak code signing).
#[utoipa::path(
    get,
    path = "/api/v1/install-macos.sh",
    params(
        ("token" = String, Query, description = "Single-use enrollment token (UUID v4)"),
    ),
    responses(
        (status = 200, description = "Bash installer dla macOS", content_type = "text/x-shellscript"),
        (status = 401, description = "Unknown/expired token", body = crate::error::ErrorBody),
        (status = 503, description = "Manager nie ma kompletnego install configu", body = crate::error::ErrorBody),
    ),
    tag = "install"
)]
pub async fn script_macos(
    State(state): State<AppState>,
    Query(q): Query<InstallScriptQuery>,
) -> ApiResult<Response> {
    render_for(&state, TargetOs::MacOs, &q.token, q.r#ref.as_deref()).await
}

/// `GET /api/v1/install.ps1?token=XYZ` — PowerShell installer dla Windows
/// (curl-flow, brak code signing).
#[utoipa::path(
    get,
    path = "/api/v1/install.ps1",
    params(
        ("token" = String, Query, description = "Single-use enrollment token (UUID v4)"),
    ),
    responses(
        (status = 200, description = "PowerShell installer dla Windows", content_type = "application/x-powershell"),
        (status = 401, description = "Unknown/expired token", body = crate::error::ErrorBody),
        (status = 503, description = "Manager nie ma kompletnego install configu", body = crate::error::ErrorBody),
    ),
    tag = "install"
)]
pub async fn script_windows(
    State(state): State<AppState>,
    Query(q): Query<InstallScriptQuery>,
) -> ApiResult<Response> {
    render_for(&state, TargetOs::Windows, &q.token, q.r#ref.as_deref()).await
}

// ────────────────────────────────────────────────────────────────────────────
// Wspólny renderer
// ────────────────────────────────────────────────────────────────────────────

/// `GET /ca.pem` — publiczny endpoint zwracający Root CA managera w PEM.
/// Brak auth (cert publiczny z definicji). Używany przez `deploy/install-agent.sh`
/// (one-liner z github raw) do bootstrap'u TLS trust przed enrollment.
pub async fn ca_pem(State(state): State<AppState>) -> ApiResult<Response> {
    let path = &state.install_config().ca_cert_path;
    let pem = std::fs::read_to_string(path).map_err(|e| {
        ApiError::Internal(format!("cannot read CA cert at {path}: {e}"))
    })?;
    let resp = (
        [(axum::http::header::CONTENT_TYPE, "application/x-pem-file")],
        pem,
    );
    Ok(resp.into_response())
}

async fn render_for(
    state: &AppState,
    os: TargetOs,
    token: &str,
    install_ref: Option<&str>,
) -> ApiResult<Response> {
    let ctx = ScriptContext::from_install_config(state.install_config())?;
    validate_token(state, token).await?;

    let ca_pem = std::fs::read_to_string(&ctx.ca_cert_path).map_err(|e| {
        ApiError::Internal(format!(
            "cannot read CA cert at {}: {e}",
            ctx.ca_cert_path.display()
        ))
    })?;

    let body = render_install_script(
        os.template(),
        &ctx,
        token,
        ca_pem.trim_end(),
        install_ref.unwrap_or(""),
    );

    tracing::info!(
        target_os = os.label(),
        release_tag = %ctx.release_tag,
        "rendered install script"
    );

    Ok(([(header::CONTENT_TYPE, os.content_type())], body).into_response())
}

/// Token sanity check — czy istnieje i nie expired. uses_count
/// inkrementuje wyłącznie Enroll (RPC).
async fn validate_token(state: &AppState, token: &str) -> ApiResult<()> {
    let token_hash = sha256_hex(token);
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
    Ok(())
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
fn render_install_script(
    template: &str,
    ctx: &ScriptContext,
    token: &str,
    ca_pem: &str,
    install_ref: &str,
) -> String {
    template
        .replace("{{MANAGER_ENDPOINT}}", &ctx.manager_endpoint)
        .replace("{{INSTALL_BASE_URL}}", &ctx.install_base_url)
        .replace("{{ENROLLMENT_TOKEN}}", token)
        .replace("{{RELEASE_TAG}}", &ctx.release_tag)
        .replace("{{PACKAGE_VERSION}}", PACKAGE_VERSION)
        .replace("{{RELEASES_BASE}}", RELEASES_BASE_URL)
        .replace("{{INSTALL_REF}}", install_ref)
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
            release_tag: "v0.1.0-rc7".to_string(),
            ca_cert_path: PathBuf::from("/tmp/ca.pem"),
        }
    }

    fn assert_no_placeholders(s: &str, label: &str) {
        assert!(!s.contains("{{"), "leftover placeholder in {label}: {s}");
    }

    #[test]
    fn linux_template_renders_all_placeholders() {
        let out = render_install_script(
            LINUX_TEMPLATE,
            &dummy_ctx(),
            "TEST-TOKEN-1234",
            "-----BEGIN CERTIFICATE-----\nfake\n-----END CERTIFICATE-----",
            "",
        );
        assert_no_placeholders(&out, "linux");
        assert!(out.contains("mgr.example.com:5443"));
        assert!(out.contains("TEST-TOKEN-1234"));
        assert!(out.contains("v0.1.0-rc7"));
        assert!(out.contains(PACKAGE_VERSION));
    }

    #[test]
    fn linux_template_with_install_ref_includes_build_from_source() {
        let out = render_install_script(
            LINUX_TEMPLATE,
            &dummy_ctx(),
            "REF-TOKEN-1234",
            "-----BEGIN CERTIFICATE-----\nfake\n-----END CERTIFICATE-----",
            "claude/wizardly-fermi-ae7494",
        );
        assert_no_placeholders(&out, "linux+ref");
        assert!(out.contains("claude/wizardly-fermi-ae7494"));
        assert!(out.contains("build-from-source"));
        assert!(out.contains("cargo build"));
    }

    #[test]
    fn macos_template_renders_all_placeholders() {
        let out = render_install_script(
            MACOS_TEMPLATE,
            &dummy_ctx(),
            "MAC-TOKEN-5678",
            "-----BEGIN CERTIFICATE-----\nmac-fake\n-----END CERTIFICATE-----",
            "",
        );
        assert_no_placeholders(&out, "macos");
        assert!(out.contains("mgr.example.com:5443"));
        assert!(out.contains("MAC-TOKEN-5678"));
        assert!(out.contains("apple-darwin"));
        assert!(out.contains("launchctl"));
    }

    #[test]
    fn windows_template_renders_all_placeholders() {
        let out = render_install_script(
            WINDOWS_TEMPLATE,
            &dummy_ctx(),
            "WIN-TOKEN-9999",
            "-----BEGIN CERTIFICATE-----\nwin-fake\n-----END CERTIFICATE-----",
            "",
        );
        assert_no_placeholders(&out, "windows");
        assert!(out.contains("mgr.example.com:5443"));
        assert!(out.contains("WIN-TOKEN-9999"));
        assert!(out.contains("pc-windows-msvc"));
        assert!(out.contains("New-Service") || out.contains("sc.exe"));
    }

    #[test]
    fn one_liner_per_os() {
        let base = "https://mgr.example.com";
        let tok = "abc";
        let lin = TargetOs::Linux.one_liner(base, tok);
        let mac = TargetOs::MacOs.one_liner(base, tok);
        let win = TargetOs::Windows.one_liner(base, tok);

        // Linux/macOS curl z -k (skip cert verify dla self-signed manager).
        assert!(lin.starts_with("curl -fsSLk"));
        assert!(lin.contains("/install.sh"));
        assert!(mac.starts_with("curl -fsSLk"));
        assert!(mac.contains("/install-macos.sh"));
        // Windows: ServicePointManager bypass + iwr.
        assert!(win.contains("ServerCertificateValidationCallback"));
        assert!(win.contains("iwr"));
        assert!(win.contains("/install.ps1"));
        assert!(win.contains("| iex"));
    }

    #[test]
    fn target_os_parse_accepts_aliases() {
        assert_eq!(TargetOs::parse("Linux"), Some(TargetOs::Linux));
        assert_eq!(TargetOs::parse("MACOS"), Some(TargetOs::MacOs));
        assert_eq!(TargetOs::parse("darwin"), Some(TargetOs::MacOs));
        assert_eq!(TargetOs::parse("Windows"), Some(TargetOs::Windows));
        assert_eq!(TargetOs::parse("freebsd"), None);
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
