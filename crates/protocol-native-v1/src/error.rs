//! Native protocol error type.

use thiserror::Error;

/// Errors produced by the native protocol implementation.
#[derive(Debug, Error)]
pub enum NativeError {
    /// I/O failure.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// JSON serialization failure.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    /// Cryptographic failure.
    #[error("crypto: {0}")]
    Crypto(String),
    /// Wire protocol failure.
    #[error("protocol: {0}")]
    Protocol(String),
    /// Transport failure.
    #[error("transport: {0}")]
    Transport(String),
    /// No direct WAN route could be established.
    #[error("no direct WAN path to peer; {hint} attempted_pairs={attempted_pairs:?}")]
    NoDirectPath {
        /// Candidate pairs attempted before giving up.
        attempted_pairs: Vec<String>,
        /// Actionable diagnostic hint for the user.
        hint: String,
        /// Last transport error observed while dialing.
        last_error: Option<String>,
    },
    /// Manifest persistence failure.
    #[error("manifest: {0}")]
    Manifest(String),
}

/// Native protocol result alias.
pub type Result<T> = std::result::Result<T, NativeError>;
