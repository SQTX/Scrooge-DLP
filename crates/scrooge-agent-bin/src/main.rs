// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 2 as
// published by the Free Software Foundation.

//! `scrooge-agent` — binarka agenta ScroogeDLP.
//!
//! Krok 8:
//! 1. Parse args + load config (`agent.yaml`).
//! 2. Inicjalizacja `tracing`.
//! 3. Inicjalizacja platformowego agenta (cfg-gated Linux/macOS/Windows).
//! 4. Detekcja first-run: jeśli `data_dir/agent.cert` nie istnieje, wykonaj
//!    enrollment przez `Enroll` RPC.
//! 5. Otwórz persistent `Stream` z heartbeat + auto-reconnect.
//! 6. SIGINT/SIGTERM → graceful shutdown.

use std::{path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use clap::Parser;
use scrooge_agent_core::{collect_system_info, PlatformAgent};
use scrooge_common::config::AgentConfig;
use tokio::sync::watch;

mod enroll;
mod state;
mod stream;
mod telemetry;

use state::AgentState;

#[cfg(target_os = "linux")]
type PlatformImpl = scrooge_agent_linux::LinuxAgent;
#[cfg(target_os = "macos")]
type PlatformImpl = scrooge_agent_macos::MacosAgent;
#[cfg(target_os = "windows")]
type PlatformImpl = scrooge_agent_windows::WindowsAgent;

#[derive(Debug, Parser)]
#[command(name = "scrooge-agent", version, about = "ScroogeDLP endpoint agent")]
struct Args {
    /// Ścieżka do pliku konfiguracyjnego agenta (YAML).
    #[arg(
        short,
        long,
        env = "AGENT_CONFIG",
        default_value = "/etc/scrooge/agent.yaml"
    )]
    config: PathBuf,

    /// Format logów.
    #[arg(long, value_enum, env = "LOG_FORMAT", default_value = "json")]
    log_format: telemetry::LogFormat,

    /// Override `manager.enrollment_token` z configu. Przydatne przy `make
    /// dev-agent` (token z `dev-certs/enrollment-token.txt` przez env).
    #[arg(long, env = "ENROLLMENT_TOKEN")]
    enrollment_token: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    telemetry::init(args.log_format);

    // Rustls 0.23+ wymaga explicit wyboru CryptoProvider (process-wide).
    // Agent używa rustls przez tonic dla gRPC enrollment + Stream (mTLS).
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("installing ring CryptoProvider for rustls");

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        config = %args.config.display(),
        "scrooge-agent starting",
    );

    let mut config = AgentConfig::load(&args.config).context("loading agent config")?;
    if let Some(tok) = args.enrollment_token {
        config.manager.enrollment_token = Some(tok);
    }

    let data_dir = PathBuf::from(&config.agent.data_dir);
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("creating data_dir {}", data_dir.display()))?;

    // Inicjalizacja platformowego agenta (cfg-gated).
    let platform: Box<dyn PlatformAgent> = Box::new(PlatformImpl::default());
    platform.init().await.context("platform agent init")?;

    // Zbierz info o systemie.
    let info = collect_system_info(env!("CARGO_PKG_VERSION")).context("collecting system info")?;
    tracing::info!(
        hostname = %info.hostname,
        os = %info.os,
        arch = %info.arch,
        "system info collected",
    );

    // First-run detection + enrollment.
    let state = if let Some(existing) = AgentState::load(&data_dir)? {
        tracing::info!(agent_id = %existing.agent_id, "loaded existing enrollment");
        existing
    } else {
        tracing::info!("first run — enrolling with manager");
        let token = config.manager.enrollment_token.clone().ok_or_else(|| {
            anyhow::anyhow!(
                "first run requires `manager.enrollment_token` w configu \
                 (lub ENROLLMENT_TOKEN env var)"
            )
        })?;
        let fresh = enroll::enroll(
            enroll::EnrollmentParams {
                endpoint: config.manager.endpoint.clone(),
                ca_cert_path: PathBuf::from(&config.manager.ca_cert_path),
                enrollment_token: token,
            },
            &info,
        )
        .await?;
        fresh.save(&data_dir)?;
        tracing::info!(agent_id = %fresh.agent_id, "enrollment saved to {}", data_dir.display());
        fresh
    };

    // Shared shutdown.
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    tokio::spawn(async move {
        shutdown_signal().await;
        let _ = shutdown_tx.send(true);
    });

    // Persistent stream + auto-reconnect.
    let stream_cfg = stream::StreamConfig {
        endpoint: config.manager.endpoint.clone(),
        ca_cert_path: PathBuf::from(&config.manager.ca_cert_path),
        // mTLS — agent prezentuje swój cert (podpisany przez manager CA
        // przy enrollment). state.rs zapisuje pliki w data_dir/agent.cert
        // i data_dir/agent.key.
        cert_path: data_dir.join("agent.cert"),
        key_path: data_dir.join("agent.key"),
        // Polityki otrzymane od managera (Sub-faza 2C) lądują tutaj jako
        // pliki msgpack — jeden per policy name.
        policies_dir: data_dir.join("policies"),
        agent_id: state.agent_id,
        heartbeat_interval: Duration::from_secs(u64::from(config.agent.heartbeat_interval_secs)),
    };

    let stream_result = stream::run_with_reconnect(stream_cfg, shutdown_rx).await;

    // Graceful shutdown platformowych modułów.
    if let Err(e) = platform.shutdown().await {
        tracing::warn!(error = %e, "platform shutdown error");
    }

    stream_result?;
    tracing::info!("scrooge-agent stopped cleanly");
    Ok(())
}

async fn shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl-C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => tracing::info!("Ctrl+C received, shutting down"),
        () = terminate => tracing::info!("SIGTERM received, shutting down"),
    }
}
