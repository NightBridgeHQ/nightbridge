//! Identity proof of possession for the native handshake.
//!
//! A connecting peer authenticates by signing a value bound to the live QUIC/TLS
//! session (the TLS exporter "channel binding") with its Ed25519 identity key. The
//! receiver verifies the signature against the public key the peer claims in its
//! `Hello`. Because the proof is bound to this session's channel binding, a peer
//! cannot be impersonated by merely claiming a public key, and a captured proof
//! cannot be relayed onto a different connection.

use ed25519_dalek::{Signature, VerifyingKey};
use lsi_core::identity::Keypair;

use crate::dto::{Hello, PROTOCOL_VERSION};
use crate::{NativeError, Result};

/// Domain-separation context mixed into every native identity proof.
const IDENTITY_PROOF_CONTEXT: &[u8] = b"nightbridge-native-identity-proof-v1";

/// TLS exporter label used to derive the channel binding for the identity proof.
pub const CHANNEL_BINDING_LABEL: &[u8] = b"nightbridge native identity proof v1";

/// Length in bytes of the channel-binding value exported from the QUIC/TLS session.
pub const CHANNEL_BINDING_LEN: usize = 32;

/// Length in bytes of a random session/pairing nonce.
pub const SESSION_NONCE_LEN: usize = 32;

/// Generate a fresh random session nonce from the OS CSPRNG.
///
/// Used as the `HelloAck` pairing nonce so the short authentication string
/// derived during pairing is unpredictable per session.
pub fn random_session_nonce() -> Vec<u8> {
    use rand::RngCore;
    let mut nonce = vec![0_u8; SESSION_NONCE_LEN];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    nonce
}

/// Build the bytes a peer must sign to prove possession of `peer_pubkey`, bound to
/// this session's `channel_binding`.
pub fn identity_proof_message(peer_pubkey: &[u8; 32], channel_binding: &[u8]) -> Vec<u8> {
    let mut message = Vec::with_capacity(
        IDENTITY_PROOF_CONTEXT.len() + peer_pubkey.len() + channel_binding.len(),
    );
    message.extend_from_slice(IDENTITY_PROOF_CONTEXT);
    message.extend_from_slice(peer_pubkey);
    message.extend_from_slice(channel_binding);
    message
}

/// Sign an identity proof for `channel_binding` using the local identity keypair.
pub fn sign_identity_proof(keypair: &Keypair, channel_binding: &[u8]) -> Vec<u8> {
    let message = identity_proof_message(&keypair.public_bytes(), channel_binding);
    keypair.sign(&message).to_bytes().to_vec()
}

/// Verify a peer's identity proof against the public key it claims and this
/// session's `channel_binding`. Returns an error if the public key or signature
/// bytes are malformed or the signature does not verify.
pub fn verify_identity_proof(
    peer_pubkey: &[u8; 32],
    channel_binding: &[u8],
    signature_bytes: &[u8],
) -> Result<()> {
    let signature = Signature::from_slice(signature_bytes).map_err(|error| {
        NativeError::Crypto(format!("invalid identity proof signature: {error}"))
    })?;
    let verifying_key = VerifyingKey::from_bytes(peer_pubkey)
        .map_err(|error| NativeError::Crypto(format!("invalid peer public key: {error}")))?;
    let message = identity_proof_message(peer_pubkey, channel_binding);
    verifying_key.verify_strict(&message, &signature).map_err(|error| {
        NativeError::Crypto(format!("identity proof verification failed: {error}"))
    })
}

/// Derive the identity-proof channel binding from a live QUIC/TLS connection.
///
/// Both endpoints of a QUIC connection export identical keying material for the
/// same label, so the sender's proof binds to a value the receiver reproduces.
pub fn connection_channel_binding(
    connection: &quinn::Connection,
) -> Result<[u8; CHANNEL_BINDING_LEN]> {
    let mut binding = [0_u8; CHANNEL_BINDING_LEN];
    connection.export_keying_material(&mut binding, CHANNEL_BINDING_LABEL, &[]).map_err(|_| {
        NativeError::Crypto("failed to export channel binding from QUIC session".to_string())
    })?;
    Ok(binding)
}

/// Build a `Hello` that carries an identity proof bound to `channel_binding`.
pub fn build_authenticated_hello(
    keypair: &Keypair,
    alias: String,
    nonce: Vec<u8>,
    extensions: Vec<String>,
    channel_binding: &[u8],
) -> Hello {
    Hello {
        protocol_version: PROTOCOL_VERSION,
        alias,
        pubkey: keypair.public_bytes(),
        nonce,
        extensions,
        identity_proof: sign_identity_proof(keypair, channel_binding),
    }
}

/// Verify that a received `Hello` proves possession of its claimed public key for
/// this session's `channel_binding`. Reject before granting any trust.
pub fn authenticate_hello(hello: &Hello, channel_binding: &[u8]) -> Result<()> {
    verify_identity_proof(&hello.pubkey, channel_binding, &hello.identity_proof)
}

#[cfg(test)]
mod tests {
    use lsi_core::identity::Keypair;

    use super::{sign_identity_proof, verify_identity_proof};

    #[test]
    fn accepts_proof_from_correct_key_and_binding() {
        let keypair = Keypair::generate();
        let binding = [9_u8; 32];

        let proof = sign_identity_proof(&keypair, &binding);

        assert!(verify_identity_proof(&keypair.public_bytes(), &binding, &proof).is_ok());
    }

    #[test]
    fn rejects_proof_signed_by_a_different_key() {
        // The attacker possesses their own key but claims the victim's public key.
        let victim = Keypair::generate();
        let attacker = Keypair::generate();
        let binding = [9_u8; 32];

        let forged = sign_identity_proof(&attacker, &binding);

        assert!(verify_identity_proof(&victim.public_bytes(), &binding, &forged).is_err());
    }

    #[test]
    fn rejects_proof_bound_to_a_different_channel() {
        // A proof captured from one session must not validate on another (relay/replay).
        let keypair = Keypair::generate();

        let proof = sign_identity_proof(&keypair, &[1_u8; 32]);

        assert!(verify_identity_proof(&keypair.public_bytes(), &[2_u8; 32], &proof).is_err());
    }

    #[test]
    fn rejects_malformed_signature_bytes() {
        let keypair = Keypair::generate();

        assert!(verify_identity_proof(&keypair.public_bytes(), &[0_u8; 32], &[0_u8; 10]).is_err());
    }

    #[test]
    fn rejects_malformed_public_key_bytes() {
        let keypair = Keypair::generate();
        let proof = sign_identity_proof(&keypair, &[3_u8; 32]);
        // A public key that is not a valid Ed25519 point must be rejected, not panic.
        let not_a_point = [0xff_u8; 32];

        assert!(verify_identity_proof(&not_a_point, &[3_u8; 32], &proof).is_err());
    }

    use super::{authenticate_hello, build_authenticated_hello, random_session_nonce};
    use crate::dto::default_extensions;

    #[test]
    fn session_nonce_is_nonempty_and_unpredictable() {
        let first = random_session_nonce();
        let second = random_session_nonce();

        assert!(!first.is_empty());
        // Two random 32-byte nonces colliding is cryptographically negligible.
        assert_ne!(first, second);
    }

    #[test]
    fn authenticated_hello_carries_a_proof_that_verifies_under_its_binding() {
        let keypair = Keypair::generate();
        let binding = [5_u8; 32];

        let hello = build_authenticated_hello(
            &keypair,
            "nas".to_string(),
            b"session-nonce".to_vec(),
            default_extensions(),
            &binding,
        );

        assert_eq!(hello.pubkey, keypair.public_bytes());
        assert!(authenticate_hello(&hello, &binding).is_ok());
    }

    #[test]
    fn authenticated_hello_is_rejected_under_a_different_binding() {
        let keypair = Keypair::generate();

        let hello = build_authenticated_hello(
            &keypair,
            "nas".to_string(),
            b"session-nonce".to_vec(),
            default_extensions(),
            &[5_u8; 32],
        );

        // A relay that replays this Hello on its own session has a different binding.
        assert!(authenticate_hello(&hello, &[6_u8; 32]).is_err());
    }
}
