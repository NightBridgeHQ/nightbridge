//! Native LAN protocol implementation.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod auth;
pub mod candidates;
pub mod chunk;
pub mod client;
pub mod discovery;
pub mod dto;
pub mod error;
pub mod framing;
pub mod hole_punch;
pub mod manifest;
pub mod pairing;
pub mod stun;
pub mod tls;
pub mod transfer;
pub mod transport;

pub use error::{NativeError, Result};
