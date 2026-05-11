// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! Enrollment flow: agent → manager `Enroll` RPC.
//!
//! Kroki:
//! 1. Wczytaj root CA managera (`ca_cert_path` z configu) — pinning.
//! 2. Zbuduj `tonic::transport::Channel` z TLS (CA pinning, `domain_name` z
//!    parsed endpoint host).
//! 3. Wygeneruj keypair + CSR przez `scrooge_common::ca::generate_agent_csr`.
//! 4. Wyślij `EnrollRequest` (token + CSR + AgentInfo).
//! 5. Zwróć `AgentState` (agent_id z response + cert + key).

use std::path::PathBuf;

use anyhow::{Context, Result};
use scrooge_agent_core::SystemInfo;
use scrooge_common::ca::generate_agent_csr;
use scrooge_proto::v1::{agent_service_client::AgentServiceClient, AgentInfo, EnrollRequest};
use tonic::transport::{Certificate, Channel, ClientTlsConfig};
use uuid::Uuid;

use crate::state::AgentState;

pub(crate) struct EnrollmentParams {
    pub endpoint: String,
    pub ca_cert_path: PathBuf,
    pub enrollment_token: String,
}

pub(crate) async fn enroll(params: EnrollmentParams, info: &SystemInfo) -> Result<AgentState> {
    let ca_pem = std::fs::read(&params.ca_cert_path)
        .with_context(|| format!("reading CA cert from {}", params.ca_cert_path.display()))?;
    let ca_cert = Certificate::from_pem(ca_pem);

    let host = params
        .endpoint
        .split(':')
        .next()
        .ok_or_else(|| anyhow::anyhow!("invalid manager endpoint `{}`", params.endpoint))?
        .to_string();

    let tls = ClientTlsConfig::new()
        .ca_certificate(ca_cert)
        .domain_name(host);
    let uri = format!("https://{}", params.endpoint);

    tracing::info!(endpoint = %params.endpoint, "connecting to manager (Enroll)");
    let channel = Channel::from_shared(uri)
        .context("parsing manager endpoint")?
        .tls_config(tls)
        .context("applying TLS config")?
        .connect()
        .await
        .context("connecting to manager gRPC")?;

    let mut client = AgentServiceClient::new(channel);

    let (csr_pem, key_pem) = generate_agent_csr(&info.hostname).context("generating CSR")?;

    let request = EnrollRequest {
        enrollment_token: params.enrollment_token,
        info: Some(AgentInfo {
            hostname: info.hostname.clone(),
            os: info.os.clone(),
            os_version: info.os_version.clone(),
            arch: info.arch.clone(),
            agent_version: info.agent_version.clone(),
            initial_tags: std::collections::HashMap::new(),
        }),
        csr_pem: csr_pem.into_bytes(),
    };

    let response = client.enroll(request).await.context("Enroll RPC failed")?;
    let resp = response.into_inner();

    let agent_id = Uuid::parse_str(&resp.agent_id).context("parsing agent_id from response")?;
    let cert_pem = String::from_utf8(resp.cert_pem).context("cert_pem not valid UTF-8")?;

    tracing::info!(%agent_id, "enrolled with manager");

    Ok(AgentState {
        agent_id,
        cert_pem,
        key_pem,
    })
}
