//! Hook event adapters, delivery sinks, and dispatcher runtime.

#[cfg(test)]
use std::time::{SystemTime, UNIX_EPOCH};
use std::{sync::Arc, time::Duration};

pub(crate) mod exec;
pub(crate) mod webhook;

use anyhow::Result;
use lsi_core::{
    config::{AppConfig, HookConfig},
    hooks::{HookEvent, HookEventType},
};
use serde_json::json;
use tokio::{sync::watch, task::JoinHandle};
use tracing::warn;
#[cfg(test)]
use uuid::Uuid;

use crate::events::{DaemonEvent, DaemonEventEnvelope, EventBus};
use exec::ExecSink;
use webhook::WebhookSink;

const HOOK_DELIVERY_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) struct HookDispatcherRuntime {
    shutdown_tx: watch::Sender<bool>,
    task: JoinHandle<()>,
}

pub(crate) fn start_hook_dispatcher(
    config: &AppConfig,
    events: &EventBus,
) -> HookDispatcherRuntime {
    let sinks = HookSinks::from_config(config);
    let rx = events.subscribe();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let task = tokio::spawn(run_dispatcher(rx, shutdown_rx, sinks));

    HookDispatcherRuntime { shutdown_tx, task }
}

pub(crate) async fn stop_hook_dispatcher(runtime: HookDispatcherRuntime) {
    let _ = runtime.shutdown_tx.send(true);
    if let Err(error) = runtime.task.await {
        warn!(%error, "hook dispatcher task panicked");
    }
}

#[cfg(test)]
pub(crate) fn hook_event_from_daemon(event: DaemonEvent) -> HookEvent {
    HookEvent {
        event_id: Uuid::new_v4().to_string(),
        event_type: hook_event_type(&event),
        occurred_at_unix_seconds: current_unix_seconds(),
        payload: hook_payload(&event),
    }
}

pub(crate) fn hook_event_from_envelope(envelope: &DaemonEventEnvelope) -> HookEvent {
    HookEvent {
        event_id: envelope.event_id().to_string(),
        event_type: hook_event_type(envelope.event()),
        occurred_at_unix_seconds: envelope.occurred_at_unix_seconds(),
        payload: hook_payload(envelope.event()),
    }
}

async fn run_dispatcher(
    mut rx: tokio::sync::broadcast::Receiver<DaemonEventEnvelope>,
    mut shutdown_rx: watch::Receiver<bool>,
    sinks: HookSinks,
) {
    loop {
        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    break;
                }
            }
            received = rx.recv() => {
                match received {
                    Ok(envelope) => sinks.dispatch(hook_event_from_envelope(&envelope)),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!(skipped, "hook dispatcher skipped lagged daemon events");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

#[derive(Clone)]
struct HookSinks {
    sinks: Arc<Vec<HookSink>>,
}

impl HookSinks {
    fn from_config(config: &AppConfig) -> Self {
        let sinks = config
            .hooks
            .iter()
            .map(|hook| match hook {
                HookConfig::Webhook { url, secret, max_attempts } => {
                    HookSink::Webhook(WebhookSink::new(url.clone(), secret.clone(), *max_attempts))
                }
                HookConfig::Exec { command, timeout_seconds } => HookSink::Exec(ExecSink::new(
                    command.clone(),
                    Duration::from_secs(*timeout_seconds),
                )),
            })
            .collect();

        Self { sinks: Arc::new(sinks) }
    }

    fn dispatch(&self, event: HookEvent) {
        for sink in self.sinks.iter() {
            let sink = sink.clone();
            let event = event.clone();
            tokio::spawn(async move {
                if let Err(error) =
                    tokio::time::timeout(HOOK_DELIVERY_TIMEOUT, sink.deliver(&event))
                        .await
                        .unwrap_or_else(|_| Err(anyhow::anyhow!("hook delivery timed out")))
                {
                    warn!(%error, event_id = %event.event_id, event_type = event.event_type.as_str(), "hook delivery failed");
                }
            });
        }
    }
}

#[derive(Clone)]
enum HookSink {
    Webhook(WebhookSink),
    Exec(ExecSink),
    #[cfg(test)]
    Fake(Arc<tokio::sync::Mutex<Vec<HookEvent>>>),
}

impl HookSink {
    async fn deliver(&self, event: &HookEvent) -> Result<()> {
        match self {
            Self::Webhook(sink) => sink.deliver(event).await,
            Self::Exec(sink) => sink.deliver(event).await,
            #[cfg(test)]
            Self::Fake(events) => {
                events.lock().await.push(event.clone());
                Ok(())
            }
        }
    }
}

fn hook_event_type(event: &DaemonEvent) -> HookEventType {
    match event {
        DaemonEvent::TransferStarted { .. } => HookEventType::TransferStarted,
        DaemonEvent::TransferProgress { .. } => HookEventType::TransferProgress,
        DaemonEvent::TransferCompleted { .. } => HookEventType::TransferCompleted,
        DaemonEvent::TransferFailed { .. } => HookEventType::TransferFailed,
        DaemonEvent::InboxChanged => HookEventType::InboxChanged,
        DaemonEvent::PeerSeen { .. } => HookEventType::PeerSeen,
    }
}

fn hook_payload(event: &DaemonEvent) -> serde_json::Value {
    match event {
        DaemonEvent::TransferStarted { transfer_id } => {
            json!({ "transfer_id": transfer_id })
        }
        DaemonEvent::TransferProgress { transfer_id, bytes_done, bytes_total } => json!({
            "transfer_id": transfer_id,
            "bytes_done": bytes_done,
            "bytes_total": bytes_total,
        }),
        DaemonEvent::TransferCompleted { transfer_id } => {
            json!({ "transfer_id": transfer_id })
        }
        DaemonEvent::TransferFailed { transfer_id, code, message } => json!({
            "transfer_id": transfer_id,
            "code": code,
            "message": message,
        }),
        DaemonEvent::InboxChanged => json!({}),
        DaemonEvent::PeerSeen { alias, address, port } => json!({
            "alias": alias,
            "address": address,
            "port": port,
        }),
    }
}

#[cfg(test)]
fn current_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use lsi_core::hooks::HookEventType;
    use tokio::time::timeout;

    use super::*;

    #[test]
    fn transfer_started_maps_to_hook_event() {
        let event =
            hook_event_from_daemon(DaemonEvent::TransferStarted { transfer_id: "tx-1".into() });

        assert_eq!(event.event_type, HookEventType::TransferStarted);
        assert_eq!(event.payload["transfer_id"], "tx-1");
        assert!(!event.event_id.is_empty());
        assert!(event.occurred_at_unix_seconds > 0);
    }

    #[test]
    fn transfer_progress_maps_to_hook_event() {
        let event = hook_event_from_daemon(DaemonEvent::TransferProgress {
            transfer_id: "tx-1".into(),
            bytes_done: 64,
            bytes_total: 128,
        });

        assert_eq!(event.event_type, HookEventType::TransferProgress);
        assert_eq!(event.payload["transfer_id"], "tx-1");
        assert_eq!(event.payload["bytes_done"], 64);
        assert_eq!(event.payload["bytes_total"], 128);
    }

    #[test]
    fn transfer_failed_maps_to_hook_event() {
        let event = hook_event_from_daemon(DaemonEvent::TransferFailed {
            transfer_id: "tx-1".into(),
            code: "io".into(),
            message: "disk full".into(),
        });

        assert_eq!(event.event_type, HookEventType::TransferFailed);
        assert_eq!(event.payload["transfer_id"], "tx-1");
        assert_eq!(event.payload["code"], "io");
        assert_eq!(event.payload["message"], "disk full");
    }

    #[test]
    fn inbox_changed_maps_to_empty_payload() {
        let event = hook_event_from_daemon(DaemonEvent::InboxChanged);

        assert_eq!(event.event_type, HookEventType::InboxChanged);
        assert_eq!(event.payload, json!({}));
    }

    #[test]
    fn peer_seen_maps_to_hook_event() {
        let event = hook_event_from_daemon(DaemonEvent::PeerSeen {
            alias: "phone".into(),
            address: "127.0.0.1".into(),
            port: 53317,
        });

        assert_eq!(event.event_type, HookEventType::PeerSeen);
        assert_eq!(event.payload["alias"], "phone");
        assert_eq!(event.payload["address"], "127.0.0.1");
        assert_eq!(event.payload["port"], 53317);
    }

    #[tokio::test]
    async fn dispatcher_sends_event_to_configured_sink() {
        let bus = EventBus::new();
        let recorded = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let sinks = HookSinks { sinks: Arc::new(vec![HookSink::Fake(Arc::clone(&recorded))]) };
        let (_shutdown_tx, shutdown_rx) = watch::channel(false);
        let dispatcher = tokio::spawn(run_dispatcher(bus.subscribe(), shutdown_rx, sinks));

        bus.emit(DaemonEvent::InboxChanged);

        timeout(Duration::from_secs(1), async {
            loop {
                if recorded.lock().await.len() == 1 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();

        let events = recorded.lock().await;
        assert_eq!(events[0].event_type, HookEventType::InboxChanged);
        assert_eq!(events[0].payload, json!({}));
        dispatcher.abort();
    }
}
