//! Hook event adapters and delivery sinks.

use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) mod webhook;

use lsi_core::hooks::{HookEvent, HookEventType};
use serde_json::json;
use uuid::Uuid;

use crate::events::DaemonEvent;

pub(crate) fn hook_event_from_daemon(event: DaemonEvent) -> HookEvent {
    HookEvent {
        event_id: Uuid::new_v4().to_string(),
        event_type: hook_event_type(&event),
        occurred_at_unix_seconds: current_unix_seconds(),
        payload: hook_payload(&event),
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

fn current_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use lsi_core::hooks::HookEventType;

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
}
