// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! `GET /health` — liveness check. No auth.

use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
}

/// `GET /health` — zwraca status liveness'u (bez kontaktu z DB).
#[utoipa::path(
    get,
    path = "/health",
    responses((status = 200, description = "Service is alive", body = HealthResponse)),
    tag = "health"
)]
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}
