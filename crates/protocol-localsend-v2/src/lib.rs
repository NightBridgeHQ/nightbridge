//! LocalSend v2 protocol compatibility.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod client;
pub mod discovery;
pub mod dto;
pub mod error;
pub mod inbox;
pub mod server;
pub mod session;
pub mod tls;

pub use error::{LocalSendError, Result};
