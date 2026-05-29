//! Pinned TLS verification for LocalSend v2 self-signed certificates.
//!
//! LocalSend peers use self-signed certs and advertise the certificate's
//! SHA-256 fingerprint during discovery. Instead of blindly accepting any
//! certificate, the outbound client pins the presented certificate to the
//! fingerprint discovered for that peer, which restores MITM protection on the
//! LAN (H-3).

use std::sync::Arc;

use reqwest::Client;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, WebPkiSupportedAlgorithms};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use sha2::{Digest, Sha256};

use crate::{LocalSendError, Result};

/// Normalize a fingerprint for comparison: lowercase, without `:`/`-` separators.
pub fn normalize_fingerprint(fingerprint: &str) -> String {
    fingerprint.chars().filter(|ch| *ch != ':' && *ch != '-').flat_map(char::to_lowercase).collect()
}

/// Lowercase hex SHA-256 of certificate DER bytes.
pub fn certificate_fingerprint_sha256_hex(cert_der: &[u8]) -> String {
    use std::fmt::Write as _;
    let digest = Sha256::digest(cert_der);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut out, "{byte:02x}").expect("writing to String cannot fail");
    }
    out
}

/// Rustls verifier that accepts a self-signed cert only when its SHA-256
/// fingerprint matches the expected (discovered) peer fingerprint.
#[derive(Debug)]
struct PinnedCertVerifier {
    expected_fingerprint: String,
    supported_algs: WebPkiSupportedAlgorithms,
}

impl PinnedCertVerifier {
    fn new(expected_fingerprint: &str) -> Self {
        Self {
            expected_fingerprint: normalize_fingerprint(expected_fingerprint),
            supported_algs: rustls::crypto::ring::default_provider()
                .signature_verification_algorithms,
        }
    }

    fn check_fingerprint(&self, cert_der: &[u8]) -> std::result::Result<(), rustls::Error> {
        let actual = certificate_fingerprint_sha256_hex(cert_der);
        if actual == self.expected_fingerprint {
            Ok(())
        } else {
            Err(rustls::Error::General(format!(
                "LocalSend peer certificate fingerprint mismatch: expected {}, got {}",
                self.expected_fingerprint, actual
            )))
        }
    }
}

impl ServerCertVerifier for PinnedCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        self.check_fingerprint(end_entity.as_ref())?;
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(message, cert, dss, &self.supported_algs)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.supported_algs)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.supported_algs.supported_schemes()
    }
}

/// Build a reqwest client that pins the peer's self-signed certificate to
/// `expected_fingerprint`. Plaintext HTTP requests are unaffected.
pub fn pinned_client(expected_fingerprint: &str) -> Result<Client> {
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|error| LocalSendError::Crypto(error.to_string()))?
    .dangerous()
    .with_custom_certificate_verifier(Arc::new(PinnedCertVerifier::new(expected_fingerprint)))
    .with_no_client_auth();

    Client::builder()
        .use_preconfigured_tls(config)
        .build()
        .map_err(|error| LocalSendError::Http(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tls::TlsIdentity;

    #[test]
    fn normalizes_separators_and_case() {
        assert_eq!(normalize_fingerprint("AA:BB-cd"), "aabbcd");
    }

    #[test]
    fn fingerprint_of_generated_cert_matches_identity() {
        let identity = TlsIdentity::generate("peer").unwrap();
        assert_eq!(
            certificate_fingerprint_sha256_hex(&identity.cert_der),
            identity.fingerprint_sha256_hex()
        );
    }

    #[test]
    fn verifier_accepts_matching_fingerprint() {
        let identity = TlsIdentity::generate("peer").unwrap();
        let verifier = PinnedCertVerifier::new(&identity.fingerprint_sha256_hex());

        assert!(verifier.check_fingerprint(&identity.cert_der).is_ok());
    }

    #[test]
    fn verifier_rejects_mismatched_fingerprint() {
        let expected = TlsIdentity::generate("expected").unwrap();
        let attacker = TlsIdentity::generate("attacker").unwrap();
        let verifier = PinnedCertVerifier::new(&expected.fingerprint_sha256_hex());

        assert!(verifier.check_fingerprint(&attacker.cert_der).is_err());
    }

    #[test]
    fn verifier_matches_fingerprint_with_separators() {
        let identity = TlsIdentity::generate("peer").unwrap();
        let hex = identity.fingerprint_sha256_hex();
        let colonized = hex
            .as_bytes()
            .chunks(2)
            .map(|c| std::str::from_utf8(c).unwrap())
            .collect::<Vec<_>>()
            .join(":");
        let verifier = PinnedCertVerifier::new(&colonized);

        assert!(verifier.check_fingerprint(&identity.cert_der).is_ok());
    }
}
