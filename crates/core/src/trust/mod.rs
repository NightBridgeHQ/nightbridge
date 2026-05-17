//! Persistent trust store (SQLite).

pub mod store;

pub use store::{LocalSendPeer, LocalSendPeerStatus, Peer, PeerPolicy, TrustStore};
