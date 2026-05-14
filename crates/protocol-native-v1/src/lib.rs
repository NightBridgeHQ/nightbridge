//! Native LAN protocol implementation.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod dto;
pub mod error;
pub mod framing;
pub mod manifest;

pub use error::{NativeError, Result};
