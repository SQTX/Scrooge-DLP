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

use std::{collections::HashMap, pin::Pin, sync::Arc};

use anyhow::Context;
use chrono::Utc;
use scrooge_common::{ca::RootCa, config::ManagerServerConfig};
use scrooge_manager_api::AgentCommandRequest;
use scrooge_proto::v1::{
    agent_message, agent_service_server::AgentService, manager_message, AgentMessage, Command,
    EnrollRequest, EnrollResponse, EventBatch, HeartbeatAck, ManagerMessage, PolicyRef,
    PolicyUpdate,
};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgPool};
use tokio::sync::{mpsc, RwLock};
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

// ────────────────────────────────────────────────────────────────────────────
// AgentRegistry — connected agents (Sub-faza 2E)
// ────────────────────────────────────────────────────────────────────────────

/// Type alias dla outbound channel per connected agent.
type AgentTx = mpsc::Sender<Result<ManagerMessage, Status>>;

/// Mapa `agent_id → outbound channel`. Stream handler registruje przy
/// accept, unregistruje przy disconnect. Bridge task (`run_command_bridge`)
/// lookuje tu agenta przy POST /agents/{id}/command.
#[derive(Clone)]
pub(crate) struct AgentRegistry {
    inner: Arc<RwLock<HashMap<Uuid, AgentTx>>>,
}

impl AgentRegistry {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn register(&self, agent_id: Uuid, tx: AgentTx) {
        self.inner.write().await.insert(agent_id, tx);
    }

    async fn unregister(&self, agent_id: Uuid) {
        self.inner.write().await.remove(&agent_id);
    }

    async fn get(&self, agent_id: Uuid) -> Option<AgentTx> {
        self.inner.read().await.get(&agent_id).cloned()
    }

    /// Snapshot UUIDs wszystkich obecnie connected agents — dla policy
    /// broadcast (Phase 3.x).
    async fn all_agent_ids(&self) -> Vec<Uuid> {
        self.inner.read().await.keys().copied().collect()
    }
}

/// Phase 3.x — bridge dla live policy broadcast. Po `policies::create/
/// update/delete/rollback` REST handler emit'uje `PolicyPushRequest`.
/// Bridge iteruje connected agents (`registry`), dla każdego loaduje
/// pending policies (delta vs ostatnie acked) i pushuje przez gRPC.
///
/// Brak DB-level filtering po targets — `load_pending_policies` już
/// to robi (matchuje os/tags/groups/agent_ids per `policy_assignments`
/// table). Bridge nie rozwiązuje target matching, polega na PG view.
pub(crate) async fn run_policy_push_bridge(
    registry: AgentRegistry,
    pool: PgPool,
    mut rx: mpsc::Receiver<scrooge_manager_api::PolicyPushRequest>,
) {
    while let Some(req) = rx.recv().await {
        let agent_ids = registry.all_agent_ids().await;
        tracing::info!(
            policy = ?req.policy_name,
            connected_agents = agent_ids.len(),
            "policy push broadcast"
        );
        for agent_id in agent_ids {
            let Some(tx) = registry.get(agent_id).await else { continue };
            match load_pending_policies(&pool, agent_id).await {
                Ok(pending) if pending.is_empty() => {
                    tracing::debug!(%agent_id, "broadcast: no pending");
                },
                Ok(pending) => {
                    for p in &pending {
                        let version_i32 = i32::try_from(p.version).unwrap_or(0);
                        if let Err(e) = mark_assigned(
                            &pool,
                            agent_id,
                            &p.policy_id,
                            version_i32,
                        )
                        .await
                        {
                            tracing::warn!(error = %e, policy = %p.policy_id, "mark_assigned (broadcast)");
                        }
                    }
                    let update = ManagerMessage {
                        payload: Some(manager_message::Payload::Policy(PolicyUpdate {
                            policies: pending,
                            full_replace: false,
                            remove_policy_ids: vec![],
                        })),
                    };
                    if tx.send(Ok(update)).await.is_err() {
                        tracing::debug!(%agent_id, "broadcast: channel closed");
                    }
                },
                Err(e) => tracing::warn!(%agent_id, error = %e, "load_pending_policies (broadcast)"),
            }
        }
    }
}

/// Bridge task — odbiera `AgentCommandRequest` z REST handler'a i wysyła
/// `ManagerMessage::Command` do agenta przez jego outbound channel (jeśli
/// connected). Działa dopóki kanał REST nie zostanie zamknięty (= manager
/// shutdown).
pub(crate) async fn run_command_bridge(
    registry: AgentRegistry,
    mut rx: mpsc::Receiver<AgentCommandRequest>,
) {
    while let Some(req) = rx.recv().await {
        match registry.get(req.agent_id).await {
            Some(tx) => {
                let cmd = ManagerMessage {
                    payload: Some(manager_message::Payload::Command(Command {
                        command_id: req.command_id.clone(),
                        r#type: req.command_type,
                        payload: req.payload,
                        issued_at_ns: Utc::now().timestamp_nanos_opt().unwrap_or(0),
                        deadline_ns: 0,
                    })),
                };
                if tx.send(Ok(cmd)).await.is_err() {
                    tracing::warn!(
                        agent_id = %req.agent_id,
                        command_id = %req.command_id,
                        "outbound channel closed — agent disconnecting"
                    );
                } else {
                    tracing::info!(
                        agent_id = %req.agent_id,
                        command_id = %req.command_id,
                        command_type = req.command_type,
                        "command forwarded to agent"
                    );
                }
            },
            None => {
                tracing::warn!(
                    agent_id = %req.agent_id,
                    command_id = %req.command_id,
                    "agent not connected — command dropped"
                );
            },
        }
    }
}

/// Implementacja `AgentService`.
#[derive(Debug)]
pub(crate) struct AgentServiceImpl {
    pool: PgPool,
    ca: Arc<RootCa>,
    registry: AgentRegistry,
}

impl AgentServiceImpl {
    pub(crate) fn new(pool: PgPool, ca: Arc<RootCa>, registry: AgentRegistry) -> Self {
        Self { pool, ca, registry }
    }
}

impl std::fmt::Debug for AgentRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentRegistry").finish_non_exhaustive()
    }
}

type StreamResponse = Pin<Box<dyn Stream<Item = Result<ManagerMessage, Status>> + Send + 'static>>;

// ────────────────────────────────────────────────────────────────────────────
// Policy push helpers (Sub-faza 2C)
// ────────────────────────────────────────────────────────────────────────────

/// Pobiera polityki które agent powinien dostać przy connect/reconnect:
/// wszystkie `enabled=true` których agent nie ma w aktualnej wersji.
///
/// MVP: bez targets matching — wszystkie enabled polityki idą do każdego
/// agenta. Targets (os/tags/groups) dorzucimy w 2C.1 gdy agents table dostanie
/// kolumny `tags JSONB` + `groups TEXT[]`.
async fn load_pending_policies(
    pool: &PgPool,
    agent_id: Uuid,
) -> Result<Vec<PolicyRef>, sqlx::Error> {
    let rows: Vec<(String, i32, Vec<u8>, String, i32)> = sqlx::query_as(
        "SELECT p.name, p.version, p.content_compiled, p.content_hash, p.priority \
         FROM policies p \
         LEFT JOIN policy_assignments pa \
           ON pa.policy_name = p.name AND pa.agent_id = $1 \
         WHERE p.enabled = TRUE \
           AND (pa.acked_version IS NULL OR pa.acked_version < p.version) \
         ORDER BY p.priority DESC, p.name ASC",
    )
    .bind(agent_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(name, version, compiled, hash, priority)| PolicyRef {
            policy_id: name,
            version: u32::try_from(version).unwrap_or(0),
            content_compiled: compiled,
            content_hash: hash,
            priority: u32::try_from(priority).unwrap_or(0),
        })
        .collect())
}

/// Rejestruje że dana polityka została wypchnięta do agenta (assigned_version
/// = aktualna policy version). `acked_version` pozostaje NULL/stara dopóki
/// agent nie odeśle `PolicyAck` (handler ack'a updateuje).
async fn mark_assigned(
    pool: &PgPool,
    agent_id: Uuid,
    policy_name: &str,
    version: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO policy_assignments \
           (agent_id, policy_name, assigned_version, last_pushed_at) \
         VALUES ($1, $2, $3, NOW()) \
         ON CONFLICT (agent_id, policy_name) \
         DO UPDATE SET assigned_version = EXCLUDED.assigned_version, \
                       last_pushed_at = EXCLUDED.last_pushed_at",
    )
    .bind(agent_id)
    .bind(policy_name)
    .bind(version)
    .execute(pool)
    .await?;
    Ok(())
}

/// Agent potwierdził że załadował policy — update `acked_version` w DB.
async fn mark_acked(
    pool: &PgPool,
    agent_id: Uuid,
    policy_name: &str,
    version: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE policy_assignments \
         SET acked_version = $1 \
         WHERE agent_id = $2 AND policy_name = $3",
    )
    .bind(version)
    .bind(agent_id)
    .bind(policy_name)
    .execute(pool)
    .await?;
    Ok(())
}

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

    #[allow(clippy::too_many_lines)] // 3 background tasks (initial push +
                                     // inbound loop + heartbeat ack) wymagają
                                     // setup'u który nie dzieli się czysto
                                     // dalej bez tracenia czytelności.
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
        let registry = self.registry.clone();
        let mut inbound = request.into_inner();
        let (tx, rx) = mpsc::channel::<Result<ManagerMessage, Status>>(16);

        // Sub-faza 2E: register agent w registry — bridge może teraz wysyłać
        // do niego commendy.
        registry.register(agent_id, tx.clone()).await;

        // ────────────────────────────────────────────────────────────────────
        // Initial policy push (Sub-faza 2C): delta polityk dla tego agenta.
        // Wysyłamy w background tasku żeby nie blokować accept Stream'a.
        // ────────────────────────────────────────────────────────────────────
        let pool_push = pool.clone();
        let tx_push = tx.clone();
        tokio::spawn(async move {
            match load_pending_policies(&pool_push, agent_id).await {
                Ok(pending) if pending.is_empty() => {
                    tracing::debug!(%agent_id, "no pending policies to push");
                },
                Ok(pending) => {
                    tracing::info!(
                        %agent_id,
                        count = pending.len(),
                        "pushing pending policies"
                    );
                    // Zapisz w policy_assignments że wysłaliśmy (acked_version
                    // poczeka na PolicyAck z agenta).
                    for p in &pending {
                        let version_i32 = i32::try_from(p.version).unwrap_or(0);
                        if let Err(e) =
                            mark_assigned(&pool_push, agent_id, &p.policy_id, version_i32).await
                        {
                            tracing::warn!(error = %e, policy = %p.policy_id, "mark_assigned");
                        }
                    }
                    let update = ManagerMessage {
                        payload: Some(manager_message::Payload::Policy(PolicyUpdate {
                            policies: pending,
                            full_replace: false,
                            remove_policy_ids: vec![],
                        })),
                    };
                    if tx_push.send(Ok(update)).await.is_err() {
                        tracing::warn!(%agent_id, "policy push channel closed");
                    }
                },
                Err(e) => {
                    tracing::error!(%agent_id, error = %e, "loading pending policies");
                },
            }
        });

        // ────────────────────────────────────────────────────────────────────
        // Inbound loop — Heartbeat / PolicyAck / inne future messages.
        // ────────────────────────────────────────────────────────────────────
        tokio::spawn(async move {
            while let Some(msg) = inbound.next().await {
                match msg {
                    Ok(AgentMessage {
                        payload: Some(agent_message::Payload::Heartbeat(hb)),
                    }) => {
                        tracing::debug!(%agent_id, version = %hb.agent_version, "heartbeat");

                        // Sub-faza 2E: aktualizuj agent_version przy każdym
                        // heartbeat — dashboard widzi outdated agents po
                        // self-update / mass deploy. Empty version = stary
                        // klient (proto przed pole 5) → zachowujemy starą
                        // wartość przez COALESCE.
                        if let Err(e) = sqlx::query(
                            "UPDATE agents SET last_seen = NOW(), \
                                              agent_version = COALESCE(NULLIF($1, ''), agent_version) \
                             WHERE id = $2",
                        )
                        .bind(&hb.agent_version)
                        .bind(agent_id)
                        .execute(&pool)
                        .await
                        {
                            tracing::warn!(error = %e, "updating last_seen/agent_version");
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
                    Ok(AgentMessage {
                        payload: Some(agent_message::Payload::PolicyAck(ack)),
                    }) => {
                        // Agent potwierdza że załadował policy (lub failed).
                        // status = 1 (APPLIED), 2-4 = różne błędy.
                        let version_i32 = i32::try_from(ack.version).unwrap_or(0);
                        if ack.status == 1 {
                            // APPLIED
                            if let Err(e) =
                                mark_acked(&pool, agent_id, &ack.policy_id, version_i32).await
                            {
                                tracing::warn!(error = %e, "mark_acked");
                            }
                            tracing::info!(
                                %agent_id,
                                policy = %ack.policy_id,
                                version = ack.version,
                                "PolicyAck APPLIED"
                            );
                        } else {
                            tracing::warn!(
                                %agent_id,
                                policy = %ack.policy_id,
                                version = ack.version,
                                status = ack.status,
                                error = %ack.error_message,
                                "PolicyAck FAILED"
                            );
                        }
                    },
                    Ok(AgentMessage {
                        payload: Some(agent_message::Payload::Events(batch)),
                    }) => {
                        let count = batch.events.len();
                        let batch_id = batch.batch_id.clone();
                        match insert_event_batch(&pool, agent_id, &batch).await {
                            Ok(inserted) => {
                                tracing::info!(
                                    %agent_id,
                                    %batch_id,
                                    count,
                                    inserted,
                                    "event batch received"
                                );
                            },
                            Err(e) => {
                                tracing::warn!(
                                    %agent_id,
                                    %batch_id,
                                    error = %e,
                                    "event batch insert failed"
                                );
                            },
                        }
                    },
                    Ok(AgentMessage {
                        payload: Some(agent_message::Payload::Event(ev)),
                    }) => {
                        // Pojedynczy event (proto wspiera oba — agent zwykle
                        // wysyła batch'em, ale bądźmy defensywni).
                        let synth = EventBatch {
                            batch_id: ev.event_id.clone(),
                            events: vec![ev],
                        };
                        if let Err(e) = insert_event_batch(&pool, agent_id, &synth).await {
                            tracing::warn!(%agent_id, error = %e, "single event insert failed");
                        }
                    },
                    Ok(_other) => {
                        tracing::debug!(%agent_id, "other inbound message (ignored in MVP)");
                    },
                    Err(e) => {
                        tracing::warn!(%agent_id, error = %e, "inbound error");
                        break;
                    },
                }
            }
            // Sub-faza 2E: unregister z registry przy disconnect — bridge
            // przestaje wysyłać commendy do tego agenta.
            registry.unregister(agent_id).await;
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

// ────────────────────────────────────────────────────────────────────────────
// Phase 3 — Event ingestion
// ────────────────────────────────────────────────────────────────────────────

/// Batch insert eventów do partycjonowanej tabeli `events` (M1 schema).
///
/// agent_id pochodzi z mTLS Subject CN — NIE z payload (payload `event.agent_id`
/// jest ignorowane, mTLS jest authoritative).
///
/// Mapowanie pól z proto Event → SQL kolumny:
/// - `event_id` (UUID string) → `id` (UUID)
/// - `timestamp_ns` → `timestamp` (TIMESTAMPTZ, z nanoseconds)
/// - `type` (EventType enum) → `event_type` (TEXT — `.as_str_name()`)
/// - `severity` enum → `severity` TEXT ("low"/"medium"/"high"/"critical")
/// - `direction` enum → `direction` TEXT ("inbound"/"outbound"/"internal")
/// - `matched_policy_id` → `matched_policy` (lub NULL gdy empty string)
/// - `matched_rule_id` → `matched_rule` (lub NULL)
/// - `action_taken` enum → `action_taken` TEXT (lub NULL)
/// - `details` (oneof) → `details` JSONB (per-variant serialize)
/// - `forward_to_siem` → `forwarded_to_syslog` (zostaje FALSE, manager
///   forward'uje osobno przez worker'a w przyszłości)
///
/// Zwraca liczbę zapisanych rekordów. Pojedyncze błędy decode'a per-event
/// są logowane i pomijane (best-effort batch).
async fn insert_event_batch(
    pool: &PgPool,
    agent_id: Uuid,
    batch: &EventBatch,
) -> Result<usize, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let mut inserted = 0usize;

    for ev in &batch.events {
        // event_id: jeśli pusty albo invalid → wygeneruj nowy UUID server-side.
        let id = Uuid::parse_str(&ev.event_id).unwrap_or_else(|_| Uuid::new_v4());
        let ts = chrono::DateTime::<chrono::Utc>::from_timestamp_nanos(ev.timestamp_ns);

        let event_type = event_type_to_str(ev.r#type);
        let severity = severity_to_str(ev.severity);
        let direction = direction_to_str_opt(ev.direction);

        let matched_policy = if ev.matched_policy_id.is_empty() {
            None
        } else {
            Some(ev.matched_policy_id.as_str())
        };
        let matched_rule = if ev.matched_rule_id.is_empty() {
            None
        } else {
            Some(ev.matched_rule_id.as_str())
        };
        let action_taken = action_to_str_opt(ev.action_taken);

        // Details: serialize cały Event do JSON żeby zachować pełen oneof
        // payload + matches. Pole `details` jest typu `Option<event::Details>`,
        // ale chcemy też matches[]/user_name/process_*  w JSON. Trick: cały Event
        // serialize'uje się przez prost-build z `prost(skip)` na niektórych
        // polach, więc używamy `serde_json::to_value` na podstrukturach.
        // Najprostsze: ręczny JSON z pól które mają sens dla dashboardu.
        let details_json = build_details_json(ev);

        let result = sqlx::query(
            "INSERT INTO events (
                id, timestamp, agent_id, event_type, severity, direction,
                matched_policy, matched_rule, action_taken, details, forwarded_to_syslog
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, FALSE)
             ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(ts)
        .bind(agent_id)
        .bind(event_type)
        .bind(severity)
        .bind(direction)
        .bind(matched_policy)
        .bind(matched_rule)
        .bind(action_taken)
        .bind(&details_json)
        .execute(&mut *tx)
        .await?;

        if result.rows_affected() > 0 {
            inserted += 1;
        }
    }

    tx.commit().await?;
    Ok(inserted)
}

fn event_type_to_str(value: i32) -> &'static str {
    use scrooge_proto::v1::EventType;
    EventType::try_from(value).map_or("EVENT_TYPE_UNSPECIFIED", |e| e.as_str_name())
}

fn severity_to_str(value: i32) -> &'static str {
    use scrooge_proto::v1::Severity;
    match Severity::try_from(value).unwrap_or(Severity::Unspecified) {
        Severity::Medium => "medium",
        Severity::High => "high",
        Severity::Critical => "critical",
        // M1 CHECK constraint dopuszcza low/medium/high/critical —
        // Unspecified mapujemy na "low" jako safe default.
        Severity::Low | Severity::Unspecified => "low",
    }
}

fn direction_to_str_opt(value: i32) -> Option<&'static str> {
    use scrooge_proto::v1::Direction;
    match Direction::try_from(value).ok()? {
        Direction::Inbound => Some("inbound"),
        Direction::Outbound => Some("outbound"),
        Direction::Internal => Some("internal"),
        Direction::Unspecified => None,
    }
}

fn action_to_str_opt(value: i32) -> Option<&'static str> {
    use scrooge_proto::v1::Action;
    match Action::try_from(value).ok()? {
        Action::Unspecified => None,
        Action::LogOnly => Some("log_only"),
        Action::Alert => Some("log_and_alert"),
        Action::WarnUser => Some("warn_user"),
        Action::Block => Some("block"),
        Action::Quarantine => Some("quarantine"),
        Action::KillProcess => Some("kill_process"),
        Action::LockWorkstation => Some("lock_workstation"),
    }
}

/// Buduje JSONB z najważniejszych pól eventu — używane przez dashboard
/// do filtrowania i wyświetlania. Klucze stabilne (front-end ma się na nich
/// opierać).
fn build_details_json(ev: &scrooge_proto::v1::Event) -> serde_json::Value {
    use scrooge_proto::v1::event::Details;
    let mut obj = serde_json::Map::new();
    if !ev.user_name.is_empty() {
        obj.insert("user_name".into(), ev.user_name.clone().into());
    }
    if ev.process_id != 0 {
        obj.insert("process_id".into(), ev.process_id.into());
    }
    if !ev.process_name.is_empty() {
        obj.insert("process_name".into(), ev.process_name.clone().into());
    }

    // Matches — lista classifier-trafień.
    if !ev.matches.is_empty() {
        let arr: Vec<serde_json::Value> = ev
            .matches
            .iter()
            .map(|m| {
                serde_json::json!({
                    "classifier_id": m.classifier_id,
                    "match_count": m.match_count,
                    "sample_excerpt": m.sample_excerpt,
                })
            })
            .collect();
        obj.insert("matches".into(), serde_json::Value::Array(arr));
    }

    // Details per-domena.
    if let Some(d) = &ev.details {
        match d {
            Details::Clipboard(c) => {
                obj.insert(
                    "clipboard".into(),
                    serde_json::json!({
                        "content_size": c.content_size,
                        "content_preview": c.content_preview,
                        "mime_type": c.mime_type,
                        "source_process_name": c.source_process_name,
                        "was_cleared": c.was_cleared,
                    }),
                );
            },
            Details::File(f) => {
                obj.insert(
                    "file".into(),
                    serde_json::json!({
                        "path": f.path,
                        "size_bytes": f.size_bytes,
                        "mime_type": f.mime_type,
                    }),
                );
            },
            // Pozostałe oneof'y serializujemy proste przez Debug — nie
            // używane jeszcze w MVP, ale ratuje audit gdy wjadą.
            other => {
                obj.insert("other_details".into(), format!("{other:?}").into());
            },
        }
    }

    serde_json::Value::Object(obj)
}
