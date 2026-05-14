use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::{SystemTime, UNIX_EPOCH};

use lsi_proto::{
    common::v1::ErrorDetail,
    events::v1::{
        daemon_event, DaemonEvent as ProtoDaemonEvent, DaemonEventType, InboxChanged, PeerSeen,
        TransferCompleted, TransferFailed, TransferProgress, TransferStarted,
    },
    peers::v1::{LanPeer, PeerProtocol},
    transfers::v1::{ActiveTransfer, TransferDirection, TransferState},
};
use tokio::sync::broadcast;

const EVENT_CHANNEL_CAPACITY: usize = 256;

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) enum DaemonEvent {
    TransferStarted { transfer_id: String },
    TransferProgress { transfer_id: String, bytes_done: u64, bytes_total: u64 },
    TransferCompleted { transfer_id: String },
    TransferFailed { transfer_id: String, code: String, message: String },
    InboxChanged,
    PeerSeen { alias: String, address: String, port: u32 },
}

#[derive(Clone, Debug)]
pub(crate) struct DaemonEventEnvelope {
    event_id: String,
    occurred_at_unix_seconds: i64,
    event: DaemonEvent,
}

impl DaemonEventEnvelope {
    pub(crate) fn to_proto(&self) -> ProtoDaemonEvent {
        match &self.event {
            DaemonEvent::TransferStarted { transfer_id } => self.proto(
                DaemonEventType::TransferStarted,
                daemon_event::Payload::TransferStarted(TransferStarted {
                    transfer: Some(ActiveTransfer {
                        transfer_id: transfer_id.clone(),
                        peer_fingerprint: String::new(),
                        direction: TransferDirection::Unspecified as i32,
                        state: TransferState::Active as i32,
                        bytes_total: 0,
                        bytes_done: 0,
                        files: Vec::new(),
                        created_at_unix_seconds: self.occurred_at_unix_seconds,
                        updated_at_unix_seconds: self.occurred_at_unix_seconds,
                        last_error: None,
                    }),
                }),
            ),
            DaemonEvent::TransferProgress { transfer_id, bytes_done, bytes_total } => self.proto(
                DaemonEventType::TransferProgress,
                daemon_event::Payload::TransferProgress(TransferProgress {
                    transfer_id: transfer_id.clone(),
                    bytes_done: *bytes_done,
                    bytes_total: *bytes_total,
                    files: Vec::new(),
                }),
            ),
            DaemonEvent::TransferCompleted { transfer_id } => self.proto(
                DaemonEventType::TransferCompleted,
                daemon_event::Payload::TransferCompleted(TransferCompleted {
                    transfer_id: transfer_id.clone(),
                    inbox_entries: Vec::new(),
                }),
            ),
            DaemonEvent::TransferFailed { transfer_id, code, message } => self.proto(
                DaemonEventType::TransferFailed,
                daemon_event::Payload::TransferFailed(TransferFailed {
                    transfer_id: transfer_id.clone(),
                    error: Some(ErrorDetail { code: code.clone(), message: message.clone() }),
                }),
            ),
            DaemonEvent::InboxChanged => self.proto(
                DaemonEventType::InboxChanged,
                daemon_event::Payload::InboxChanged(InboxChanged { entries: Vec::new() }),
            ),
            DaemonEvent::PeerSeen { alias, address, port } => self.proto(
                DaemonEventType::PeerSeen,
                daemon_event::Payload::PeerSeen(PeerSeen {
                    peer: Some(LanPeer {
                        alias: alias.clone(),
                        address: address.clone(),
                        port: *port,
                        protocol: PeerProtocol::Unspecified as i32,
                        fingerprint: None,
                        device_model: None,
                        device_type: None,
                        download: false,
                        extensions: Vec::new(),
                    }),
                }),
            ),
        }
    }

    fn proto(
        &self,
        event_type: DaemonEventType,
        payload: daemon_event::Payload,
    ) -> ProtoDaemonEvent {
        ProtoDaemonEvent {
            event_id: self.event_id.clone(),
            occurred_at_unix_seconds: self.occurred_at_unix_seconds,
            r#type: event_type as i32,
            payload: Some(payload),
        }
    }
}

#[derive(Clone)]
pub(crate) struct EventBus {
    inner: Arc<EventBusInner>,
}

struct EventBusInner {
    tx: broadcast::Sender<DaemonEventEnvelope>,
    #[allow(dead_code)]
    next_id: AtomicU64,
}

impl EventBus {
    pub(crate) fn new() -> Self {
        let (tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Self { inner: Arc::new(EventBusInner { tx, next_id: AtomicU64::new(1) }) }
    }

    pub(crate) fn subscribe(&self) -> broadcast::Receiver<DaemonEventEnvelope> {
        self.inner.tx.subscribe()
    }

    #[allow(dead_code)]
    pub(crate) fn emit(&self, event: DaemonEvent) {
        let event_id = self.inner.next_id.fetch_add(1, Ordering::Relaxed).to_string();
        let occurred_at_unix_seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or_default();
        let _ =
            self.inner.tx.send(DaemonEventEnvelope { event_id, occurred_at_unix_seconds, event });
    }

    pub(crate) fn subscriber_count(&self) -> usize {
        self.inner.tx.receiver_count()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn subscribers_receive_emitted_events() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();

        bus.emit(DaemonEvent::InboxChanged);

        let event = rx.recv().await.unwrap().to_proto();
        assert_eq!(event.r#type, DaemonEventType::InboxChanged as i32);
        assert!(matches!(event.payload, Some(daemon_event::Payload::InboxChanged(_))));
    }
}
