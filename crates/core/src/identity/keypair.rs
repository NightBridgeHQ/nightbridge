//! Ed25519 keypair: generation, serialization, signing.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;

use crate::error::{CoreError, Result};

/// An Ed25519 identity keypair owned by this node.
#[derive(Clone)]
pub struct Keypair {
    signing: SigningKey,
}

impl Keypair {
    /// Generate a fresh keypair from OS randomness.
    pub fn generate() -> Self {
        let signing = SigningKey::generate(&mut OsRng);
        Self { signing }
    }

    /// Reconstruct a keypair from its 32-byte secret key.
    pub fn from_secret_bytes(bytes: &[u8; 32]) -> Self {
        Self { signing: SigningKey::from_bytes(bytes) }
    }

    /// Export the 32-byte secret key. Treat this value as sensitive.
    pub fn secret_bytes(&self) -> [u8; 32] {
        self.signing.to_bytes()
    }

    /// Return the public verifying key.
    pub fn public(&self) -> VerifyingKey {
        self.signing.verifying_key()
    }

    /// Return the raw 32-byte public key.
    pub fn public_bytes(&self) -> [u8; 32] {
        self.public().to_bytes()
    }

    /// Sign a message.
    pub fn sign(&self, message: &[u8]) -> Signature {
        self.signing.sign(message)
    }

    /// Verify a signature against this keypair's public key.
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<()> {
        self.public().verify(message, signature).map_err(|err| CoreError::Crypto(err.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_secret_bytes() {
        let keypair = Keypair::generate();
        let secret = keypair.secret_bytes();

        let reconstructed = Keypair::from_secret_bytes(&secret);

        assert_eq!(keypair.public_bytes(), reconstructed.public_bytes());
    }

    #[test]
    fn sign_and_verify_roundtrip() {
        let keypair = Keypair::generate();
        let message = b"hello, lan";

        let signature = keypair.sign(message);

        assert!(keypair.verify(message, &signature).is_ok());
    }

    #[test]
    fn verify_rejects_tampered_message() {
        let keypair = Keypair::generate();
        let signature = keypair.sign(b"original");

        assert!(keypair.verify(b"tampered", &signature).is_err());
    }

    #[test]
    fn distinct_generations_produce_distinct_keys() {
        let a = Keypair::generate();
        let b = Keypair::generate();

        assert_ne!(a.public_bytes(), b.public_bytes());
    }
}
