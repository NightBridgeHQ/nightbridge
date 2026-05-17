//! Resolve per-OS config, state, and inbox directories.
//!
//! Uses native platform directories when available and falls back to system
//! paths suited to headless deployments.

use std::path::PathBuf;

const APP_DIR_NAME: &str = "night-bridge";

/// Directory holding `identity.key`, `api.token`, and `config.toml`.
pub fn config_dir() -> PathBuf {
    if let Some(project_dirs) = directories::ProjectDirs::from("dev", "nightbridge", APP_DIR_NAME) {
        return project_dirs.config_dir().to_path_buf();
    }
    PathBuf::from("/etc").join(APP_DIR_NAME)
}

/// Directory holding `trust.db`, manifests, and runtime state.
pub fn state_dir() -> PathBuf {
    if let Some(project_dirs) = directories::ProjectDirs::from("dev", "nightbridge", APP_DIR_NAME) {
        return project_dirs.data_dir().to_path_buf();
    }
    PathBuf::from("/var/lib").join(APP_DIR_NAME)
}

/// Directory where received files land by default.
pub fn default_inbox() -> PathBuf {
    if let Some(user_dirs) = directories::UserDirs::new() {
        if let Some(downloads) = user_dirs.download_dir() {
            return downloads.join(APP_DIR_NAME);
        }
    }
    if let Some(project_dirs) = directories::ProjectDirs::from("dev", "nightbridge", APP_DIR_NAME) {
        return project_dirs.data_dir().join("inbox");
    }
    PathBuf::from("/var/lib").join(APP_DIR_NAME).join("inbox")
}

/// Path to the identity keypair file.
pub fn identity_file() -> PathBuf {
    config_dir().join("identity.key")
}

/// Path to the local API bearer token file.
pub fn api_token_file() -> PathBuf {
    config_dir().join("api.token")
}

/// Path to the trust store database.
pub fn trust_db_file() -> PathBuf {
    state_dir().join("trust.db")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_absolute() {
        assert!(config_dir().is_absolute());
        assert!(state_dir().is_absolute());
        assert!(default_inbox().is_absolute());
        assert!(identity_file().is_absolute());
        assert!(api_token_file().is_absolute());
        assert!(trust_db_file().is_absolute());
    }

    #[test]
    fn api_token_file_is_under_config_dir() {
        assert_eq!(api_token_file(), config_dir().join("api.token"));
    }

    #[test]
    fn config_and_state_under_app_namespace() {
        let cfg = config_dir().to_string_lossy().to_lowercase();
        let st = state_dir().to_string_lossy().to_lowercase();
        assert!(cfg.contains("night-bridge"));
        assert!(st.contains("night-bridge"));
    }
}
