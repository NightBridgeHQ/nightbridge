//! Core error type.

use thiserror::Error;

/// Errors produced anywhere in `lsi-core`.
#[derive(Debug, Error)]
pub enum CoreError {
    /// I/O failure (filesystem, etc.).
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// SQLite failure.
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// Identity vault corruption or wrong format.
    #[error("identity vault: {0}")]
    IdentityVault(String),

    /// Trust store error.
    #[error("trust store: {0}")]
    TrustStore(String),

    /// Cryptographic failure (signature, key parsing, etc.).
    #[error("crypto: {0}")]
    Crypto(String),
}

/// Convenience `Result` alias for `lsi-core`.
pub type Result<T> = std::result::Result<T, CoreError>;
