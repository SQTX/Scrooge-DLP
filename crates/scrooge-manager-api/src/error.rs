// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! Typy błędów REST API + ich mapowanie na HTTP responses.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;
use utoipa::ToSchema;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("invalid credentials")]
    InvalidCredentials,

    #[error("invalid or expired token")]
    InvalidToken,

    #[error("not authenticated")]
    Unauthorized,

    #[error("forbidden")]
    Forbidden,

    #[error("not found")]
    NotFound,

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("database error")]
    Database(#[from] sqlx::Error),

    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorBody {
    /// Stabilny code do programmatic handling.
    pub error: &'static str,
    /// Human-readable opis.
    pub message: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            Self::InvalidCredentials => (StatusCode::UNAUTHORIZED, "invalid_credentials"),
            Self::InvalidToken => (StatusCode::UNAUTHORIZED, "invalid_token"),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            Self::ServiceUnavailable(_) => (StatusCode::SERVICE_UNAVAILABLE, "service_unavailable"),
            Self::Database(e) => {
                tracing::error!(error = %e, "database error");
                (StatusCode::INTERNAL_SERVER_ERROR, "database_error")
            },
            Self::Internal(msg) => {
                tracing::error!(error = %msg, "internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
            },
        };

        let body = ErrorBody {
            error: code,
            message: self.to_string(),
        };
        (status, Json(body)).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
