//! Persistent trust store (SQLite).

pub mod store;

pub use store::{Peer, PeerPolicy, TrustStore};
