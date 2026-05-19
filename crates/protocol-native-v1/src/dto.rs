//! Native protocol data transfer objects.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{NativeError, Result};

/// Native protocol version implemented by this crate.
pub const PROTOCOL_VERSION: u16 = 1;

/// Resume extension identifier.
pub const EXT_RESUME_V1: &str = "resume@1";
/// BLAKE3 verification extension identifier.
pub const EXT_BLAKE3_V1: &str = "blake3@1";
/// Zstd compression extension identifier.
pub const EXT_ZSTD_V1: &str = "zstd@1";

/// Native peer information advertised on the LAN.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NativePeerInfo {
    /// Human-readable node alias.
    pub alias: String,
    /// Human-readable fingerprint derived from the Ed25519 public key.
    pub fingerprint: String,
    /// Raw 32-byte Ed25519 public key.
    pub pubkey: [u8; 32],
    /// QUIC UDP port used by the native protocol listener.
    pub quic_port: u16,
    /// Lowercase SHA-256 fingerprint of the peer's native TLS certificate.
    pub native_certificate_fingerprint: Option<String>,
    /// Extensions supported by this peer.
    pub extensions: Vec<String>,
}

/// Initial control-stream hello.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Hello {
    /// Native protocol version.
    pub protocol_version: u16,
    /// Human-readable node alias.
    pub alias: String,
    /// Raw 32-byte Ed25519 public key.
    pub pubkey: [u8; 32],
    /// Pairing/session nonce.
    pub nonce: Vec<u8>,
    /// Extensions supported by this endpoint.
    pub extensions: Vec<String>,
}

/// Control-stream hello response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HelloAck {
    /// Native protocol version.
    pub protocol_version: u16,
    /// Human-readable node alias.
    pub alias: String,
    /// Raw 32-byte Ed25519 public key.
    pub pubkey: [u8; 32],
    /// Pairing/session nonce.
    pub nonce: Vec<u8>,
    /// Negotiated extensions accepted by both endpoints.
    pub accepted_extensions: Vec<String>,
}

/// A file offered for transfer.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FileOffer {
    /// Sender-scoped file id.
    pub file_id: String,
    /// Original file name.
    pub file_name: String,
    /// File size in bytes.
    pub size: u64,
    /// Optional full-file BLAKE3 hex digest.
    pub blake3: Option<String>,
}

/// Request to transfer one or more files.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RequestTransfer {
    /// Transfer id shared by all files in this request.
    pub transfer_id: String,
    /// Offered files.
    pub files: Vec<FileOffer>,
}

/// Receiver acceptance for a transfer request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AcceptTransfer {
    /// Transfer id being accepted.
    pub transfer_id: String,
    /// Chunk size the receiver expects.
    pub chunk_size: u64,
}

/// Acknowledgement that a chunk range was verified.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ChunkAck {
    /// Transfer id.
    pub transfer_id: String,
    /// File id.
    pub file_id: String,
    /// Chunk byte offset.
    pub offset: u64,
    /// Chunk length in bytes.
    pub length: u64,
    /// BLAKE3 hex digest of the chunk bytes.
    pub blake3: String,
}

/// Transfer cancellation message.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CancelTransfer {
    /// Transfer id.
    pub transfer_id: String,
    /// Human-readable reason.
    pub reason: String,
}

/// Transfer completion message.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DoneTransfer {
    /// Transfer id.
    pub transfer_id: String,
}

/// Resume request for an interrupted transfer.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResumeRequest {
    /// Transfer id to resume.
    pub transfer_id: String,
}

/// Supported extensions in local preference order.
pub fn default_extensions() -> Vec<String> {
    vec![EXT_RESUME_V1.to_string(), EXT_BLAKE3_V1.to_string()]
}

/// Intersects extension lists while preserving local preference order.
pub fn negotiate_extensions(local: &[String], remote: &[String]) -> Vec<String> {
    let remote = remote.iter().map(String::as_str).collect::<BTreeSet<_>>();
    local
        .iter()
        .filter(|extension| is_known_extension(extension) && remote.contains(extension.as_str()))
        .cloned()
        .collect()
}

/// Validates a transfer request before allocating receiver state.
pub fn validate_transfer_request(request: &RequestTransfer) -> Result<()> {
    if request.transfer_id.trim().is_empty() {
        return Err(protocol_error("transfer id is required"));
    }

    if request.files.is_empty() {
        return Err(protocol_error("at least one file is required"));
    }

    let mut file_ids = BTreeSet::new();
    for file in &request.files {
        if file.file_id.trim().is_empty() {
            return Err(protocol_error("file id is required"));
        }
        if !file_ids.insert(file.file_id.as_str()) {
            return Err(protocol_error("duplicate file id"));
        }
        if file.file_name.trim().is_empty() {
            return Err(protocol_error("file name is required"));
        }
    }

    Ok(())
}

fn is_known_extension(extension: &str) -> bool {
    matches!(extension, EXT_RESUME_V1 | EXT_BLAKE3_V1 | EXT_ZSTD_V1)
}

fn protocol_error(message: impl Into<String>) -> NativeError {
    NativeError::Protocol(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pubkey(seed: u8) -> [u8; 32] {
        [seed; 32]
    }

    fn file_offer(id: &str) -> FileOffer {
        FileOffer {
            file_id: id.to_string(),
            file_name: format!("{id}.bin"),
            size: 42,
            blake3: None,
        }
    }

    #[test]
    fn hello_roundtrip_preserves_pubkey_and_extensions() {
        let hello = Hello {
            protocol_version: PROTOCOL_VERSION,
            alias: "nas".to_string(),
            pubkey: pubkey(7),
            nonce: b"nonce".to_vec(),
            extensions: default_extensions(),
        };

        let json = serde_json::to_vec(&hello).unwrap();
        let decoded: Hello = serde_json::from_slice(&json).unwrap();

        assert_eq!(decoded, hello);
    }

    #[test]
    fn extension_negotiation_keeps_order_from_local_preferences() {
        let local =
            vec![EXT_BLAKE3_V1.to_string(), EXT_RESUME_V1.to_string(), EXT_ZSTD_V1.to_string()];
        let remote = vec![EXT_RESUME_V1.to_string(), EXT_BLAKE3_V1.to_string()];

        let accepted = negotiate_extensions(&local, &remote);

        assert_eq!(accepted, vec![EXT_BLAKE3_V1.to_string(), EXT_RESUME_V1.to_string()]);
    }

    #[test]
    fn extension_negotiation_drops_unknown_values() {
        let local = vec!["vip-sync@1".to_string(), EXT_RESUME_V1.to_string()];
        let remote = vec!["vip-sync@1".to_string(), EXT_RESUME_V1.to_string()];

        let accepted = negotiate_extensions(&local, &remote);

        assert_eq!(accepted, vec![EXT_RESUME_V1.to_string()]);
    }

    #[test]
    fn transfer_request_rejects_empty_file_list() {
        let request = RequestTransfer { transfer_id: "tx".to_string(), files: Vec::new() };

        assert!(validate_transfer_request(&request).is_err());
    }

    #[test]
    fn transfer_request_rejects_duplicate_file_ids() {
        let request = RequestTransfer {
            transfer_id: "tx".to_string(),
            files: vec![file_offer("file-1"), file_offer("file-1")],
        };

        assert!(validate_transfer_request(&request).is_err());
    }

    #[test]
    fn transfer_request_accepts_valid_files() {
        let request = RequestTransfer {
            transfer_id: "tx".to_string(),
            files: vec![file_offer("file-1"), file_offer("file-2")],
        };

        validate_transfer_request(&request).unwrap();
    }
}
