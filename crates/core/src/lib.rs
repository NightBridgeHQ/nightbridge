//! Protocol-agnostic core for LocalSend Improved.
//!
//! Exposes identity, trust, and (later) protocol primitives via traits so the
//! daemon and other surfaces can inject I/O implementations.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod api_token;
pub mod config;
pub mod error;
pub mod hooks;
pub mod identity;
pub mod paths;
pub mod trust;

pub use error::{CoreError, Result};
