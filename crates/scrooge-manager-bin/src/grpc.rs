// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 2 as
// published by the Free Software Foundation.

//! gRPC server managera — implementacja `AgentService`.
//!
//! Pełne mTLS (Sub-faza 1E): `Stream` wymaga client cert podpisanego przez
//! nasz Root CA. Manager wyciąga `agent_id` z Subject CN cert'a (NIE z
//! metadata jak w MVP). `Enroll` RPC jest z definicji pre-cert (agent nie
//! ma jeszcze swojego cert'a), więc TLS jest skonfigurowany jako
//! `client_auth_optional(true)` — connection bez cert jest akceptowane
//! przez transport, a per-RPC logika decyduje czy wymagać.

use std::{pin::Pin, sync::Arc};

use anyhow::Context;
use chrono::Utc;
use scrooge_common::{ca::RootCa, config::ManagerServerConfig};
use scrooge_proto::v1::{
    agent_message, agent_service_server::AgentService, manager_message, AgentMessage,
    EnrollRequest, EnrollResponse, HeartbeatAck, ManagerMessage,
};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgPool};
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, Stream, StreamExt};
use tonic::{
    transport::{Certificate, Identity, ServerTlsConfig},
    Request, Response, Status, Streaming,
};
use uuid::Uuid;
use x509_parser::prelude::*;

/// Buduje konfigurację TLS dla tonic server (mTLS — pełne weryfikowanie
/// client cert podpisanego przez nasz Root CA).
///
/// `client_auth_optional(true)`: TLS handshake przepuszcza zarówno
/// połączenia z client cert (Stream) jak i bez (Enroll). Per-RPC handler
/// sprawdza w `request.peer_certs()` czy cert obecny.
pub(crate) fn tls_config(server: &ManagerServerConfig) -> anyhow::Result<ServerTlsConfig> {
    let cert = std::fs::read(&server.tls_cert_path)
        .with_context(|| format!("reading TLS cert `{}`", server.tls_cert_path))?;
    let key = std::fs::read(&server.tls_key_path)
        .with_context(|| format!("reading TLS key `{}`", server.tls_key_path))?;
    let ca = std::fs::read(&server.ca_cert_path)
        .with_context(|| format!("reading CA cert `{}`", server.ca_cert_path))?;
    Ok(ServerTlsConfig::new()
        .identity(Identity::from_pem(cert, key))
        .client_ca_root(Certificate::from_pem(ca))
        .client_auth_optional(true))
}

/// Wyciąga `agent_id` (UUID) z Subject CN client cert'a w peer chain.
///
/// Returns `Status::unauthenticated` gdy:
/// - brak client cert (TLS przepuścił bo `client_auth_optional`, ale Stream wymaga)
/// - cert nie jest poprawnym X.509
/// - CN nie istnieje lub nie jest UUID-em
///
/// Sam fakt że cert dotarł do handler'a oznacza że TLS go zweryfikowało
/// przeciwko Root CA — tu tylko ekstrahujemy tożsamość.
//
// `tonic::Status` jest sporej wagi (176B), ale wymuszone przez tonic API.
#[allow(clippy::result_large_err)]
fn extract_agent_id<T>(request: &Request<T>) -> Result<Uuid, Status> {
    let certs = request
        .peer_certs()
        .ok_or_else(|| Status::unauthenticated("client cert required (mTLS)"))?;
    let first = certs
        .first()
        .ok_or_else(|| Status::unauthenticated("no client cert in peer chain"))?;
    let (_, parsed) = X509Certificate::from_der(first.as_ref())
        .map_err(|_| Status::unauthenticated("invalid client cert (X.509 parse)"))?;
    let cn = parsed
        .subject()
        .iter_common_name()
        .next()
        .and_then(|attr| attr.as_str().ok())
        .ok_or_else(|| Status::unauthenticated("client cert subject has no CN"))?;
    Uuid::parse_str(cn).map_err(|_| Status::unauthenticated("client cert CN is not a UUID"))
}

/// Implementacja `AgentService`.
#[derive(Debug)]
pub(crate) struct AgentServiceImpl {
    pool: PgPool,
    ca: Arc<RootCa>,
}

impl AgentServiceImpl {
    pub(crate) fn new(pool: PgPool, ca: Arc<RootCa>) -> Self {
        Self { pool, ca }
    }
}

type StreamResponse = Pin<Box<dyn Stream<Item = Result<ManagerMessage, Status>> + Send + 'static>>;

#[derive(Debug, FromRow)]
struct EnrollmentTokenRow {
    max_uses: Option<i32>,
    uses_count: i32,
    expires_at: Option<chrono::DateTime<Utc>>,
}

#[tonic::async_trait]
impl AgentService for AgentServiceImpl {
    async fn enroll(
        &self,
        request: Request<EnrollRequest>,
    ) -> Result<Response<EnrollResponse>, Status> {
        let req = request.into_inner();
        let info = req
            .info
            .ok_or_else(|| Status::invalid_argument("missing AgentInfo"))?;

        tracing::info!(
            hostname = %info.hostname,
            os = %info.os,
            arch = %info.arch,
            "enrollment request received",
        );

        // 1. Walidacja enrollment token (SHA-256 lookup).
        let token_hash = sha256_hex(req.enrollment_token.as_bytes());
        let token: Option<EnrollmentTokenRow> = sqlx::query_as(
            "SELECT max_uses, uses_count, expires_at FROM enrollment_tokens WHERE token_hash = $1",
        )
        .bind(&token_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "DB error during token lookup");
            Status::internal("database error")
        })?;

        let token = token.ok_or_else(|| Status::unauthenticated("invalid enrollment token"))?;

        if let Some(expires) = token.expires_at {
            if expires < Utc::now() {
                return Err(Status::unauthenticated("enrollment token expired"));
            }
        }
        if let Some(max) = token.max_uses {
            if token.uses_count >= max {
                return Err(Status::unauthenticated("enrollment token usage exceeded"));
            }
        }

        // 2. Wygeneruj agent_id przed signowaniem — manager jest autorytatywnym
        //    źródłem identity, więc CN wystawianego cert'a = agent_id (NIE
        //    hostname z CSR-a). Wymagane przez mTLS handler `extract_agent_id`
        //    który czyta `agent_id` z peer cert Subject CN.
        let agent_id = Uuid::new_v4();

        // 3. Sign CSR z wymuszonym CN=agent_id.
        let csr_pem = std::str::from_utf8(&req.csr_pem)
            .map_err(|_| Status::invalid_argument("CSR must be valid UTF-8 PEM"))?;
        let cert_pem = self
            .ca
            .sign_csr_with_cn(csr_pem, &agent_id.to_string())
            .map_err(|e| {
                tracing::error!(error = %e, "CSR signing failed");
                Status::internal("CSR signing failed")
            })?;
        let cert_fingerprint = sha256_hex(cert_pem.as_bytes());

        sqlx::query(
            "INSERT INTO agents \
             (id, hostname, os, os_version, arch, agent_version, enrolled_at, last_seen, status, cert_fingerprint) \
             VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW(), 'active', $7)",
        )
        .bind(agent_id)
        .bind(&info.hostname)
        .bind(&info.os)
        .bind(&info.os_version)
        .bind(&info.arch)
        .bind(&info.agent_version)
        .bind(&cert_fingerprint)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "inserting agent");
            Status::internal("database error")
        })?;

        // 4. Increment token usage (best-effort).
        if let Err(e) = sqlx::query(
            "UPDATE enrollment_tokens SET uses_count = uses_count + 1 WHERE token_hash = $1",
        )
        .bind(&token_hash)
        .execute(&self.pool)
        .await
        {
            tracing::warn!(error = %e, "failed to increment token usage");
        }

        tracing::info!(agent_id = %agent_id, hostname = %info.hostname, "agent enrolled");

        Ok(Response::new(EnrollResponse {
            agent_id: agent_id.to_string(),
            cert_pem: cert_pem.into_bytes(),
            ca_chain_pem: self.ca.cert_pem().to_string().into_bytes(),
        }))
    }

    type StreamStream = StreamResponse;

    async fn stream(
        &self,
        request: Request<Streaming<AgentMessage>>,
    ) -> Result<Response<Self::StreamStream>, Status> {
        // mTLS: agent_id z Subject CN client cert (NIE z metadata).
        // TLS layer już zweryfikował że cert jest podpisany przez nasz Root CA.
        let agent_id = extract_agent_id(&request)?;

        // Weryfikuj że agent istnieje w DB i nie został revoke'd.
        // Aktualnie sprawdzamy tylko obecność — revoke flag w DB dochodzi
        // wraz z dashboardowym "Remove agent" w przyszłej fazie.
        let exists: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM agents WHERE id = $1")
            .bind(agent_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "DB lookup");
                Status::internal("database error")
            })?;
        if exists.is_none() {
            return Err(Status::unauthenticated("agent not enrolled"));
        }

        tracing::info!(%agent_id, "Stream connection accepted (mTLS)");

        let pool = self.pool.clone();
        let mut inbound = request.into_inner();
        let (tx, rx) = mpsc::channel::<Result<ManagerMessage, Status>>(16);

        tokio::spawn(async move {
            while let Some(msg) = inbound.next().await {
                match msg {
                    Ok(AgentMessage {
                        payload: Some(agent_message::Payload::Heartbeat(_hb)),
                    }) => {
                        tracing::debug!(%agent_id, "heartbeat");

                        if let Err(e) =
                            sqlx::query("UPDATE agents SET last_seen = NOW() WHERE id = $1")
                                .bind(agent_id)
                                .execute(&pool)
                                .await
                        {
                            tracing::warn!(error = %e, "updating last_seen");
                        }

                        let ack = ManagerMessage {
                            payload: Some(manager_message::Payload::HeartbeatAck(HeartbeatAck {
                                timestamp_ns: Utc::now().timestamp_nanos_opt().unwrap_or(0),
                            })),
                        };
                        if tx.send(Ok(ack)).await.is_err() {
                            break;
                        }
                    },
                    Ok(_other) => {
                        tracing::debug!(%agent_id, "non-heartbeat message (ignored in MVP)");
                    },
                    Err(e) => {
                        tracing::warn!(%agent_id, error = %e, "inbound error");
                        break;
                    },
                }
            }
            tracing::info!(%agent_id, "Stream disconnected");
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }
}

fn sha256_hex(input: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input);
    format!("{:x}", hasher.finalize())
}
