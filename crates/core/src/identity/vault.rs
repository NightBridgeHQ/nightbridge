//! Persistent storage of the node's keypair.

use std::path::{Path, PathBuf};

use crate::error::{CoreError, Result};
use crate::identity::Keypair;

/// Persists and loads the node's [`Keypair`].
pub trait IdentityVault {
    /// Load the keypair from storage, returning `None` if no key is present.
    fn load(&self) -> Result<Option<Keypair>>;

    /// Save the keypair, overwriting any prior value.
    fn save(&self, keypair: &Keypair) -> Result<()>;

    /// Delete the stored keypair, if one exists.
    fn delete(&self) -> Result<()>;
}

/// Filesystem-backed [`IdentityVault`].
///
/// Stores the 32-byte secret key at the configured path. On Unix, saved files
/// are set to `0600`; on other platforms the file inherits OS defaults.
pub struct FsVault {
    path: PathBuf,
}

impl FsVault {
    /// Create a filesystem vault at `path`.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Return the storage path used by this vault.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl IdentityVault for FsVault {
    fn load(&self) -> Result<Option<Keypair>> {
        if !self.path.exists() {
            return Ok(None);
        }

        let bytes = std::fs::read(&self.path)?;
        if bytes.len() != 32 {
            return Err(CoreError::IdentityVault(format!(
                "expected 32 bytes, found {}",
                bytes.len()
            )));
        }

        let mut secret = [0u8; 32];
        secret.copy_from_slice(&bytes);
        Ok(Some(Keypair::from_secret_bytes(&secret)))
    }

    fn save(&self, keypair: &Keypair) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let tmp_path = self.path.with_extension("key.tmp");
        std::fs::write(&tmp_path, keypair.secret_bytes())?;
        set_owner_only_permissions(&tmp_path)?;
        std::fs::rename(&tmp_path, &self.path)?;
        Ok(())
    }

    fn delete(&self) -> Result<()> {
        if self.path.exists() {
            std::fs::remove_file(&self.path)?;
        }
        Ok(())
    }
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
    use tempfile::TempDir;

    #[test]
    fn save_then_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let vault = FsVault::new(dir.path().join("identity.key"));
        let keypair = Keypair::generate();

        vault.save(&keypair).unwrap();
        let loaded = vault.load().unwrap().unwrap();

        assert_eq!(keypair.public_bytes(), loaded.public_bytes());
    }

    #[test]
    fn load_returns_none_when_missing() {
        let dir = TempDir::new().unwrap();
        let vault = FsVault::new(dir.path().join("identity.key"));

        assert!(vault.load().unwrap().is_none());
    }

    #[test]
    fn save_creates_parent_directory() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("a").join("b").join("identity.key");
        let vault = FsVault::new(&path);

        vault.save(&Keypair::generate()).unwrap();

        assert!(path.exists());
    }

    #[test]
    fn delete_removes_file() {
        let dir = TempDir::new().unwrap();
        let vault = FsVault::new(dir.path().join("identity.key"));

        vault.save(&Keypair::generate()).unwrap();
        vault.delete().unwrap();

        assert!(vault.load().unwrap().is_none());
    }

    #[test]
    fn load_rejects_wrong_length_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("identity.key");
        std::fs::write(&path, b"too short").unwrap();
        let vault = FsVault::new(&path);

        assert!(vault.load().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn save_sets_mode_0600() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("identity.key");
        let vault = FsVault::new(&path);

        vault.save(&Keypair::generate()).unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
