// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! JWT auth dla REST API: login, refresh, middleware ekstrakcji `Claims`.
//!
//! Uwaga: queries używają non-macro `sqlx::query_as` (runtime-checked).
//! W przyszłości można zmigrować na `sqlx::query_as!` + offline cache
//! (`cargo sqlx prepare`) gdy będzie dev DB pipeline.

use axum::{
    extract::{FromRequestParts, State},
    http::request::Parts,
    Json,
};
use chrono::{DateTime, Utc};
use jsonwebtoken::{decode, encode, Algorithm, Header, Validation};
use scrooge_common::crypto;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    error::{ApiError, ApiResult},
    state::AppState,
};

// ────────────────────────────────────────────────────────────────────────────
// JWT Claims
// ────────────────────────────────────────────────────────────────────────────

/// JWT payload. Tworzony przez `login`/`refresh`, weryfikowany przez middleware.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// user_id (UUID).
    pub sub: String,
    /// username (dla logów).
    pub username: String,
    /// role: "admin" | "analyst" | "viewer".
    pub role: String,
    /// expiration (unix seconds).
    pub exp: i64,
    /// issued at (unix seconds).
    pub iat: i64,
}

/// Extractor — zwraca `Claims` z poprawnego Bearer token'a.
#[axum::async_trait]
impl FromRequestParts<AppState> for Claims {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or(ApiError::Unauthorized)?;

        let token = header
            .strip_prefix("Bearer ")
            .ok_or(ApiError::Unauthorized)?
            .trim();

        let mut validation = Validation::new(Algorithm::HS256);
        validation.leeway = 5;

        let data = decode::<Self>(token, &state.auth().decoding, &validation)
            .map_err(|_| ApiError::InvalidToken)?;
        Ok(data.claims)
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Login
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LoginResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: &'static str,
    pub expires_in: u64,
}

#[derive(Debug, FromRow)]
struct UserRow {
    id: Uuid,
    username: String,
    password_hash: String,
    role: String,
    enabled: bool,
}

/// `POST /api/v1/auth/login` — wymienia username/password na parę JWT + refresh.
#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Authenticated", body = LoginResponse),
        (status = 401, description = "Invalid credentials", body = crate::error::ErrorBody),
    ),
    tag = "auth"
)]
pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> ApiResult<Json<LoginResponse>> {
    let user: Option<UserRow> = sqlx::query_as(
        "SELECT id, username, password_hash, role, enabled FROM users WHERE username = $1",
    )
    .bind(&req.username)
    .fetch_optional(state.pool())
    .await?;

    let user = user.ok_or(ApiError::InvalidCredentials)?;
    if !user.enabled {
        return Err(ApiError::InvalidCredentials);
    }
    if !crypto::verify_password(&req.password, &user.password_hash)
        .map_err(|_| ApiError::InvalidCredentials)?
    {
        return Err(ApiError::InvalidCredentials);
    }

    sqlx::query("UPDATE users SET last_login = NOW() WHERE id = $1")
        .bind(user.id)
        .execute(state.pool())
        .await?;

    let tokens = issue_tokens(&state, user.id, &user.username, &user.role).await?;
    Ok(Json(tokens))
}

// ────────────────────────────────────────────────────────────────────────────
// Refresh
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, FromRow)]
struct RefreshUserRow {
    user_id: Uuid,
    username: String,
    role: String,
    enabled: bool,
}

/// `POST /api/v1/auth/refresh` — rotuje refresh token (revokuje stary, wydaje nowy).
#[utoipa::path(
    post,
    path = "/api/v1/auth/refresh",
    request_body = RefreshRequest,
    responses(
        (status = 200, description = "Refreshed", body = LoginResponse),
        (status = 401, description = "Invalid refresh token", body = crate::error::ErrorBody),
    ),
    tag = "auth"
)]
pub async fn refresh(
    State(state): State<AppState>,
    Json(req): Json<RefreshRequest>,
) -> ApiResult<Json<LoginResponse>> {
    let hash = hash_token(&req.refresh_token);

    let user: Option<RefreshUserRow> = sqlx::query_as(
        r"
        SELECT u.id AS user_id, u.username, u.role, u.enabled
        FROM refresh_tokens t
        JOIN users u ON u.id = t.user_id
        WHERE t.token_hash = $1
          AND t.revoked = FALSE
          AND t.expires_at > NOW()
        ",
    )
    .bind(&hash)
    .fetch_optional(state.pool())
    .await?;

    let user = user.ok_or(ApiError::InvalidToken)?;
    if !user.enabled {
        return Err(ApiError::InvalidToken);
    }

    // Rotate - revoke old.
    sqlx::query("UPDATE refresh_tokens SET revoked = TRUE WHERE token_hash = $1")
        .bind(&hash)
        .execute(state.pool())
        .await?;

    let tokens = issue_tokens(&state, user.user_id, &user.username, &user.role).await?;
    Ok(Json(tokens))
}

// ────────────────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────────────────

async fn issue_tokens(
    state: &AppState,
    user_id: Uuid,
    username: &str,
    role: &str,
) -> ApiResult<LoginResponse> {
    let auth = state.auth();
    let now = Utc::now().timestamp();

    let claims = Claims {
        sub: user_id.to_string(),
        username: username.to_string(),
        role: role.to_string(),
        iat: now,
        exp: now
            + i64::try_from(auth.access_ttl_secs)
                .map_err(|_| ApiError::Internal("access_ttl overflow".into()))?,
    };
    let access_token = encode(&Header::new(Algorithm::HS256), &claims, &auth.encoding)
        .map_err(|_| ApiError::Internal("JWT encode failed".into()))?;

    // Refresh token: opaque UUID, persist SHA-256.
    let refresh_raw = Uuid::new_v4().to_string();
    let refresh_hash = hash_token(&refresh_raw);
    let expires_at: DateTime<Utc> = Utc::now()
        + chrono::Duration::seconds(
            i64::try_from(auth.refresh_ttl_secs)
                .map_err(|_| ApiError::Internal("refresh_ttl overflow".into()))?,
        );

    sqlx::query("INSERT INTO refresh_tokens (token_hash, user_id, expires_at) VALUES ($1, $2, $3)")
        .bind(&refresh_hash)
        .bind(user_id)
        .bind(expires_at)
        .execute(state.pool())
        .await?;

    Ok(LoginResponse {
        access_token,
        refresh_token: refresh_raw,
        token_type: "Bearer",
        expires_in: auth.access_ttl_secs,
    })
}

fn hash_token(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    format!("{:x}", hasher.finalize())
}
