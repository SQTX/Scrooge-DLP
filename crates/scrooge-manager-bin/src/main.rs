// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 2 as
// published by the Free Software Foundation.

//! `scrooge-manager` — binarka centralnego serwera ScroogeDLP.
//!
//! Krok 5–6 (szkielet):
//! 1. Parsuje args + ładuje config YAML.
//! 2. Inicjalizuje `tracing` (JSON w produkcji).
//! 3. Ładuje root CA (z `scrooge-common`).
//! 4. Otwiera pool PostgreSQL i aplikuje migracje (`sqlx::migrate!`).
//! 5. Startuje tonic gRPC server z TLS na `grpc_listen_addr`.
//! 6. Startuje axum REST API na `rest_listen_addr` (`scrooge-manager-api`).
//! 7. Czeka na SIGINT/SIGTERM → graceful shutdown obu serwerów.

use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use anyhow::Context;
use clap::Parser;
use scrooge_common::{ca::RootCa, config::ManagerConfig};
use scrooge_manager_api::AppState;
use scrooge_proto::v1::agent_service_server::AgentServiceServer;
use sqlx::postgres::PgPoolOptions;
use tokio::sync::watch;
use tonic::transport::Server;

mod grpc;
mod telemetry;

#[derive(Debug, Parser)]
#[command(name = "scrooge-manager", version, about = "ScroogeDLP central server")]
struct Args {
    /// Ścieżka do pliku konfiguracyjnego managera (YAML).
    #[arg(
        short,
        long,
        env = "MANAGER_CONFIG",
        default_value = "/etc/scrooge/manager.yaml"
    )]
    config: PathBuf,

    /// Format logów.
    #[arg(long, value_enum, env = "LOG_FORMAT", default_value = "json")]
    log_format: telemetry::LogFormat,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    telemetry::init(args.log_format);

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        config = %args.config.display(),
        "scrooge-manager starting",
    );

    let config = ManagerConfig::load(&args.config).context("loading manager config")?;

    // Załaduj root CA — używane do signowania CSR-ów w `AgentService::Enroll`.
    let ca = Arc::new(
        RootCa::load_from_files(&config.server.ca_cert_path, &config.server.ca_key_path)
            .context("loading root CA")?,
    );
    tracing::info!(cert_path = %config.server.ca_cert_path, "root CA loaded");

    // PostgreSQL pool.
    let pool = PgPoolOptions::new()
        .max_connections(config.database.pool_max)
        .connect(&config.database.url)
        .await
        .context("connecting to PostgreSQL")?;
    tracing::info!(
        pool_max = config.database.pool_max,
        "PostgreSQL pool initialized"
    );

    // Aplikuj migracje DB.
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .context("applying database migrations")?;
    tracing::info!("database migrations applied");

    // Parse addr-y.
    let grpc_addr: SocketAddr = config.server.grpc_listen_addr.parse().with_context(|| {
        format!(
            "parsing grpc_listen_addr `{}`",
            config.server.grpc_listen_addr
        )
    })?;
    let rest_addr: SocketAddr = config.server.rest_listen_addr.parse().with_context(|| {
        format!(
            "parsing rest_listen_addr `{}`",
            config.server.rest_listen_addr
        )
    })?;

    // Komponenty serwerów.
    let api_state = AppState::new(
        pool.clone(),
        &config.auth,
        config.dashboard.clone(),
        &config.server,
    );
    let tls = grpc::tls_config(&config.server).context("building TLS config")?;
    let grpc_service = grpc::AgentServiceImpl::new(pool.clone(), ca);
    let grpc_server = Server::builder()
        .tls_config(tls)
        .context("applying TLS config")?
        .add_service(AgentServiceServer::new(grpc_service));

    // Wspólny kanał shutdown — SIGINT/SIGTERM triggeruje oba serwery.
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let rest_shutdown = wait_for_shutdown(shutdown_rx.clone());
    let grpc_shutdown = wait_for_shutdown(shutdown_rx);

    tokio::spawn(async move {
        shutdown_signal().await;
        let _ = shutdown_tx.send(true);
    });

    // Spawn serwerów.
    tracing::info!(addr = %rest_addr, "REST API starting");
    let rest_task = tokio::spawn(scrooge_manager_api::serve(
        rest_addr,
        api_state,
        rest_shutdown,
    ));

    tracing::info!(addr = %grpc_addr, "gRPC server starting (server-side TLS)");
    let grpc_task = tokio::spawn(async move {
        grpc_server
            .serve_with_shutdown(grpc_addr, grpc_shutdown)
            .await
    });

    // Czekamy aż oba serwery zakończą się gracefully.
    let (rest_res, grpc_res) = tokio::join!(rest_task, grpc_task);
    rest_res
        .context("REST task panicked")?
        .context("REST server error")?;
    grpc_res
        .context("gRPC task panicked")?
        .context("gRPC server error")?;

    tracing::info!("scrooge-manager stopped cleanly");
    Ok(())
}

/// Czeka aż watch channel zostanie zsygnalizowany (`true`).
async fn wait_for_shutdown(mut rx: watch::Receiver<bool>) {
    let _ = rx.changed().await;
}

/// Czeka na SIGINT albo SIGTERM (Unix) / Ctrl+C (Windows).
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
