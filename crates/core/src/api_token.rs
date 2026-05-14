//! Persistent storage for the local API bearer token.

use std::path::{Path, PathBuf};

use rand::RngCore;

use crate::error::{CoreError, Result};

/// Local API bearer token.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApiToken(String);

impl ApiToken {
    /// Create a token, rejecting empty or whitespace-only values.
    pub fn new(token: impl AsRef<str>) -> Result<Self> {
        let token = token.as_ref().trim();
        if token.is_empty() {
            return Err(CoreError::IdentityVault("api token is blank".to_string()));
        }
        Ok(Self(token.to_string()))
    }

    /// Return the raw token string.
    pub fn expose_secret(&self) -> &str {
        &self.0
    }

    fn generate() -> Self {
        let mut bytes = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        Self(base64url_no_pad(&bytes))
    }
}

/// Persists and loads the local API bearer token.
pub trait ApiTokenVault {
    /// Load the token from storage, returning `None` if no token is present.
    fn load(&self) -> Result<Option<ApiToken>>;

    /// Load the stored token or generate and persist a new one when missing.
    fn load_or_generate(&self) -> Result<ApiToken>;

    /// Save the token, overwriting any prior value.
    fn save(&self, token: &ApiToken) -> Result<()>;
}

/// Filesystem-backed [`ApiTokenVault`].
///
/// On Unix, saved token files are set to `0600`; on other platforms the file
/// inherits OS defaults.
pub struct FsApiTokenVault {
    path: PathBuf,
}

impl FsApiTokenVault {
    /// Create a filesystem vault at `path`.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Return the storage path used by this vault.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl ApiTokenVault for FsApiTokenVault {
    fn load(&self) -> Result<Option<ApiToken>> {
        if !self.path.exists() {
            return Ok(None);
        }

        let token = std::fs::read_to_string(&self.path)?;
        Ok(Some(ApiToken::new(token)?))
    }

    fn load_or_generate(&self) -> Result<ApiToken> {
        if let Some(token) = self.load()? {
            return Ok(token);
        }

        let token = ApiToken::generate();
        self.save(&token)?;
        Ok(token)
    }

    fn save(&self, token: &ApiToken) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let tmp_path = self.path.with_extension("token.tmp");
        write_token_file(&tmp_path, token.expose_secret())?;
        set_owner_only_permissions(&tmp_path)?;
        std::fs::rename(&tmp_path, &self.path)?;
        Ok(())
    }
}

fn base64url_no_pad(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

    let mut encoded = String::with_capacity((bytes.len() * 4).div_ceil(3));
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);

        encoded.push(TABLE[(b0 >> 2) as usize] as char);
        encoded.push(TABLE[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            encoded.push(TABLE[(((b1 & 0b0000_1111) << 2) | (b2 >> 6)) as usize] as char);
        }
        if chunk.len() > 2 {
            encoded.push(TABLE[(b2 & 0b0011_1111) as usize] as char);
        }
    }
    encoded
}

#[cfg(unix)]
fn write_token_file(path: &Path, token: &str) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(token.as_bytes())?;
    Ok(())
}

#[cfg(not(unix))]
fn write_token_file(path: &Path, token: &str) -> Result<()> {
    std::fs::write(path, token)?;
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
    use tempfile::TempDir;

    #[test]
    fn load_returns_none_when_missing() {
        let dir = TempDir::new().unwrap();
        let vault = FsApiTokenVault::new(dir.path().join("api.token"));

        assert!(vault.load().unwrap().is_none());
    }

    #[test]
    fn load_rejects_blank_token() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("api.token");
        std::fs::write(&path, b" \n\t").unwrap();
        let vault = FsApiTokenVault::new(&path);

        assert!(vault.load().is_err());
    }

    #[test]
    fn load_or_generate_persists_token() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("api.token");
        let vault = FsApiTokenVault::new(&path);

        let token = vault.load_or_generate().unwrap();

        assert!(!token.expose_secret().is_empty());
        assert_eq!(token, vault.load().unwrap().unwrap());
        assert_eq!(token.expose_secret(), std::fs::read_to_string(&path).unwrap().trim());
    }

    #[cfg(unix)]
    #[test]
    fn save_sets_mode_0600() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("api.token");
        let vault = FsApiTokenVault::new(&path);
        let token = ApiToken::new("test-token").unwrap();

        vault.save(&token).unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
