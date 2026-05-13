//! Human-readable fingerprint derived from a public key.
//!
//! The fingerprint is `SHA-256(pubkey)` truncated to 64 bits and rendered as
//! four lowercase hex groups, for example `a1b2-c3d4-e5f6-7890`.

use std::fmt;
use std::str::FromStr;

use sha2::{Digest, Sha256};

use crate::error::{CoreError, Result};

/// Stable 64-bit fingerprint of an Ed25519 public key.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fingerprint([u8; 8]);

impl Fingerprint {
    /// Compute the fingerprint of a 32-byte Ed25519 public key.
    pub fn from_pubkey(pubkey: &[u8; 32]) -> Self {
        let digest = Sha256::digest(pubkey);
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&digest[..8]);
        Self(bytes)
    }

    /// Return the raw 8 fingerprint bytes.
    pub fn as_bytes(&self) -> &[u8; 8] {
        &self.0
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, byte) in self.0.iter().enumerate() {
            if index > 0 && index % 2 == 0 {
                f.write_str("-")?;
            }
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fingerprint({self})")
    }
}

impl FromStr for Fingerprint {
    type Err = CoreError;

    fn from_str(value: &str) -> Result<Self> {
        let cleaned: String = value.chars().filter(|char| *char != '-').collect();
        if cleaned.len() != 16 {
            return Err(CoreError::Crypto(format!(
                "fingerprint must be 16 hex chars, got {}",
                cleaned.len()
            )));
        }

        let mut bytes = [0u8; 8];
        for (index, byte) in bytes.iter_mut().enumerate() {
            let start = index * 2;
            let end = start + 2;
            *byte = u8::from_str_radix(&cleaned[start..end], 16)
                .map_err(|err| CoreError::Crypto(err.to_string()))?;
        }

        Ok(Self(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_has_expected_shape() {
        let fingerprint = Fingerprint::from_pubkey(&[0u8; 32]);
        let formatted = fingerprint.to_string();

        assert_eq!(formatted.len(), 19);
        assert_eq!(formatted.chars().filter(|char| *char == '-').count(), 3);
        assert!(formatted
            .chars()
            .filter(|char| *char != '-')
            .all(|char| char.is_ascii_hexdigit() && !char.is_ascii_uppercase()));
    }

    #[test]
    fn deterministic_from_same_pubkey() {
        let pubkey = [42u8; 32];

        assert_eq!(Fingerprint::from_pubkey(&pubkey), Fingerprint::from_pubkey(&pubkey));
    }

    #[test]
    fn distinct_pubkeys_distinct_fingerprints() {
        let a = Fingerprint::from_pubkey(&[1u8; 32]);
        let b = Fingerprint::from_pubkey(&[2u8; 32]);

        assert_ne!(a, b);
    }

    #[test]
    fn display_parse_roundtrip() {
        let original = Fingerprint::from_pubkey(&[99u8; 32]);

        let parsed: Fingerprint = original.to_string().parse().unwrap();

        assert_eq!(original, parsed);
    }

    #[test]
    fn parse_accepts_non_hyphenated_hex() {
        let original = Fingerprint::from_pubkey(&[99u8; 32]);
        let compact = original.to_string().replace('-', "");

        let parsed: Fingerprint = compact.parse().unwrap();

        assert_eq!(original, parsed);
    }

    #[test]
    fn parse_rejects_wrong_length() {
        let result: Result<Fingerprint> = "abcd-1234".parse();

        assert!(result.is_err());
    }

    #[test]
    fn parse_rejects_non_hex() {
        let result: Result<Fingerprint> = "zzzz-1234-5678-9abc".parse();

        assert!(result.is_err());
    }
}
