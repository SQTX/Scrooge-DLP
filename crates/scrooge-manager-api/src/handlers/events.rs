// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! Phase 3 — odczyt eventów DLP z tabeli `events` (partycjonowanej po
//! timestamp). REST endpoint zasila dashboard "Events" tab.
//!
//! `POST` (zapis eventów) idzie przez gRPC Stream — patrz
//! `scrooge-manager-bin/src/grpc.rs::insert_event_batch`.

use axum::{
    extract::{Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::FromRow;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    auth::Claims,
    error::{ApiError, ApiResult},
    state::AppState,
};

/// Pojedynczy event w odpowiedzi REST. Bezpośrednia projekcja `events`
/// table z M1 (partycjonowana). Pola opisane w spec sekcji 3.
#[derive(Debug, Serialize, FromRow, ToSchema)]
pub struct EventDto {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub agent_id: Uuid,
    pub event_type: String,
    pub severity: String,
    pub direction: Option<String>,
    pub matched_policy: Option<String>,
    pub matched_rule: Option<String>,
    pub action_taken: Option<String>,
    /// JSONB z dashboardowych pól — `clipboard`, `matches`, `user_name`,
    /// `process_*`. Frontend traktuje jako nieprzezroczystą strukturę.
    #[schema(value_type = Object)]
    pub details: JsonValue,
}

/// Filtry query string'a. Wszystkie opcjonalne — brak = match wszystkich.
#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct EventQuery {
    pub agent_id: Option<Uuid>,
    /// "low" / "medium" / "high" / "critical".
    pub severity: Option<String>,
    /// Filtr po typie eventu (np. "CLIPBOARD_COPY").
    pub event_type: Option<String>,
    /// ISO 8601 — `since <= timestamp`.
    pub since: Option<DateTime<Utc>>,
    /// `timestamp <= until`.
    pub until: Option<DateTime<Utc>>,
    /// Domyślnie 100, max 1000.
    pub limit: Option<u32>,
    /// Offset (paginacja); domyślnie 0.
    pub offset: Option<u32>,
}

/// `GET /api/v1/events?...` — lista eventów (DESC po timestamp).
#[utoipa::path(
    get,
    path = "/api/v1/events",
    params(EventQuery),
    responses(
        (status = 200, description = "Lista eventów", body = [EventDto]),
        (status = 401, description = "Brak/zły token", body = crate::error::ErrorBody),
    ),
    security(("bearer_auth" = [])),
    tag = "events"
)]
pub async fn list(
    State(state): State<AppState>,
    claims: Claims,
    Query(q): Query<EventQuery>,
) -> ApiResult<Json<Vec<EventDto>>> {
    // Odczyt eventów dozwolony dla każdego zalogowanego usera (czyli admina
    // w MVP — bo enroll/login wystawia tylko admin tokens). W Phase 5+
    // wjedzie role-based access (viewer/auditor).
    claims.require_admin()?;

    let limit = i64::from(q.limit.unwrap_or(100).min(1000));
    let offset = i64::from(q.offset.unwrap_or(0));

    // sqlx::query_as nie ma dynamic WHERE — używamy `QueryBuilder`.
    // M1 events table partycjonowana po timestamp → DESC po timestamp jest
    // szybkie (najnowsze partycje sequential scan).
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT id, timestamp, agent_id, event_type, severity, direction, \
                matched_policy, matched_rule, action_taken, details \
         FROM events WHERE 1=1",
    );
    if let Some(aid) = q.agent_id {
        qb.push(" AND agent_id = ").push_bind(aid);
    }
    if let Some(s) = &q.severity {
        if !["low", "medium", "high", "critical"].contains(&s.as_str()) {
            return Err(ApiError::BadRequest(format!("invalid severity: {s}")));
        }
        qb.push(" AND severity = ").push_bind(s);
    }
    if let Some(et) = &q.event_type {
        qb.push(" AND event_type = ").push_bind(et);
    }
    if let Some(t) = q.since {
        qb.push(" AND timestamp >= ").push_bind(t);
    }
    if let Some(t) = q.until {
        qb.push(" AND timestamp <= ").push_bind(t);
    }
    qb.push(" ORDER BY timestamp DESC LIMIT ").push_bind(limit);
    qb.push(" OFFSET ").push_bind(offset);

    let rows: Vec<EventDto> = qb.build_query_as().fetch_all(state.pool()).await?;
    Ok(Json(rows))
}
