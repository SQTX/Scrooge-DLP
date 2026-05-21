// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! Background task batch-pushing local sqlite event queue do managera
//! przez gRPC Stream. Spawnowane per-session (run_one_session) — gdy
//! Stream się rozłącza, task kończy się i jest restartowany przy
//! następnym session'ie. EventQueue żyje globalnie, więc nieparsowane
//! eventy są zachowane przez reconnect.

use std::{sync::Arc, time::Duration};

use scrooge_agent_core::queue::EventQueue;
use scrooge_proto::v1::{agent_message, AgentMessage, EventBatch};
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

/// Jak często sprawdzamy kolejkę — kompromis latency vs traffic.
pub(crate) const TICK_INTERVAL: Duration = Duration::from_secs(5);

/// Max eventów w jednym EventBatch. Większe batch'e = mniej overhead,
/// ale ryzyko że pojedynczy fail straci dużo (gRPC nie ma per-event ack
/// — całość albo nic).
pub(crate) const BATCH_LIMIT: u32 = 50;

/// Spawnuje task w obrębie session'a. Wraca gdy channel `tx` zamknięty
/// lub shutdown signal otrzymany.
pub(crate) fn spawn(
    queue: Arc<EventQueue>,
    tx: mpsc::Sender<AgentMessage>,
    mut shutdown: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(TICK_INTERVAL);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = tick.tick() => {
                    if let Err(e) = drain_once(&queue, &tx).await {
                        // Reasons: kolejka closed, gRPC channel closed.
                        // Brak retry w petli — gdy stream offline, task kończy się,
                        // nowy spawn przy reconnect podejmie pracę.
                        tracing::debug!(error = %e, "event_uploader: drain failed, exiting task");
                        return;
                    }
                }
                _ = shutdown.changed() => {
                    tracing::debug!("event_uploader: shutdown signal");
                    return;
                }
            }
        }
    })
}

/// Jeden cykl: take_batch → send → mark_sent.
///
/// W obecnym MVP gRPC Stream nie ma per-batch ack (manager wpisuje batch
/// w handlerze, jeśli stream zamknięty = manager nie odebrał). Zakładamy
/// fire-and-forget — `mark_sent` od razu po `tx.send().await`. Jeśli
/// connection padnie pomiędzy send a manager-side insert, te eventy
/// mogą zniknąć. Production w v0.3.x doda explicit EventAck w Stream.
async fn drain_once(
    queue: &EventQueue,
    tx: &mpsc::Sender<AgentMessage>,
) -> anyhow::Result<()> {
    let batch = queue.take_batch(BATCH_LIMIT).await?;
    if batch.is_empty() {
        return Ok(());
    }

    let ids: Vec<i64> = batch.iter().map(|p| p.id).collect();
    let events: Vec<scrooge_proto::v1::Event> =
        batch.into_iter().map(|p| p.event).collect();

    let msg = AgentMessage {
        payload: Some(agent_message::Payload::Events(EventBatch {
            batch_id: Uuid::new_v4().to_string(),
            events,
        })),
    };

    tx.send(msg)
        .await
        .map_err(|_| anyhow::anyhow!("stream channel closed"))?;
    queue.mark_sent(&ids).await?;
    tracing::debug!(count = ids.len(), "event_uploader: batch sent");
    Ok(())
}
