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
    routing::{get, post},
    Router,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

pub use state::AppState;

/// Buduje główny router REST API.
pub fn router(state: AppState) -> Router {
    let v1 = Router::new()
        .route("/auth/login", post(auth::login))
        .route("/auth/refresh", post(auth::refresh))
        .route("/agents", get(handlers::agents::list))
        .with_state(state.clone());

    Router::new()
        .route("/health", get(handlers::health::health))
        .nest("/api/v1", v1)
        .merge(SwaggerUi::new("/api/v1/docs").url("/api/v1/openapi.json", ApiDoc::openapi()))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive()) // dev-friendly; w produkcji zwęzić.
        .with_state(state)
}

/// Startuje REST API na podanym `addr` i wraca po zakończeniu `shutdown`.
pub async fn serve(
    addr: SocketAddr,
    state: AppState,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let app = router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(addr = %addr, "REST API listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
}

// ────────────────────────────────────────────────────────────────────────────
// OpenAPI document
// ────────────────────────────────────────────────────────────────────────────

#[derive(OpenApi)]
#[openapi(
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
    ),
    components(schemas(
        handlers::health::HealthResponse,
        auth::LoginRequest,
        auth::LoginResponse,
        auth::RefreshRequest,
        handlers::agents::AgentDto,
        error::ErrorBody,
    )),
    tags(
        (name = "health",  description = "Liveness/readiness checks"),
        (name = "auth",    description = "Authentication: login + refresh"),
        (name = "agents",  description = "Agent registry"),
    )
)]
struct ApiDoc;
