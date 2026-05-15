use std::collections::HashMap;
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::protocol::{
    LookupPeer, LookupRequest, LookupResponse, NotifyQueuedResponse, NotifyRequest,
    PeerNotification, RegisterRequest, RegisteredResponse,
};

/// Result type for rendezvous registry operations.
pub type Result<T> = std::result::Result<T, RegistryError>;

/// Registry-level errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RegistryError {
    /// A protocol DTO failed validation.
    #[error(transparent)]
    Protocol(#[from] crate::protocol::ProtocolError),
    /// The requested peer is not currently registered.
    #[error("target peer is not registered")]
    TargetNotRegistered,
}

/// In-memory rendezvous peer registry.
#[derive(Debug)]
pub struct Registry {
    max_ttl: Duration,
    entries: HashMap<[u8; 32], RegistryEntry>,
    notifications: HashMap<[u8; 32], Vec<PeerNotification>>,
}

impl Registry {
    /// Creates an empty registry with a maximum entry TTL.
    pub fn new(max_ttl: Duration) -> Self {
        Self { max_ttl, entries: HashMap::new(), notifications: HashMap::new() }
    }

    /// Registers or refreshes one peer entry.
    pub fn register(
        &mut self,
        now: Instant,
        request: RegisterRequest,
    ) -> Result<RegisteredResponse> {
        request.validate()?;
        let ttl = self.capped_ttl(request.ttl_seconds);
        let ttl_seconds = ttl.as_secs();
        let expires_at = now + ttl;
        let entry = RegistryEntry {
            alias: request.alias,
            pubkey: request.pubkey,
            candidates: request.candidates,
            registered_at: now,
            expires_at,
            last_seen: now,
        };

        self.entries.insert(entry.pubkey, entry);

        Ok(RegisteredResponse { ttl_seconds, expires_at_unix_seconds: ttl_seconds })
    }

    /// Looks up one peer by public key, omitting expired entries.
    pub fn lookup(&mut self, now: Instant, pubkey: &[u8; 32]) -> Option<LookupPeer> {
        self.prune_expired(now);
        let entry = self.entries.get_mut(pubkey)?;
        entry.last_seen = now;
        Some(entry.to_lookup_peer())
    }

    /// Handles a lookup request and wraps the result in the wire response type.
    pub fn lookup_request(
        &mut self,
        now: Instant,
        request: LookupRequest,
    ) -> Result<LookupResponse> {
        Ok(LookupResponse { peer: self.lookup(now, &request.target_pubkey) })
    }

    /// Queues a notification for a currently registered target peer.
    pub fn queue_notify(
        &mut self,
        now: Instant,
        request: NotifyRequest,
    ) -> Result<NotifyQueuedResponse> {
        if self.lookup(now, &request.target_pubkey).is_none() {
            return Err(RegistryError::TargetNotRegistered);
        }

        self.notifications
            .entry(request.target_pubkey)
            .or_default()
            .push(PeerNotification { requester_pubkey: request.requester_pubkey });

        Ok(NotifyQueuedResponse { target_pubkey: request.target_pubkey })
    }

    /// Drains queued notifications for a target peer.
    pub fn drain_notifications(
        &mut self,
        now: Instant,
        target_pubkey: &[u8; 32],
    ) -> Vec<PeerNotification> {
        if self.lookup(now, target_pubkey).is_none() {
            return Vec::new();
        }

        self.notifications.remove(target_pubkey).unwrap_or_default()
    }

    /// Removes expired entries and returns the number of entries pruned.
    pub fn prune_expired(&mut self, now: Instant) -> usize {
        let before = self.entries.len();
        self.entries.retain(|pubkey, entry| {
            let keep = entry.expires_at > now;
            if !keep {
                self.notifications.remove(pubkey);
            }
            keep
        });
        before - self.entries.len()
    }

    fn capped_ttl(&self, requested_seconds: u64) -> Duration {
        let requested = Duration::from_secs(requested_seconds);
        requested.min(self.max_ttl)
    }
}

#[derive(Debug)]
struct RegistryEntry {
    alias: String,
    pubkey: [u8; 32],
    candidates: Vec<crate::protocol::Candidate>,
    registered_at: Instant,
    expires_at: Instant,
    last_seen: Instant,
}

impl RegistryEntry {
    fn to_lookup_peer(&self) -> LookupPeer {
        LookupPeer {
            alias: self.alias.clone(),
            pubkey: self.pubkey,
            candidates: self.candidates.clone(),
            last_seen_unix_seconds: self.last_seen.duration_since(self.registered_at).as_secs(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::protocol::{
        Candidate, CandidateKind, LookupRequest, NotifyRequest, RegisterRequest,
    };

    #[test]
    fn register_then_lookup_returns_candidates() {
        let mut registry = Registry::new(Duration::from_secs(120));
        let now = Instant::now();

        registry.register(now, register_request("alice", [1; 32], 60)).unwrap();
        let peer = registry.lookup(now, &[1; 32]).unwrap();

        assert_eq!(peer.alias, "alice");
        assert_eq!(peer.pubkey, [1; 32]);
        assert_eq!(peer.candidates[0].address, "127.0.0.1:53400");
    }

    #[test]
    fn later_registration_replaces_candidates() {
        let mut registry = Registry::new(Duration::from_secs(120));
        let now = Instant::now();
        registry.register(now, register_request("alice", [1; 32], 60)).unwrap();

        let replacement = RegisterRequest {
            candidates: vec![candidate("127.0.0.1:53401", 200)],
            ..register_request("alice-new", [1; 32], 60)
        };
        registry.register(now + Duration::from_secs(1), replacement).unwrap();
        let peer = registry.lookup(now + Duration::from_secs(1), &[1; 32]).unwrap();

        assert_eq!(peer.alias, "alice-new");
        assert_eq!(peer.candidates[0].address, "127.0.0.1:53401");
    }

    #[test]
    fn lookup_omits_expired_peer() {
        let mut registry = Registry::new(Duration::from_secs(120));
        let now = Instant::now();
        registry.register(now, register_request("alice", [1; 32], 1)).unwrap();

        let result = registry.lookup(now + Duration::from_secs(2), &[1; 32]);

        assert!(result.is_none());
    }

    #[test]
    fn notify_for_missing_target_reports_not_found() {
        let mut registry = Registry::new(Duration::from_secs(120));
        let now = Instant::now();

        let error = registry
            .queue_notify(now, NotifyRequest { requester_pubkey: [1; 32], target_pubkey: [2; 32] })
            .unwrap_err();

        assert!(error.to_string().contains("target peer is not registered"));
    }

    #[test]
    fn registry_caps_requested_ttl() {
        let mut registry = Registry::new(Duration::from_secs(30));
        let now = Instant::now();

        let response = registry.register(now, register_request("alice", [1; 32], 300)).unwrap();

        assert_eq!(response.ttl_seconds, 30);
        assert!(registry.lookup(now + Duration::from_secs(31), &[1; 32]).is_none());
    }

    #[test]
    fn drain_notifications_returns_queued_requests() {
        let mut registry = Registry::new(Duration::from_secs(120));
        let now = Instant::now();
        registry.register(now, register_request("alice", [1; 32], 60)).unwrap();
        registry.register(now, register_request("bob", [2; 32], 60)).unwrap();
        registry
            .queue_notify(now, NotifyRequest { requester_pubkey: [1; 32], target_pubkey: [2; 32] })
            .unwrap();

        let notifications = registry.drain_notifications(now, &[2; 32]);

        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].requester_pubkey, [1; 32]);
        assert!(registry.drain_notifications(now, &[2; 32]).is_empty());
    }

    #[test]
    fn lookup_request_uses_target_pubkey() {
        let mut registry = Registry::new(Duration::from_secs(120));
        let now = Instant::now();
        registry.register(now, register_request("alice", [1; 32], 60)).unwrap();

        let peer = registry
            .lookup_request(
                now,
                LookupRequest { requester_pubkey: [9; 32], target_pubkey: [1; 32] },
            )
            .unwrap()
            .peer
            .unwrap();

        assert_eq!(peer.pubkey, [1; 32]);
    }

    fn register_request(alias: &str, pubkey: [u8; 32], ttl_seconds: u64) -> RegisterRequest {
        RegisterRequest {
            alias: alias.to_string(),
            pubkey,
            candidates: vec![candidate("127.0.0.1:53400", 100)],
            ttl_seconds,
        }
    }

    fn candidate(address: &str, priority: u32) -> Candidate {
        Candidate { kind: CandidateKind::Local, address: address.to_string(), priority }
    }
}
