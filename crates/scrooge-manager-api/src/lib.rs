// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 2 as
// published by the Free Software Foundation.

//! `scrooge-manager-api` — REST API i WebSocket dla managera ScroogeDLP.
//!
//! Architektura:
//! - axum 0.7 router
//! - JWT HS256 dla auth (login + refresh)
//! - sqlx PostgreSQL pool przez [`AppState`]
//! - OpenAPI 3.0 spec generowany przez utoipa
//! - Swagger UI na `/api/v1/docs`
//!
//! Wejście: [`serve`] startuje serwer na podanym adresie z graceful
//! shutdown przez podany future.

pub mod auth;
pub mod error;
pub mod handlers;
pub mod state;

use std::net::SocketAddr;

use axum::{
    response::Html,
    routing::{get, post},
    Router,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use utoipa::{
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
    Modify, OpenApi,
};
use utoipa_swagger_ui::SwaggerUi;

pub use state::{AgentCommandRequest, AppState};

/// Embedded dashboard HTML — minimalistyczny SPA (login + agents list).
/// `include_str!` wpisuje treść pliku w binarkę przy compile time, więc
/// `scrooge-manager` jest self-contained (zero filesystem deps).
const DASHBOARD_HTML: &str = include_str!("../static/index.html");

async fn dashboard() -> Html<&'static str> {
    Html(DASHBOARD_HTML)
}

/// Buduje główny router REST API.
pub fn router(state: AppState) -> Router {
    let v1 = Router::new()
        .route("/auth/login", post(auth::login))
        .route("/auth/refresh", post(auth::refresh))
        .route("/agents", get(handlers::agents::list))
        .route("/agents/:id/command", post(handlers::agents::send_command))
        .route("/agents/install", post(handlers::install::create))
        .route("/install.sh", get(handlers::install::script))
        .route("/install-macos.sh", get(handlers::install::script_macos))
        .route("/install.ps1", get(handlers::install::script_windows))
        .route(
            "/policies",
            get(handlers::policies::list).post(handlers::policies::create),
        )
        .route(
            "/policies/:name",
            get(handlers::policies::get)
                .put(handlers::policies::update)
                .delete(handlers::policies::delete_),
        )
        .route("/policies/:name/history", get(handlers::policies::history))
        .route(
            "/policies/:name/rollback/:version",
            post(handlers::policies::rollback),
        )
        .route("/policies/validate", post(handlers::policies::validate))
        .route("/dashboard/config", get(handlers::dashboard::config))
        .with_state(state.clone());

    Router::new()
        .route("/", get(dashboard))
        .route("/dashboard", get(dashboard))
        .route(
            "/static/policy-editor/*path",
            get(handlers::policy_editor::asset),
        )
        .route("/health", get(handlers::health::health))
        .nest("/api/v1", v1)
        .merge(SwaggerUi::new("/api/v1/docs").url("/api/v1/openapi.json", ApiDoc::openapi()))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive()) // dev-friendly; w produkcji zwęzić.
        .with_state(state)
}

/// Opcjonalna konfiguracja TLS dla REST API. Gdy `None`, manager nasłuchuje
/// plain HTTP (dev mode / kompat. wsteczna). Gdy `Some`, HTTPS przez
/// axum-server + rustls.
#[derive(Debug, Clone)]
pub struct RestTlsConfig {
    pub cert_path: std::path::PathBuf,
    pub key_path: std::path::PathBuf,
}

/// Startuje REST API na podanym `addr` i wraca po zakończeniu `shutdown`.
///
/// Gdy `tls` jest `Some`, wystawia HTTPS (rustls). Gdy `None`, plain HTTP.
pub async fn serve(
    addr: SocketAddr,
    state: AppState,
    tls: Option<RestTlsConfig>,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let app = router(state);

    if let Some(tls_cfg) = tls {
        tracing::info!(addr = %addr, "REST API listening (HTTPS/TLS)");
        let rustls_cfg = axum_server::tls_rustls::RustlsConfig::from_pem_file(
            &tls_cfg.cert_path,
            &tls_cfg.key_path,
        )
        .await
        .map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "loading REST TLS cert/key ({}, {}): {e}",
                    tls_cfg.cert_path.display(),
                    tls_cfg.key_path.display()
                ),
            )
        })?;
        let handle = axum_server::Handle::new();
        let handle_for_shutdown = handle.clone();
        tokio::spawn(async move {
            shutdown.await;
            // graceful_shutdown(None) = nieskończony grace period; dla
            // dev wystarczy hard close po użytkowniku.
            handle_for_shutdown.shutdown();
        });
        axum_server::bind_rustls(addr, rustls_cfg)
            .handle(handle)
            .serve(app.into_make_service())
            .await
    } else {
        tracing::info!(
            addr = %addr,
            "REST API listening (plain HTTP — set server.rest_tls_* w manager.yaml dla HTTPS)"
        );
        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown)
            .await
    }
}

// ────────────────────────────────────────────────────────────────────────────
// OpenAPI document
// ────────────────────────────────────────────────────────────────────────────

/// Rejestruje `bearer_auth` jako security scheme w OpenAPI spec.
/// Bez tego Swagger UI nie wie jak prosić o token (przycisk "Authorize" nie reaguje).
struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer_auth",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .description(Some(
                        "Wklej `access_token` z `POST /api/v1/auth/login`. \
                         Swagger sam dorzuci prefix `Bearer ` — wklej **sam token**.",
                    ))
                    .build(),
            ),
        );
    }
}

#[derive(OpenApi)]
#[openapi(
    modifiers(&SecurityAddon),
    info(
        title = "ScroogeDLP Manager API",
        version = "0.1.0",
        description = "REST API for the ScroogeDLP manager (Krok 6 — minimal)."
    ),
    paths(
        handlers::health::health,
        auth::login,
        auth::refresh,
        handlers::agents::list,
        handlers::agents::send_command,
        handlers::install::create,
        handlers::install::script,
        handlers::install::script_macos,
        handlers::install::script_windows,
        handlers::policies::list,
        handlers::policies::get,
        handlers::policies::create,
        handlers::policies::update,
        handlers::policies::delete_,
        handlers::policies::history,
        handlers::policies::rollback,
        handlers::policies::validate,
        handlers::dashboard::config,
    ),
    components(schemas(
        handlers::health::HealthResponse,
        auth::LoginRequest,
        auth::LoginResponse,
        auth::RefreshRequest,
        handlers::agents::AgentDto,
        handlers::agents::SendCommandRequest,
        handlers::agents::SendCommandResponse,
        handlers::install::InstallRequest,
        handlers::install::InstallResponse,
        handlers::policies::PolicySummaryDto,
        handlers::policies::PolicyDto,
        handlers::policies::PolicyHistoryEntryDto,
        handlers::policies::CreatePolicyRequest,
        handlers::policies::UpdatePolicyRequest,
        handlers::policies::ValidateRequest,
        handlers::policies::ValidateResponse,
        handlers::dashboard::DashboardConfigDto,
        handlers::dashboard::AutoRefreshDto,
        handlers::dashboard::SessionDto,
        error::ErrorBody,
    )),
    tags(
        (name = "health",    description = "Liveness/readiness checks"),
        (name = "auth",      description = "Authentication: login + refresh"),
        (name = "agents",    description = "Agent registry"),
        (name = "install",   description = "Wazuh-flow agent installer tokens"),
        (name = "policies",  description = "DLP policies CRUD + history + rollback + validate"),
        (name = "dashboard", description = "Dashboard preferences"),
    )
)]
struct ApiDoc;
