use std::net::SocketAddr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Result type for rendezvous protocol validation.
pub type Result<T> = std::result::Result<T, ProtocolError>;

/// Errors returned by rendezvous protocol DTO validation.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtocolError {
    /// A required string field was empty or whitespace-only.
    #[error("{0} is required")]
    Required(&'static str),
    /// A candidate list was empty.
    #[error("register request must include at least one candidate")]
    EmptyCandidates,
    /// A TTL was zero.
    #[error("ttl_seconds must be greater than zero")]
    InvalidTtl,
    /// A candidate address was malformed.
    #[error("candidate address must be host:port")]
    InvalidCandidateAddress,
    /// A candidate port was zero.
    #[error("candidate port must be greater than zero")]
    InvalidCandidatePort,
}

/// Messages sent from daemon clients to a rendezvous server.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Register or refresh one peer's current candidates.
    Register(RegisterRequest),
    /// Lookup one target peer.
    Lookup(LookupRequest),
    /// Ask rendezvous to notify a target that a requester is trying to connect.
    Notify(NotifyRequest),
    /// Liveness probe.
    Ping,
}

/// Messages sent from a rendezvous server to daemon clients.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Registration succeeded.
    Registered(RegisteredResponse),
    /// Lookup completed.
    LookupResult(LookupResponse),
    /// Notification was queued for a registered peer.
    NotifyQueued(NotifyQueuedResponse),
    /// Peer notification delivered to a registered peer.
    PeerNotification(PeerNotification),
    /// Liveness response.
    Pong,
    /// Request failed.
    Error(ErrorResponse),
}

/// Candidate endpoint type.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateKind {
    /// Candidate reachable on the peer's local network.
    Local,
    /// Public endpoint discovered through STUN.
    ServerReflexive,
}

/// A possible UDP endpoint for native QUIC connectivity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    /// Candidate source.
    pub kind: CandidateKind,
    /// Socket address as `ip:port`.
    pub address: String,
    /// Higher values should be attempted first.
    pub priority: u32,
}

impl Candidate {
    /// Validates the candidate endpoint syntax.
    pub fn validate(&self) -> Result<()> {
        let address = self
            .address
            .parse::<SocketAddr>()
            .map_err(|_| ProtocolError::InvalidCandidateAddress)?;
        if address.port() == 0 {
            return Err(ProtocolError::InvalidCandidatePort);
        }
        Ok(())
    }
}

/// Registration request for a daemon peer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterRequest {
    /// Human-readable peer alias.
    pub alias: String,
    /// Peer Ed25519 public key bytes.
    pub pubkey: [u8; 32],
    /// Current native QUIC connection candidates.
    pub candidates: Vec<Candidate>,
    /// Registration TTL requested by the client.
    pub ttl_seconds: u64,
}

impl RegisterRequest {
    /// Validates semantic constraints before registry insertion.
    pub fn validate(&self) -> Result<()> {
        require_nonblank("alias", &self.alias)?;
        if self.candidates.is_empty() {
            return Err(ProtocolError::EmptyCandidates);
        }
        if self.ttl_seconds == 0 {
            return Err(ProtocolError::InvalidTtl);
        }
        for candidate in &self.candidates {
            candidate.validate()?;
        }
        Ok(())
    }
}

/// Lookup request for one target peer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookupRequest {
    /// Public key of the peer performing the lookup.
    pub requester_pubkey: [u8; 32],
    /// Public key of the peer being looked up.
    pub target_pubkey: [u8; 32],
}

/// Notification request for one target peer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotifyRequest {
    /// Public key of the peer requesting a connection.
    pub requester_pubkey: [u8; 32],
    /// Public key of the peer that should be notified.
    pub target_pubkey: [u8; 32],
}

/// Successful registration response.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredResponse {
    /// Server-selected TTL after applying caps.
    pub ttl_seconds: u64,
    /// Unix timestamp when this entry expires.
    pub expires_at_unix_seconds: u64,
}

/// Lookup response containing zero or one peer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookupResponse {
    /// Registered peer, if found and not expired.
    pub peer: Option<LookupPeer>,
}

/// Peer details returned by lookup.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookupPeer {
    /// Human-readable peer alias.
    pub alias: String,
    /// Peer Ed25519 public key bytes.
    pub pubkey: [u8; 32],
    /// Current native QUIC candidates.
    pub candidates: Vec<Candidate>,
    /// Last server-side observation timestamp.
    pub last_seen_unix_seconds: u64,
}

/// Successful notification enqueue response.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotifyQueuedResponse {
    /// Public key of the peer that will receive this notification.
    pub target_pubkey: [u8; 32],
}

/// Notification delivered to a registered peer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerNotification {
    /// Public key of the peer asking for a direct connection.
    pub requester_pubkey: [u8; 32],
}

/// Structured protocol error response.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Stable machine-readable code.
    pub code: String,
    /// Human-readable message.
    pub message: String,
}

fn require_nonblank(name: &'static str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(ProtocolError::Required(name));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_request_roundtrips_with_candidates() {
        let request = RegisterRequest {
            alias: "nas".to_string(),
            pubkey: [7; 32],
            candidates: vec![Candidate {
                kind: CandidateKind::Local,
                address: "192.168.1.20:53400".to_string(),
                priority: 100,
            }],
            ttl_seconds: 60,
        };

        let json = serde_json::to_string(&ClientMessage::Register(request.clone())).unwrap();
        let parsed: ClientMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed, ClientMessage::Register(request));
    }

    #[test]
    fn register_rejects_empty_candidates() {
        let request = RegisterRequest {
            alias: "nas".to_string(),
            pubkey: [7; 32],
            candidates: Vec::new(),
            ttl_seconds: 60,
        };

        let error = request.validate().unwrap_err();

        assert!(error.to_string().contains("at least one candidate"));
    }

    #[test]
    fn lookup_request_roundtrips() {
        let request = LookupRequest { requester_pubkey: [1; 32], target_pubkey: [2; 32] };

        let json = serde_json::to_string(&ClientMessage::Lookup(request.clone())).unwrap();
        let parsed: ClientMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed, ClientMessage::Lookup(request));
    }

    #[test]
    fn notify_request_roundtrips() {
        let request = NotifyRequest { requester_pubkey: [1; 32], target_pubkey: [2; 32] };

        let json = serde_json::to_string(&ClientMessage::Notify(request.clone())).unwrap();
        let parsed: ClientMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed, ClientMessage::Notify(request));
    }

    #[test]
    fn candidate_validation_rejects_invalid_socket_address() {
        let candidate = Candidate {
            kind: CandidateKind::Local,
            address: "not-a-socket".to_string(),
            priority: 1,
        };

        let error = candidate.validate().unwrap_err();

        assert!(error.to_string().contains("candidate address must be host:port"));
    }
}
