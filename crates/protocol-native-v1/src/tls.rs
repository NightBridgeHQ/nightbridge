//! TLS identity helpers for native QUIC transport.

use std::fmt::Write as _;
use std::sync::Arc;

use quinn::crypto::rustls::QuicServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use sha2::{Digest, Sha256};

use crate::transport::NATIVE_ALPN;
use crate::{NativeError, Result};

/// Self-signed TLS certificate and private key used by the native QUIC listener.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeTlsIdentity {
    /// X.509 certificate DER bytes.
    pub cert_der: Vec<u8>,
    /// PKCS#8 private key DER bytes.
    pub key_der: Vec<u8>,
}

impl NativeTlsIdentity {
    /// Generate a self-signed TLS identity for `alias`.
    pub fn generate(alias: &str) -> Result<Self> {
        let certified = rcgen::generate_simple_self_signed([alias.to_string()])
            .map_err(|error| NativeError::Crypto(error.to_string()))?;

        Ok(Self {
            cert_der: certified.cert.der().to_vec(),
            key_der: certified.key_pair.serialize_der(),
        })
    }

    /// Return the lowercase hex SHA-256 fingerprint of the certificate DER.
    pub fn fingerprint_sha256_hex(&self) -> String {
        certificate_fingerprint_sha256_hex(&self.cert_der)
    }

    /// Return a single-certificate chain suitable for rustls and Quinn.
    pub fn cert_chain(&self) -> Vec<CertificateDer<'static>> {
        vec![CertificateDer::from(self.cert_der.clone())]
    }

    /// Return the PKCS#8 private key in the rustls wrapper type expected by Quinn.
    pub fn private_key_der(&self) -> PrivateKeyDer<'static> {
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.key_der.clone()))
    }

    /// Build a Quinn server config using this identity.
    pub fn quinn_server_config(&self) -> Result<quinn::ServerConfig> {
        ensure_ring_crypto_provider();

        let mut server_crypto = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(self.cert_chain(), self.private_key_der())
            .map_err(|error| NativeError::Crypto(error.to_string()))?;
        server_crypto.alpn_protocols = vec![NATIVE_ALPN.to_vec()];

        let quic_crypto = QuicServerConfig::try_from(server_crypto)
            .map_err(|error| NativeError::Crypto(error.to_string()))?;
        Ok(quinn::ServerConfig::with_crypto(Arc::new(quic_crypto)))
    }
}

/// Return the lowercase hex SHA-256 fingerprint of certificate DER bytes.
pub fn certificate_fingerprint_sha256_hex(cert_der: &[u8]) -> String {
    let digest = Sha256::digest(cert_der);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut out, "{byte:02x}").expect("writing to String cannot fail");
    }
    out
}

/// Ensure rustls has the ring crypto provider installed for builder APIs.
///
/// This is safe to call repeatedly. If another caller already installed a provider,
/// rustls keeps that provider and this function leaves it alone.
pub fn ensure_ring_crypto_provider() {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_identity_has_certificate_and_key_material() {
        let identity = NativeTlsIdentity::generate("native-peer").unwrap();

        assert!(!identity.cert_der.is_empty());
        assert!(!identity.key_der.is_empty());
        assert_eq!(identity.cert_chain().len(), 1);
    }

    #[test]
    fn certificate_fingerprint_is_stable_lowercase_sha256_hex() {
        let identity = NativeTlsIdentity::generate("native-peer").unwrap();

        let first = identity.fingerprint_sha256_hex();
        let second = certificate_fingerprint_sha256_hex(&identity.cert_der);

        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
        assert!(first.chars().all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase()));
    }

    #[test]
    fn ring_provider_install_is_idempotent() {
        ensure_ring_crypto_provider();
        ensure_ring_crypto_provider();

        assert!(rustls::crypto::CryptoProvider::get_default().is_some());
    }

    #[test]
    fn identity_builds_quinn_server_config() {
        let identity = NativeTlsIdentity::generate("native-peer").unwrap();

        let _config = identity.quinn_server_config().unwrap();
    }
}
