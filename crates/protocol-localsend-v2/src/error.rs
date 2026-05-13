//! Error types for LocalSend v2 protocol operations.

/// Error type used by the LocalSend v2 compatibility crate.
#[derive(Debug, thiserror::Error)]
pub enum LocalSendError {
    /// I/O failure.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// JSON serialization or deserialization failure.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    /// HTTP protocol failure.
    #[error("http: {0}")]
    Http(String),
    /// Upload session failure.
    #[error("session: {0}")]
    Session(String),
    /// Cryptographic material or verification failure.
    #[error("crypto: {0}")]
    Crypto(String),
}

/// Convenient result alias for LocalSend v2 protocol operations.
pub type Result<T> = std::result::Result<T, LocalSendError>;
