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

use std::{path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use clap::Parser;
use scrooge_agent_core::{collect_system_info, queue::EventQueue, PlatformAgent};
use scrooge_common::config::AgentConfig;
use scrooge_proto::v1::Event;
use tokio::sync::{mpsc, watch};

mod enroll;
mod event_uploader;
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

    /// Dev only: wstrzykuje sztuczny clipboard event (kart kredytowa
    /// Visa 4532015112830366 redacted) do lokalnej event queue i wychodzi.
    /// Działający scrooge-agent service podbierze go w następnym tick'u
    /// event_uploader (5s) i wypchnie do managera. Headless VM friendly —
    /// nie wymaga DISPLAY ani clipboard hook'u.
    ///
    /// Liczba eventów do wstrzyknięcia (default 1).
    #[arg(long, value_name = "COUNT")]
    emit_test_event: Option<u32>,
}

#[tokio::main]
#[allow(clippy::too_many_lines)]
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

    // Dev: --emit-test-event <N> — wstrzyknij N sztucznych clipboard eventów
    // do sqlite queue i wyjdź. Działający scrooge-agent service podbierze
    // je w next tick (5s) i wypchnie do managera.
    if let Some(count) = args.emit_test_event {
        let queue = scrooge_agent_core::queue::EventQueue::open(&data_dir.join("events.db"))
            .await
            .context("opening event queue")?;
        for i in 0..count {
            let ev = build_test_event(i);
            let id = queue.enqueue(&ev).await.context("enqueue test event")?;
            tracing::info!(id, "test event enqueued");
        }
        println!("✔ wstrzyknięto {count} test event(ów) do {}/events.db", data_dir.display());
        println!("ℹ  scrooge-agent service je wypcha w next event_uploader tick (~5s).");
        println!("ℹ  Verify w dashboardzie: Events tab → Refresh.");
        return Ok(());
    }

    // Inicjalizacja platformowego agenta (cfg-gated). Phase 3 M4.2:
    // przekazujemy classifier registry + watch_paths z agent.yaml.
    let classifiers = std::sync::Arc::new(
        scrooge_agent_core::modules::classifier::ClassifierRegistry::with_defaults(),
    );
    let watch_paths: Vec<std::path::PathBuf> = config
        .agent
        .watch_paths
        .iter()
        .map(std::path::PathBuf::from)
        .collect();
    let platform: Box<dyn PlatformAgent> =
        Box::new(PlatformImpl::new(classifiers, watch_paths));
    platform.init().await.context("platform agent init")?;

    // Phase 3: lokalna kolejka eventów (sqlite WAL).
    let queue = Arc::new(
        EventQueue::open(&data_dir.join("events.db"))
            .await
            .context("opening event queue")?,
    );

    // Channel pomiędzy modułami platformowymi (clipboard / future filemon / ...)
    // a kolejką. Buffer 256: ~5s ruchu przy szczytowych poll'ach.
    let (event_tx, mut event_rx) = mpsc::channel::<Event>(256);

    // Collector — wpycha każdy emitowany Event do sqlite. Działa do końca
    // życia procesu; gdy channel zamknięty (modules drop), exit.
    {
        let q = Arc::clone(&queue);
        tokio::spawn(async move {
            while let Some(ev) = event_rx.recv().await {
                if let Err(e) = q.enqueue(&ev).await {
                    tracing::warn!(error = %e, "event queue enqueue failed");
                }
            }
            tracing::info!("event collector task exiting (channel closed)");
        });
    }

    // Aktywacja modułów detekcji.
    platform
        .start_clipscreen(event_tx.clone())
        .await
        .context("start_clipscreen")?;
    platform
        .start_filemon(event_tx.clone())
        .await
        .context("start_filemon")?;

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
        queue: Arc::clone(&queue),
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


/// Sztuczny clipboard event z wbudowanym Visa test number (Luhn-valid).
/// Używane przez `--emit-test-event` dla weryfikacji event pipeline na
/// headless VM (bez DISPLAY/clipboard).
fn build_test_event(idx: u32) -> scrooge_proto::v1::Event {
    use scrooge_proto::v1::{event::Details, ClipboardEvent, Event, EventType, Match, Severity};
    Event {
        event_id: uuid::Uuid::new_v4().to_string(),
        timestamp_ns: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
        agent_id: String::new(), // mTLS Subject CN authoritative
        r#type: EventType::ClipboardCopy as i32,
        severity: Severity::Medium as i32,
        direction: 0,
        user_name: format!("test-user-{idx}"),
        process_id: 0,
        process_name: "scrooge-agent --emit-test-event".to_string(),
        details: Some(Details::Clipboard(ClipboardEvent {
            content_size: 16,
            content_preview: "**** **** **** 0366".to_string(),
            content_hash: vec![],
            mime_type: "text/plain".to_string(),
            source_process_name: String::new(),
            was_cleared: false,
        })),
        matched_policy_id: String::new(),
        matched_rule_id: String::new(),
        action_taken: 0,
        matches: vec![Match {
            classifier_id: "credit_card".to_string(),
            match_count: 1,
            sample_excerpt: "**** **** **** 0366".to_string(),
        }],
        forward_to_siem: false,
    }
}
