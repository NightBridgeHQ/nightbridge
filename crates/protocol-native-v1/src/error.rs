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
    #[error("no direct path: {0}")]
    NoDirectPath(String),
    /// Manifest persistence failure.
    #[error("manifest: {0}")]
    Manifest(String),
}

/// Native protocol result alias.
pub type Result<T> = std::result::Result<T, NativeError>;
