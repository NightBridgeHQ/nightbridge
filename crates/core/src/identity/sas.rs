//! Short Authentication String (SAS) derivation.
//!
//! Peers compute this value from their public keys and a session nonce during
//! first-pairing authentication. Public keys are canonicalized by
//! lexicographic order so both sides derive the same display code.

use hkdf::Hkdf;
use sha2::Sha256;

/// Length of the SAS in decimal digits.
pub const SAS_DIGITS: usize = 6;

/// Compute the six-digit SAS for a pairing session.
pub fn compute_sas(pubkey_a: &[u8; 32], pubkey_b: &[u8; 32], nonce: &[u8]) -> String {
    let (lo, hi) = if pubkey_a <= pubkey_b { (pubkey_a, pubkey_b) } else { (pubkey_b, pubkey_a) };

    let mut input_key_material = Vec::with_capacity(64 + nonce.len());
    input_key_material.extend_from_slice(lo);
    input_key_material.extend_from_slice(hi);
    input_key_material.extend_from_slice(nonce);

    let hkdf = Hkdf::<Sha256>::new(None, &input_key_material);
    let mut output_key_material = [0u8; 4];
    hkdf.expand(b"lsi-sas-v1", &mut output_key_material)
        .expect("HKDF-SHA256 can expand to 4 bytes");

    let value = u32::from_be_bytes(output_key_material);
    let modulus = 10u32.pow(SAS_DIGITS as u32);
    format!("{:0width$}", value % modulus, width = SAS_DIGITS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_inputs_produce_same_sas() {
        let a = [1u8; 32];
        let b = [2u8; 32];
        let nonce = b"session-1";

        assert_eq!(compute_sas(&a, &b, nonce), compute_sas(&a, &b, nonce));
    }

    #[test]
    fn order_of_pubkeys_does_not_matter() {
        let a = [1u8; 32];
        let b = [2u8; 32];
        let nonce = b"session-1";

        assert_eq!(compute_sas(&a, &b, nonce), compute_sas(&b, &a, nonce));
    }

    #[test]
    fn different_nonces_produce_different_sas() {
        let a = [1u8; 32];
        let b = [2u8; 32];

        assert_ne!(compute_sas(&a, &b, b"nonce-1"), compute_sas(&a, &b, b"nonce-2"));
    }

    #[test]
    fn sas_has_expected_length() {
        let sas = compute_sas(&[0u8; 32], &[1u8; 32], b"x");

        assert_eq!(sas.len(), SAS_DIGITS);
        assert!(sas.chars().all(|char| char.is_ascii_digit()));
    }

    #[test]
    fn sas_zero_pads() {
        let a = [0u8; 32];
        let b = [0u8; 32];

        for nonce in 0u32..100_000 {
            let sas = compute_sas(&a, &b, &nonce.to_be_bytes());
            assert_eq!(sas.len(), SAS_DIGITS, "padding broken for nonce {nonce}");
        }
    }
}
