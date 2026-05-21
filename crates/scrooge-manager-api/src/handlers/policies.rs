// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! REST CRUD dla polityk DLP (Sub-faza 2B).
//!
//! **Workflow:**
//! 1. Admin pisze YAML w dashboard editor lub przez `curl POST`
//! 2. Manager parsuje + waliduje przez `scrooge-policy::Policy::from_yaml`
//! 3. Jeśli OK: compile do msgpack + SHA-256, INSERT do `policies` (+ wpis
//!    do `policy_history` z `version` z YAML)
//! 4. Update: bumpuje policies.version, append do history, recompile
//! 5. Delete: usuwa z policies (history zostaje — append-only)
//! 6. Rollback: kopiuje content_yaml ze starszej wersji z history, bumpuje
//!    do nowej wersji (current+1)
//! 7. Validate (dry-run): parse + walidacja bez DB write — dla preview w
//!    dashboard przed save
//!
//! Wszystkie endpointy poza `validate` (który też jest admin-only)
//! wymagają roli `admin` przez `Claims::require_admin`.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use scrooge_policy::Policy;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::Claims,
    error::{ApiError, ApiResult},
    state::AppState,
};

// ────────────────────────────────────────────────────────────────────────────
// DTOs
// ────────────────────────────────────────────────────────────────────────────

/// Lekki wpis dla listing — bez `content_yaml` (szczegóły przez GET single).
#[derive(Debug, Serialize, FromRow, ToSchema)]
pub struct PolicySummaryDto {
    pub name: String,
    pub version: i32,
    pub enabled: bool,
    pub priority: i32,
    pub content_hash: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Pełny wpis — zwracany przez GET single + POST/PUT response.
#[derive(Debug, Serialize, ToSchema)]
pub struct PolicyDto {
    pub name: String,
    pub version: i32,
    pub enabled: bool,
    pub priority: i32,
    pub content_yaml: String,
    pub content_hash: String,
    pub targets: JsonValue,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreatePolicyRequest {
    /// Pełen YAML polityki. Parser akceptuje schemat `scroogedlp.io/v1`.
    pub content_yaml: String,
    /// Opcjonalny komentarz audytowy (trafia do `policy_history.change_reason`).
    #[serde(default)]
    pub change_reason: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdatePolicyRequest {
    pub content_yaml: String,
    #[serde(default)]
    pub change_reason: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ValidateRequest {
    pub content_yaml: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ValidateResponse {
    pub valid: bool,
    /// Lista błędów walidacji (puste gdy `valid=true`).
    pub errors: Vec<String>,
    /// Compiled hash — gdy `valid=true`. Pozwala dashboardowi porównać z
    /// istniejącymi politykami (czy zmiana faktycznie zmieni zawartość).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    /// Metadata (name + version z YAML) — dla preview w dashboard.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
    /// Sparsowana polityka (JSON) — używana przez dashboard form mode do
    /// rekonstrukcji pól formularza z YAMLa. Pole dostępne tylko gdy
    /// `valid=true`. Frontend traktuje to jako nieprzezroczystą strukturę
    /// (kształt dyktowany przez `scrooge_policy::Policy`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Object)]
    pub parsed: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, FromRow, ToSchema)]
pub struct PolicyHistoryEntryDto {
    pub id: i64,
    pub version: i32,
    pub content_yaml: String,
    pub changed_by: Option<Uuid>,
    pub changed_at: DateTime<Utc>,
    pub change_reason: Option<String>,
}

// ────────────────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────────────────

/// Pełna krotka kolumn z `SELECT ... FROM policies` — wynik sqlx::query_as.
/// Wyciągnięte do alias żeby uniknąć clippy `type_complexity` warning'a w
/// `get()` i `fetch_full()`.
type PolicyRow = (
    String,        // name
    i32,           // version
    bool,          // enabled
    i32,           // priority
    String,        // content_yaml
    String,        // content_hash
    JsonValue,     // targets
    DateTime<Utc>, // created_at
    DateTime<Utc>, // updated_at
);

/// Parsuje YAML → `Policy` + zwraca compiled (msgpack + hash) + serialized
/// targets (JSON). Wszystkie endpointy które zapisują do DB używają tego.
fn parse_and_compile(
    yaml: &str,
) -> Result<(Policy, scrooge_policy::CompiledPolicy, JsonValue), ApiError> {
    let policy = Policy::from_yaml(yaml).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let compiled = policy
        .compile()
        .map_err(|e| ApiError::Internal(format!("compile failed: {e}")))?;
    let targets_json = serde_json::to_value(&policy.targets)
        .map_err(|e| ApiError::Internal(format!("targets serialization failed: {e}")))?;
    Ok((policy, compiled, targets_json))
}

// ────────────────────────────────────────────────────────────────────────────
// GET /api/v1/policies
// ────────────────────────────────────────────────────────────────────────────

/// `GET /api/v1/policies` — lista wszystkich polityk (lekkie summary).
#[utoipa::path(
    get,
    path = "/api/v1/policies",
    responses(
        (status = 200, description = "Lista polityk", body = [PolicySummaryDto]),
        (status = 401, description = "Brak/zły token", body = crate::error::ErrorBody),
        (status = 403, description = "Wymagana rola admin", body = crate::error::ErrorBody),
    ),
    security(("bearer_auth" = [])),
    tag = "policies"
)]
pub async fn list(
    State(state): State<AppState>,
    claims: Claims,
) -> ApiResult<Json<Vec<PolicySummaryDto>>> {
    claims.require_admin()?;
    let rows: Vec<PolicySummaryDto> = sqlx::query_as(
        "SELECT name, version, enabled, priority, content_hash, created_at, updated_at \
         FROM policies ORDER BY priority DESC, name ASC",
    )
    .fetch_all(state.pool())
    .await?;
    Ok(Json(rows))
}

// ────────────────────────────────────────────────────────────────────────────
// GET /api/v1/policies/{name}
// ────────────────────────────────────────────────────────────────────────────

/// `GET /api/v1/policies/{name}` — pełna polityka z `content_yaml`.
#[utoipa::path(
    get,
    path = "/api/v1/policies/{name}",
    params(("name" = String, Path, description = "Policy name (slug)")),
    responses(
        (status = 200, description = "Pełen wpis polityki", body = PolicyDto),
        (status = 404, description = "Policy nie istnieje", body = crate::error::ErrorBody),
        (status = 401, body = crate::error::ErrorBody),
        (status = 403, body = crate::error::ErrorBody),
    ),
    security(("bearer_auth" = [])),
    tag = "policies"
)]
pub async fn get(
    State(state): State<AppState>,
    claims: Claims,
    Path(name): Path<String>,
) -> ApiResult<Json<PolicyDto>> {
    claims.require_admin()?;
    let row: Option<PolicyRow> = sqlx::query_as(
        "SELECT name, version, enabled, priority, content_yaml, content_hash, targets, \
                created_at, updated_at \
         FROM policies WHERE name = $1",
    )
    .bind(&name)
    .fetch_optional(state.pool())
    .await?;

    let row = row.ok_or(ApiError::NotFound)?;
    Ok(Json(PolicyDto {
        name: row.0,
        version: row.1,
        enabled: row.2,
        priority: row.3,
        content_yaml: row.4,
        content_hash: row.5,
        targets: row.6,
        created_at: row.7,
        updated_at: row.8,
    }))
}

// ────────────────────────────────────────────────────────────────────────────
// POST /api/v1/policies
// ────────────────────────────────────────────────────────────────────────────

/// `POST /api/v1/policies` — tworzy nową politykę (409 jeśli `metadata.name`
/// już istnieje).
#[utoipa::path(
    post,
    path = "/api/v1/policies",
    request_body = CreatePolicyRequest,
    responses(
        (status = 201, description = "Stworzona", body = PolicyDto),
        (status = 400, description = "Zły YAML / walidacja", body = crate::error::ErrorBody),
        (status = 409, description = "Policy o tej nazwie już istnieje", body = crate::error::ErrorBody),
    ),
    security(("bearer_auth" = [])),
    tag = "policies"
)]
pub async fn create(
    State(state): State<AppState>,
    claims: Claims,
    Json(req): Json<CreatePolicyRequest>,
) -> ApiResult<(StatusCode, Json<PolicyDto>)> {
    claims.require_admin()?;
    let (policy, compiled, targets) = parse_and_compile(&req.content_yaml)?;

    let created_by: Uuid = claims
        .sub
        .parse()
        .map_err(|_| ApiError::Internal("sub not UUID".into()))?;

    // Sprawdź unikalność nazwy.
    let exists: Option<(String,)> = sqlx::query_as("SELECT name FROM policies WHERE name = $1")
        .bind(&policy.metadata.name)
        .fetch_optional(state.pool())
        .await?;
    if exists.is_some() {
        return Err(ApiError::BadRequest(format!(
            "policy `{}` already exists — use PUT to update",
            policy.metadata.name
        )));
    }

    let priority = policy.priority;
    let version = i32::try_from(policy.metadata.version).unwrap_or(1);

    sqlx::query(
        "INSERT INTO policies \
         (name, version, content_yaml, content_compiled, content_hash, targets, priority, \
          enabled, created_by, updated_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, TRUE, $8, $8)",
    )
    .bind(&policy.metadata.name)
    .bind(version)
    .bind(&req.content_yaml)
    .bind(&compiled.msgpack)
    .bind(&compiled.hash)
    .bind(&targets)
    .bind(priority)
    .bind(created_by)
    .execute(state.pool())
    .await?;

    sqlx::query(
        "INSERT INTO policy_history (policy_name, version, content_yaml, changed_by, change_reason) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&policy.metadata.name)
    .bind(version)
    .bind(&req.content_yaml)
    .bind(created_by)
    .bind(req.change_reason.as_deref().unwrap_or("create"))
    .execute(state.pool())
    .await?;

    tracing::info!(
        user = %claims.username,
        policy = %policy.metadata.name,
        version,
        "policy created"
    );

    // Phase 3.x — live broadcast do connected agents (fire-and-forget).
    notify_policy_changed(&state, Some(policy.metadata.name.clone()));

    fetch_full(state.pool(), &policy.metadata.name)
        .await
        .map(|dto| (StatusCode::CREATED, Json(dto)))
}

/// Helper — wysyła `PolicyPushRequest` do manager-bin bridge'a jeśli wpięty.
/// `try_send` — drop fire-and-forget gdy kanał pełny (lepiej zignorować
/// niż blokować response'u — admin może retry'ować).
fn notify_policy_changed(state: &AppState, policy_name: Option<String>) {
    use crate::state::PolicyPushRequest;
    if let Some(tx) = state.policy_push_tx() {
        if let Err(e) = tx.try_send(PolicyPushRequest { policy_name }) {
            tracing::warn!(error = %e, "policy push notify failed (queue full?)");
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// PUT /api/v1/policies/{name}
// ────────────────────────────────────────────────────────────────────────────

/// `PUT /api/v1/policies/{name}` — update content_yaml. Bumpuje version w
/// DB (ignoruje `metadata.version` z YAML — manager jest autorytatywny).
#[utoipa::path(
    put,
    path = "/api/v1/policies/{name}",
    params(("name" = String, Path, description = "Policy name")),
    request_body = UpdatePolicyRequest,
    responses(
        (status = 200, description = "Updated", body = PolicyDto),
        (status = 400, description = "Zły YAML lub niezgodna nazwa", body = crate::error::ErrorBody),
        (status = 404, description = "Policy nie istnieje", body = crate::error::ErrorBody),
    ),
    security(("bearer_auth" = [])),
    tag = "policies"
)]
pub async fn update(
    State(state): State<AppState>,
    claims: Claims,
    Path(name): Path<String>,
    Json(req): Json<UpdatePolicyRequest>,
) -> ApiResult<Json<PolicyDto>> {
    claims.require_admin()?;
    let (policy, compiled, targets) = parse_and_compile(&req.content_yaml)?;

    if policy.metadata.name != name {
        return Err(ApiError::BadRequest(format!(
            "URL name `{}` does not match metadata.name `{}` in YAML",
            name, policy.metadata.name
        )));
    }

    let updated_by: Uuid = claims
        .sub
        .parse()
        .map_err(|_| ApiError::Internal("sub not UUID".into()))?;

    // Current version z DB (manager autoritatively bumpuje, ignoruje YAML).
    let current: Option<(i32,)> = sqlx::query_as("SELECT version FROM policies WHERE name = $1")
        .bind(&name)
        .fetch_optional(state.pool())
        .await?;
    let current_version = current.ok_or(ApiError::NotFound)?.0;
    let new_version = current_version + 1;

    let priority = policy.priority;

    sqlx::query(
        "UPDATE policies SET version = $1, content_yaml = $2, content_compiled = $3, \
                              content_hash = $4, targets = $5, priority = $6, updated_by = $7 \
         WHERE name = $8",
    )
    .bind(new_version)
    .bind(&req.content_yaml)
    .bind(&compiled.msgpack)
    .bind(&compiled.hash)
    .bind(&targets)
    .bind(priority)
    .bind(updated_by)
    .bind(&name)
    .execute(state.pool())
    .await?;

    sqlx::query(
        "INSERT INTO policy_history (policy_name, version, content_yaml, changed_by, change_reason) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&name)
    .bind(new_version)
    .bind(&req.content_yaml)
    .bind(updated_by)
    .bind(req.change_reason.as_deref().unwrap_or("update"))
    .execute(state.pool())
    .await?;

    tracing::info!(
        user = %claims.username,
        policy = %name,
        version = new_version,
        "policy updated"
    );

    notify_policy_changed(&state, Some(name.clone()));

    fetch_full(state.pool(), &name).await.map(Json)
}

// ────────────────────────────────────────────────────────────────────────────
// DELETE /api/v1/policies/{name}
// ────────────────────────────────────────────────────────────────────────────

/// `DELETE /api/v1/policies/{name}` — hard delete (kaskadowo usuwa
/// `policy_assignments`, ale `policy_history` zostaje — append-only).
#[utoipa::path(
    delete,
    path = "/api/v1/policies/{name}",
    params(("name" = String, Path, description = "Policy name")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 404, description = "Policy nie istnieje", body = crate::error::ErrorBody),
    ),
    security(("bearer_auth" = [])),
    tag = "policies"
)]
pub async fn delete_(
    State(state): State<AppState>,
    claims: Claims,
    Path(name): Path<String>,
) -> ApiResult<StatusCode> {
    claims.require_admin()?;
    let result = sqlx::query("DELETE FROM policies WHERE name = $1")
        .bind(&name)
        .execute(state.pool())
        .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    tracing::info!(user = %claims.username, policy = %name, "policy deleted");
    // Po delete agent powinien usunąć msgpack lokalnie — Phase 3.x flow
    // do dopracowania (obecnie agent zostawia stary plik). Notify żeby
    // przyszły delete-aware kod miał trigger.
    notify_policy_changed(&state, Some(name.clone()));
    Ok(StatusCode::NO_CONTENT)
}

// ────────────────────────────────────────────────────────────────────────────
// GET /api/v1/policies/{name}/history
// ────────────────────────────────────────────────────────────────────────────

/// `GET /api/v1/policies/{name}/history` — wszystkie wersje (od najnowszej).
#[utoipa::path(
    get,
    path = "/api/v1/policies/{name}/history",
    params(("name" = String, Path, description = "Policy name")),
    responses(
        (status = 200, description = "Historia wersji", body = [PolicyHistoryEntryDto]),
    ),
    security(("bearer_auth" = [])),
    tag = "policies"
)]
pub async fn history(
    State(state): State<AppState>,
    claims: Claims,
    Path(name): Path<String>,
) -> ApiResult<Json<Vec<PolicyHistoryEntryDto>>> {
    claims.require_admin()?;
    let rows: Vec<PolicyHistoryEntryDto> = sqlx::query_as(
        "SELECT id, version, content_yaml, changed_by, changed_at, change_reason \
         FROM policy_history WHERE policy_name = $1 ORDER BY version DESC",
    )
    .bind(&name)
    .fetch_all(state.pool())
    .await?;
    Ok(Json(rows))
}

// ────────────────────────────────────────────────────────────────────────────
// POST /api/v1/policies/{name}/rollback/{version}
// ────────────────────────────────────────────────────────────────────────────

/// `POST /api/v1/policies/{name}/rollback/{version}` — odtwarza policy z
/// `policy_history.version` jako nowa wersja (current+1). Dorzuca wpis do
/// history z `change_reason = "rollback to vN"`.
#[utoipa::path(
    post,
    path = "/api/v1/policies/{name}/rollback/{version}",
    params(
        ("name"    = String, Path, description = "Policy name"),
        ("version" = i32,    Path, description = "Wersja z history do której rollback"),
    ),
    responses(
        (status = 200, description = "Rollback wykonany", body = PolicyDto),
        (status = 404, description = "Brak takiej polityki / wersji w history", body = crate::error::ErrorBody),
    ),
    security(("bearer_auth" = [])),
    tag = "policies"
)]
pub async fn rollback(
    State(state): State<AppState>,
    claims: Claims,
    Path((name, version)): Path<(String, i32)>,
) -> ApiResult<Json<PolicyDto>> {
    claims.require_admin()?;
    let updated_by: Uuid = claims
        .sub
        .parse()
        .map_err(|_| ApiError::Internal("sub not UUID".into()))?;

    // Pobierz historyczny YAML.
    let historical: Option<(String,)> = sqlx::query_as(
        "SELECT content_yaml FROM policy_history WHERE policy_name = $1 AND version = $2",
    )
    .bind(&name)
    .bind(version)
    .fetch_optional(state.pool())
    .await?;
    let yaml = historical.ok_or(ApiError::NotFound)?.0;

    // Sprawdź czy policy w ogóle istnieje obecnie (jeśli była usunięta,
    // rollback nie ma sensu — najpierw POST recreate).
    let current: Option<(i32,)> = sqlx::query_as("SELECT version FROM policies WHERE name = $1")
        .bind(&name)
        .fetch_optional(state.pool())
        .await?;
    let current_version = current.ok_or(ApiError::NotFound)?.0;
    let new_version = current_version + 1;

    // Re-compile (regex'y mogły się zmienić w nowej wersji scrooge-policy,
    // ale to OK — walidacja przejdzie jeśli stary YAML jest backwards-compat).
    let (policy, compiled, targets) = parse_and_compile(&yaml)?;
    let priority = policy.priority;

    sqlx::query(
        "UPDATE policies SET version = $1, content_yaml = $2, content_compiled = $3, \
                              content_hash = $4, targets = $5, priority = $6, updated_by = $7 \
         WHERE name = $8",
    )
    .bind(new_version)
    .bind(&yaml)
    .bind(&compiled.msgpack)
    .bind(&compiled.hash)
    .bind(&targets)
    .bind(priority)
    .bind(updated_by)
    .bind(&name)
    .execute(state.pool())
    .await?;

    sqlx::query(
        "INSERT INTO policy_history (policy_name, version, content_yaml, changed_by, change_reason) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&name)
    .bind(new_version)
    .bind(&yaml)
    .bind(updated_by)
    .bind(format!("rollback to v{version}"))
    .execute(state.pool())
    .await?;

    tracing::info!(
        user = %claims.username,
        policy = %name,
        rolled_back_to = version,
        new_version,
        "policy rolled back"
    );

    notify_policy_changed(&state, Some(name.clone()));

    fetch_full(state.pool(), &name).await.map(Json)
}

// ────────────────────────────────────────────────────────────────────────────
// POST /api/v1/policies/validate
// ────────────────────────────────────────────────────────────────────────────

/// `POST /api/v1/policies/validate` — dry-run walidacji YAML bez zapisywania.
/// Dashboard używa do live-preview gdy admin edytuje YAML w editorze.
#[utoipa::path(
    post,
    path = "/api/v1/policies/validate",
    request_body = ValidateRequest,
    responses(
        (status = 200, description = "Validation result", body = ValidateResponse),
    ),
    security(("bearer_auth" = [])),
    tag = "policies"
)]
pub async fn validate(
    _state: State<AppState>,
    claims: Claims,
    Json(req): Json<ValidateRequest>,
) -> ApiResult<Json<ValidateResponse>> {
    claims.require_admin()?;
    match Policy::from_yaml(&req.content_yaml) {
        Ok(policy) => {
            let compiled = policy
                .compile()
                .map_err(|e| ApiError::Internal(format!("compile: {e}")))?;
            // Serializuj sparsowaną politykę do JSON-a dla dashboardu (form mode).
            // `Policy` implementuje `Serialize`, więc to nie powinno fail'ować —
            // jeśli jednak, zwracamy None zamiast 500 (parsed jest opcjonalne).
            let parsed = serde_json::to_value(&policy).ok();
            Ok(Json(ValidateResponse {
                valid: true,
                errors: vec![],
                hash: Some(compiled.hash),
                name: Some(policy.metadata.name),
                version: Some(policy.metadata.version),
                parsed,
            }))
        },
        Err(e) => Ok(Json(ValidateResponse {
            valid: false,
            errors: vec![e.to_string()],
            hash: None,
            name: None,
            version: None,
            parsed: None,
        })),
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────────────────

async fn fetch_full(pool: &sqlx::PgPool, name: &str) -> ApiResult<PolicyDto> {
    let row: PolicyRow = sqlx::query_as(
        "SELECT name, version, enabled, priority, content_yaml, content_hash, targets, \
                created_at, updated_at \
         FROM policies WHERE name = $1",
    )
    .bind(name)
    .fetch_one(pool)
    .await?;

    Ok(PolicyDto {
        name: row.0,
        version: row.1,
        enabled: row.2,
        priority: row.3,
        content_yaml: row.4,
        content_hash: row.5,
        targets: row.6,
        created_at: row.7,
        updated_at: row.8,
    })
}
