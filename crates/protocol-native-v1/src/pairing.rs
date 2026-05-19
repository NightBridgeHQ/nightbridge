//! Pairing and trust helpers for the native protocol.

use lsi_core::{
    identity::{compute_sas, Fingerprint},
    trust::{PeerPolicy, TrustStore},
};

use crate::{
    dto::{Hello, HelloAck, NativePeerInfo, PROTOCOL_VERSION},
    NativeError, Result,
};

/// Pairing state derived from a remote `Hello` or `HelloAck`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairingState {
    /// Human-readable alias advertised by the peer.
    pub peer_alias: String,
    /// Raw 32-byte Ed25519 public key advertised by the peer.
    pub peer_pubkey: [u8; 32],
    /// Stable fingerprint derived from `peer_pubkey`.
    pub peer_fingerprint: Fingerprint,
    /// Six-digit short authentication string for human comparison.
    pub sas: String,
}

/// Transport-independent decision for a peer observed during pairing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrustDecision {
    /// The peer is trusted and may proceed without user confirmation.
    AutoAccept,
    /// The peer is unknown or configured to require confirmation.
    Prompt,
    /// The peer is explicitly blocked.
    Block,
}

/// Compute the pairing SAS from both public keys and both session nonces.
pub fn pairing_sas(
    local_pubkey: &[u8; 32],
    local_nonce: &[u8],
    remote_pubkey: &[u8; 32],
    remote_nonce: &[u8],
) -> String {
    let nonce = combined_nonce(local_pubkey, local_nonce, remote_pubkey, remote_nonce);
    compute_sas(local_pubkey, remote_pubkey, &nonce)
}

/// Derive pairing state from an inbound `Hello`.
pub fn derive_from_hello(
    local_pubkey: &[u8; 32],
    local_nonce: &[u8],
    hello: &Hello,
) -> Result<PairingState> {
    validate_protocol_version(hello.protocol_version)?;
    derive_state(local_pubkey, local_nonce, &hello.alias, &hello.pubkey, &hello.nonce)
}

/// Derive pairing state from an inbound `HelloAck`.
pub fn derive_from_hello_ack(
    local_pubkey: &[u8; 32],
    local_nonce: &[u8],
    ack: &HelloAck,
) -> Result<PairingState> {
    validate_protocol_version(ack.protocol_version)?;
    derive_state(local_pubkey, local_nonce, &ack.alias, &ack.pubkey, &ack.nonce)
}

/// Validate LAN-advertised fingerprint text against the advertised public key.
pub fn validate_peer_info_fingerprint(peer: &NativePeerInfo) -> Result<Fingerprint> {
    let advertised = peer
        .fingerprint
        .parse::<Fingerprint>()
        .map_err(|err| NativeError::Protocol(err.to_string()))?;
    let derived = Fingerprint::from_pubkey(&peer.pubkey);
    if advertised != derived {
        return Err(NativeError::Protocol(format!(
            "fingerprint mismatch for peer {:?}: advertised {}, derived {}",
            peer.alias, advertised, derived
        )));
    }

    Ok(derived)
}

/// Resolve the current trust decision for a peer fingerprint.
pub fn trust_decision(store: &TrustStore, fingerprint: &Fingerprint) -> Result<TrustDecision> {
    match store.get(fingerprint).map_err(core_error)? {
        Some(peer) => Ok(match peer.policy {
            PeerPolicy::AutoAccept => TrustDecision::AutoAccept,
            PeerPolicy::Prompt => TrustDecision::Prompt,
            PeerPolicy::Block => TrustDecision::Block,
        }),
        None => Ok(TrustDecision::Prompt),
    }
}

fn derive_state(
    local_pubkey: &[u8; 32],
    local_nonce: &[u8],
    peer_alias: &str,
    peer_pubkey: &[u8; 32],
    peer_nonce: &[u8],
) -> Result<PairingState> {
    validate_nonce(local_nonce, "local nonce")?;
    validate_nonce(peer_nonce, "peer nonce")?;

    Ok(PairingState {
        peer_alias: peer_alias.to_string(),
        peer_pubkey: *peer_pubkey,
        peer_fingerprint: Fingerprint::from_pubkey(peer_pubkey),
        sas: pairing_sas(local_pubkey, local_nonce, peer_pubkey, peer_nonce),
    })
}

fn combined_nonce(
    local_pubkey: &[u8; 32],
    local_nonce: &[u8],
    remote_pubkey: &[u8; 32],
    remote_nonce: &[u8],
) -> Vec<u8> {
    let mut nonce = Vec::with_capacity(local_nonce.len() + remote_nonce.len() + 8);
    if local_pubkey <= remote_pubkey {
        push_nonce(&mut nonce, local_nonce);
        push_nonce(&mut nonce, remote_nonce);
    } else {
        push_nonce(&mut nonce, remote_nonce);
        push_nonce(&mut nonce, local_nonce);
    }
    nonce
}

fn push_nonce(output: &mut Vec<u8>, nonce: &[u8]) {
    output.extend_from_slice(&(nonce.len() as u32).to_be_bytes());
    output.extend_from_slice(nonce);
}

fn validate_protocol_version(protocol_version: u16) -> Result<()> {
    if protocol_version != PROTOCOL_VERSION {
        return Err(NativeError::Protocol(format!(
            "unsupported native protocol version {protocol_version}"
        )));
    }
    Ok(())
}

fn validate_nonce(nonce: &[u8], name: &str) -> Result<()> {
    if nonce.is_empty() {
        return Err(NativeError::Protocol(format!("{name} is required for pairing")));
    }
    Ok(())
}

fn core_error(err: lsi_core::CoreError) -> NativeError {
    NativeError::Protocol(err.to_string())
}

#[cfg(test)]
mod tests {
    use lsi_core::{
        identity::Fingerprint,
        trust::{PeerPolicy, TrustStore},
    };

    use crate::{
        dto::{Hello, HelloAck, NativePeerInfo, PROTOCOL_VERSION},
        pairing::{
            derive_from_hello, derive_from_hello_ack, pairing_sas, trust_decision,
            validate_peer_info_fingerprint, TrustDecision,
        },
    };

    fn pubkey(seed: u8) -> [u8; 32] {
        [seed; 32]
    }

    fn hello(seed: u8, nonce: &[u8]) -> Hello {
        Hello {
            protocol_version: PROTOCOL_VERSION,
            alias: format!("peer-{seed}"),
            pubkey: pubkey(seed),
            nonce: nonce.to_vec(),
            extensions: Vec::new(),
        }
    }

    fn hello_ack(seed: u8, nonce: &[u8]) -> HelloAck {
        HelloAck {
            protocol_version: PROTOCOL_VERSION,
            alias: format!("peer-{seed}"),
            pubkey: pubkey(seed),
            nonce: nonce.to_vec(),
            accepted_extensions: Vec::new(),
        }
    }

    #[test]
    fn sas_is_symmetric_across_hello_and_ack_roles() {
        let local_pubkey = pubkey(1);
        let remote_pubkey = pubkey(2);
        let local_nonce = b"local-nonce";
        let remote_nonce = b"remote-nonce";

        let initiator = pairing_sas(&local_pubkey, local_nonce, &remote_pubkey, remote_nonce);
        let responder = pairing_sas(&remote_pubkey, remote_nonce, &local_pubkey, local_nonce);

        assert_eq!(initiator, responder);
    }

    #[test]
    fn derives_pairing_state_from_hello_and_ack() {
        let local_pubkey = pubkey(7);
        let local_nonce = b"local-nonce";

        let from_hello =
            derive_from_hello(&local_pubkey, local_nonce, &hello(8, b"remote")).unwrap();
        let from_ack =
            derive_from_hello_ack(&local_pubkey, local_nonce, &hello_ack(8, b"remote")).unwrap();

        assert_eq!(from_hello.peer_alias, "peer-8");
        assert_eq!(from_hello.peer_pubkey, pubkey(8));
        assert_eq!(from_hello.peer_fingerprint, Fingerprint::from_pubkey(&pubkey(8)));
        assert_eq!(from_hello.sas, from_ack.sas);
    }

    #[test]
    fn validates_advertised_fingerprint_matches_pubkey() {
        let pubkey = pubkey(3);
        let peer = NativePeerInfo {
            alias: "known".to_string(),
            fingerprint: Fingerprint::from_pubkey(&pubkey).to_string(),
            pubkey,
            quic_port: 53318,
            native_certificate_fingerprint: None,
            extensions: Vec::new(),
        };

        let fingerprint = validate_peer_info_fingerprint(&peer).unwrap();

        assert_eq!(fingerprint, Fingerprint::from_pubkey(&pubkey));
    }

    #[test]
    fn rejects_native_peer_info_fingerprint_mismatch() {
        let peer = NativePeerInfo {
            alias: "spoofed".to_string(),
            fingerprint: Fingerprint::from_pubkey(&pubkey(4)).to_string(),
            pubkey: pubkey(5),
            quic_port: 53318,
            native_certificate_fingerprint: None,
            extensions: Vec::new(),
        };

        let result = validate_peer_info_fingerprint(&peer);

        assert!(result.is_err());
    }

    #[test]
    fn maps_known_peer_policy_to_trust_decision() {
        let store = TrustStore::open_in_memory().unwrap();
        let auto = store.trust(pubkey(9), "auto", PeerPolicy::AutoAccept).unwrap();
        let prompt = store.trust(pubkey(10), "prompt", PeerPolicy::Prompt).unwrap();
        let blocked = store.trust(pubkey(11), "blocked", PeerPolicy::Block).unwrap();

        assert_eq!(trust_decision(&store, &auto.fingerprint).unwrap(), TrustDecision::AutoAccept);
        assert_eq!(trust_decision(&store, &prompt.fingerprint).unwrap(), TrustDecision::Prompt);
        assert_eq!(trust_decision(&store, &blocked.fingerprint).unwrap(), TrustDecision::Block);
    }

    #[test]
    fn unknown_peer_prompts_for_user_decision() {
        let store = TrustStore::open_in_memory().unwrap();
        let unknown = Fingerprint::from_pubkey(&pubkey(12));

        let decision = trust_decision(&store, &unknown).unwrap();

        assert_eq!(decision, TrustDecision::Prompt);
    }
}
