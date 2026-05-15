// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! Persistent `Stream` RPC z managerem:
//! - heartbeat task w pętli (interval z config),
//! - inbound reader przyjmujący `ManagerMessage` (heartbeat ack / policy / komendy),
//! - exponential backoff reconnect przy disconnectach,
//! - graceful shutdown przez `watch` channel.

use std::{path::Path, path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use scrooge_proto::v1::{
    agent_message, agent_service_client::AgentServiceClient, manager_message, AgentMessage,
    AgentStatus, Heartbeat, PolicyAck,
};
use tokio::sync::{mpsc, watch};
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tonic::{
    transport::{Certificate, Channel, ClientTlsConfig, Identity},
    Request,
};
use uuid::Uuid;

const RECONNECT_MIN: Duration = Duration::from_secs(1);
const RECONNECT_MAX: Duration = Duration::from_secs(60);
const CHANNEL_CAPACITY: usize = 64;

pub(crate) struct StreamConfig {
    pub endpoint: String,
    pub ca_cert_path: PathBuf,
    /// Cert agenta (podpisany przez manager CA przy enrollment) — do mTLS.
    pub cert_path: PathBuf,
    /// Klucz prywatny agenta (generowany lokalnie przy enrollment, nigdy
    /// nie opuszcza endpointa).
    pub key_path: PathBuf,
    /// Katalog do zapisu polityk otrzymanych z managera (msgpack files
    /// + metadata). Zwykle `{data_dir}/policies/`.
    pub policies_dir: PathBuf,
    pub agent_id: Uuid,
    pub heartbeat_interval: Duration,
}

/// Główna pętla: connect → run stream → on disconnect, exponential backoff,
/// retry. Wraca `Ok(())` po sygnale shutdown.
pub(crate) async fn run_with_reconnect(
    cfg: StreamConfig,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    let mut backoff = RECONNECT_MIN;

    loop {
        if *shutdown.borrow() {
            return Ok(());
        }

        match build_channel(
            &cfg.endpoint,
            &cfg.ca_cert_path,
            &cfg.cert_path,
            &cfg.key_path,
        )
        .await
        {
            Ok(channel) => match run_one_session(channel, &cfg, shutdown.clone()).await {
                Ok(()) => {
                    tracing::info!("stream ended cleanly, reconnecting in 1s");
                    backoff = RECONNECT_MIN;
                },
                Err(e) => {
                    tracing::warn!(error = %e, backoff_secs = backoff.as_secs(), "stream session error");
                },
            },
            Err(e) => {
                tracing::warn!(error = %e, backoff_secs = backoff.as_secs(), "connect failed");
            },
        }

        tokio::select! {
            () = tokio::time::sleep(backoff) => {},
            _ = shutdown.changed() => return Ok(()),
        }

        backoff = (backoff * 2).min(RECONNECT_MAX);
    }
}

async fn build_channel(
    endpoint: &str,
    ca_cert_path: &Path,
    cert_path: &Path,
    key_path: &Path,
) -> Result<Channel> {
    let ca_pem = std::fs::read(ca_cert_path)
        .with_context(|| format!("reading CA cert from {}", ca_cert_path.display()))?;
    let cert_pem = std::fs::read(cert_path)
        .with_context(|| format!("reading agent cert from {}", cert_path.display()))?;
    let key_pem = std::fs::read(key_path)
        .with_context(|| format!("reading agent key from {}", key_path.display()))?;
    let host = endpoint
        .split(':')
        .next()
        .ok_or_else(|| anyhow::anyhow!("invalid endpoint"))?
        .to_string();

    // mTLS: prezentujemy nasz cert (podpisany przez manager CA przy
    // enrollment) — manager wyciągnie agent_id z Subject CN.
    let tls = ClientTlsConfig::new()
        .ca_certificate(Certificate::from_pem(ca_pem))
        .identity(Identity::from_pem(cert_pem, key_pem))
        .domain_name(host);
    let uri = format!("https://{endpoint}");

    Channel::from_shared(uri)
        .context("parsing endpoint")?
        .tls_config(tls)
        .context("TLS config")?
        .connect()
        .await
        .context("connecting")
}

async fn run_one_session(
    channel: Channel,
    cfg: &StreamConfig,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    let mut client = AgentServiceClient::new(channel);

    let (tx, rx) = mpsc::channel::<AgentMessage>(CHANNEL_CAPACITY);
    let outbound = ReceiverStream::new(rx);

    // mTLS: manager wyciąga agent_id z Subject CN naszego client cert'a.
    // `agent_id` w cfg trzymamy tylko dla lokalnego logowania.
    let request = Request::new(outbound);

    tracing::info!(endpoint = %cfg.endpoint, agent_id = %cfg.agent_id, "Stream opening (mTLS)");
    let response = client.stream(request).await.context("Stream RPC")?;
    let mut inbound = response.into_inner();

    // Heartbeat task.
    let tx_hb = tx.clone();
    let interval = cfg.heartbeat_interval;
    let mut hb_shutdown = shutdown.clone();
    let heartbeat = tokio::spawn(async move {
        let mut tick = tokio::time::interval(interval);
        // Pierwszy heartbeat od razu po połączeniu.
        loop {
            tokio::select! {
                _ = tick.tick() => {
                    let hb = AgentMessage {
                        payload: Some(agent_message::Payload::Heartbeat(Heartbeat {
                            timestamp_ns: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
                            status: Some(AgentStatus::default()),
                            active_policy_ids: vec![],
                            active_policy_versions: std::collections::HashMap::new(),
                            // Sub-faza 2E: manager updateuje agents.agent_version
                            // przy każdym heartbeat.
                            agent_version: env!("CARGO_PKG_VERSION").to_string(),
                        })),
                    };
                    if tx_hb.send(hb).await.is_err() {
                        tracing::debug!("heartbeat channel closed");
                        break;
                    }
                }
                _ = hb_shutdown.changed() => break,
            }
        }
    });

    // Inbound reader loop — handle Heartbeat ack, PolicyUpdate, Command.
    let policies_dir = cfg.policies_dir.clone();
    let tx_ack = tx.clone();
    let result = loop {
        tokio::select! {
            msg = inbound.next() => match msg {
                Some(Ok(m)) => {
                    handle_inbound(m, &policies_dir, &tx_ack).await;
                },
                Some(Err(e)) => break Err(anyhow::anyhow!("inbound error: {e}")),
                None => break Ok(()),
            },
            _ = shutdown.changed() => break Ok(()),
        }
    };

    heartbeat.abort();
    let _ = heartbeat.await;
    drop(tx);
    result
}

/// Routing przychodzących `ManagerMessage` (Sub-faza 2C).
///
/// Aktualnie:
/// - `HeartbeatAck` → tylko log (nie ma akcji)
/// - `PolicyUpdate` → zapisz każdy policy do `policies_dir/{name}.msgpack`,
///   odeślij `PolicyAck` z status=APPLIED (lub odpowiedni error)
/// - `Command` → log "not implemented" (do Sub-fazy 2E)
async fn handle_inbound(
    msg: scrooge_proto::v1::ManagerMessage,
    policies_dir: &Path,
    tx_ack: &mpsc::Sender<AgentMessage>,
) {
    match msg.payload {
        Some(manager_message::Payload::HeartbeatAck(_)) => {
            tracing::trace!("HeartbeatAck received");
        },
        Some(manager_message::Payload::Policy(update)) => {
            if let Err(e) = std::fs::create_dir_all(policies_dir) {
                tracing::error!(error = %e, dir = %policies_dir.display(), "creating policies dir");
                return;
            }
            tracing::info!(
                count = update.policies.len(),
                full_replace = update.full_replace,
                "PolicyUpdate received"
            );
            for policy_ref in &update.policies {
                let path = policies_dir.join(format!("{}.msgpack", policy_ref.policy_id));
                let (status, error_message) =
                    match std::fs::write(&path, &policy_ref.content_compiled) {
                        Ok(()) => {
                            tracing::info!(
                                policy = %policy_ref.policy_id,
                                version = policy_ref.version,
                                hash = %policy_ref.content_hash,
                                bytes = policy_ref.content_compiled.len(),
                                "policy applied (msgpack saved)"
                            );
                            (1, String::new()) // APPLIED
                        },
                        Err(e) => {
                            tracing::error!(
                                error = %e,
                                policy = %policy_ref.policy_id,
                                "failed to write policy msgpack"
                            );
                            (3, format!("write error: {e}")) // RUNTIME_FAILED
                        },
                    };

                let ack = AgentMessage {
                    payload: Some(agent_message::Payload::PolicyAck(PolicyAck {
                        policy_id: policy_ref.policy_id.clone(),
                        version: policy_ref.version,
                        status,
                        error_message,
                    })),
                };
                if tx_ack.send(ack).await.is_err() {
                    tracing::warn!("policy ack channel closed");
                    return;
                }
            }
        },
        Some(manager_message::Payload::Command(cmd)) => {
            tracing::info!(
                command_id = %cmd.command_id,
                cmd_type = cmd.r#type,
                "Command received — handler not implemented yet (Sub-faza 2E)"
            );
        },
        None => {
            tracing::warn!("ManagerMessage without payload (proto3 default)");
        },
    }
}
