//! TLS certificate identity storage for LocalSend v2 HTTPS mode.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::{LocalSendError, Result};
use sha2::{Digest, Sha256};

const CERT_FILE: &str = "localsend-v2.cert.der";
const KEY_FILE: &str = "localsend-v2.key.der";

/// Self-signed TLS identity advertised by LocalSend v2 HTTPS discovery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TlsIdentity {
    /// X.509 certificate DER bytes.
    pub cert_der: Vec<u8>,
    /// Private key DER bytes.
    pub key_der: Vec<u8>,
}

impl TlsIdentity {
    /// Generate a self-signed TLS identity for `alias`.
    pub fn generate(alias: &str) -> Result<Self> {
        let certified = rcgen::generate_simple_self_signed([alias.to_string()])
            .map_err(|error| LocalSendError::Crypto(error.to_string()))?;

        Ok(Self {
            cert_der: certified.cert.der().to_vec(),
            key_der: certified.key_pair.serialize_der(),
        })
    }

    /// Return the lowercase hex SHA-256 fingerprint of the certificate DER.
    pub fn fingerprint_sha256_hex(&self) -> String {
        let digest = Sha256::digest(&self.cert_der);
        let mut out = String::with_capacity(digest.len() * 2);
        for byte in digest {
            write!(&mut out, "{byte:02x}").expect("writing to String cannot fail");
        }
        out
    }
}

/// Filesystem-backed TLS identity vault.
#[derive(Clone, Debug)]
pub struct TlsIdentityVault {
    config_dir: PathBuf,
}

impl TlsIdentityVault {
    /// Create a vault rooted at `config_dir`.
    pub fn new(config_dir: impl Into<PathBuf>) -> Self {
        Self { config_dir: config_dir.into() }
    }

    /// Load the TLS identity, returning `None` when both identity files are missing.
    pub fn load(&self) -> Result<Option<TlsIdentity>> {
        let cert_path = self.cert_path();
        let key_path = self.key_path();

        match (cert_path.exists(), key_path.exists()) {
            (false, false) => Ok(None),
            (true, true) => Ok(Some(TlsIdentity {
                cert_der: std::fs::read(cert_path)?,
                key_der: std::fs::read(key_path)?,
            })),
            _ => Err(LocalSendError::Crypto(
                "incomplete LocalSend v2 TLS identity on disk".to_string(),
            )),
        }
    }

    /// Save the TLS identity, overwriting any previous certificate/key pair.
    pub fn save(&self, identity: &TlsIdentity) -> Result<()> {
        std::fs::create_dir_all(&self.config_dir)?;

        write_atomically(&self.cert_path(), &identity.cert_der, false)?;
        write_atomically(&self.key_path(), &identity.key_der, true)?;
        Ok(())
    }

    /// Load the stored identity or generate and persist a new one.
    pub fn load_or_generate(&self, alias: &str) -> Result<TlsIdentity> {
        if let Some(identity) = self.load()? {
            return Ok(identity);
        }

        let identity = TlsIdentity::generate(alias)?;
        self.save(&identity)?;
        Ok(identity)
    }

    fn cert_path(&self) -> PathBuf {
        self.config_dir.join(CERT_FILE)
    }

    fn key_path(&self) -> PathBuf {
        self.config_dir.join(KEY_FILE)
    }
}

fn write_atomically(path: &Path, bytes: &[u8], owner_only: bool) -> Result<()> {
    let tmp_path = path.with_extension("tmp");
    std::fs::write(&tmp_path, bytes)?;
    if owner_only {
        set_owner_only_permissions(&tmp_path)?;
    }
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

#[cfg(unix)]
fn set_owner_only_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(0o600);
    std::fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_owner_only_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_cert_has_stable_sha256_fingerprint() {
        let identity = TlsIdentity::generate("NAS").unwrap();

        let first = identity.fingerprint_sha256_hex();
        let second = identity.fingerprint_sha256_hex();

        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
        assert!(first.chars().all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase()));
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = tempfile::TempDir::new().unwrap();
        let vault = TlsIdentityVault::new(dir.path());
        let identity = TlsIdentity::generate("Phone").unwrap();

        vault.save(&identity).unwrap();
        let loaded = vault.load().unwrap().unwrap();

        assert_eq!(loaded.cert_der, identity.cert_der);
        assert_eq!(loaded.key_der, identity.key_der);
    }

    #[test]
    fn load_returns_none_when_missing() {
        let dir = tempfile::TempDir::new().unwrap();
        let vault = TlsIdentityVault::new(dir.path());

        assert!(vault.load().unwrap().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn save_sets_key_mode_0600() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::TempDir::new().unwrap();
        let vault = TlsIdentityVault::new(dir.path());
        let identity = TlsIdentity::generate("Phone").unwrap();

        vault.save(&identity).unwrap();

        let mode = std::fs::metadata(dir.path().join("localsend-v2.key.der"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
}
