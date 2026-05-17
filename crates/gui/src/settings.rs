use std::{
    fs,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

const SETTINGS_FILE_NAME: &str = "gui-settings.json";

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GuiMode {
    Remote,
    Standalone,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct GuiSettings {
    pub mode: GuiMode,
    pub remote_endpoint: Option<String>,
    pub api_token: Option<String>,
}

impl Default for GuiSettings {
    fn default() -> Self {
        Self { mode: GuiMode::Remote, remote_endpoint: None, api_token: None }
    }
}

impl GuiSettings {
    pub fn validate_endpoint(endpoint: &str) -> Result<(), SettingsError> {
        let url = Url::parse(endpoint).map_err(|source| SettingsError::InvalidEndpoint {
            endpoint: endpoint.to_string(),
            reason: source.to_string(),
        })?;
        match url.scheme() {
            "http" | "https" => Ok(()),
            scheme => Err(SettingsError::InvalidEndpoint {
                endpoint: endpoint.to_string(),
                reason: format!("unsupported scheme `{scheme}`"),
            }),
        }
    }

    fn normalized(self) -> Result<Self, SettingsError> {
        if let Some(endpoint) = self.remote_endpoint.as_deref() {
            Self::validate_endpoint(endpoint)?;
        }

        Ok(Self {
            mode: self.mode,
            remote_endpoint: self.remote_endpoint.and_then(|endpoint| {
                non_empty(endpoint).map(|endpoint| endpoint.trim_end_matches('/').to_string())
            }),
            api_token: self.api_token.and_then(non_empty),
        })
    }
}

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("could not determine LocalSend Improved config directory")]
    MissingConfigDir,
    #[error("invalid remote endpoint `{endpoint}`: {reason}")]
    InvalidEndpoint { endpoint: String, reason: String },
    #[error("read GUI settings")]
    Read(#[source] std::io::Error),
    #[error("write GUI settings")]
    Write(#[source] std::io::Error),
    #[error("parse GUI settings")]
    Parse(#[source] serde_json::Error),
    #[error("serialize GUI settings")]
    Serialize(#[source] serde_json::Error),
}

pub fn default_settings_path() -> Result<PathBuf, SettingsError> {
    let dirs = ProjectDirs::from("com", "localsendimproved", "LocalSend Improved")
        .ok_or(SettingsError::MissingConfigDir)?;
    Ok(dirs.config_dir().join(SETTINGS_FILE_NAME))
}

pub fn load_settings() -> Result<GuiSettings, SettingsError> {
    load_settings_from(&default_settings_path()?)
}

pub fn save_settings(settings: GuiSettings) -> Result<(), SettingsError> {
    save_settings_to(&default_settings_path()?, settings)
}

pub fn load_settings_from(path: &Path) -> Result<GuiSettings, SettingsError> {
    match fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice::<GuiSettings>(&bytes)
            .map_err(SettingsError::Parse)?
            .normalized()?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(GuiSettings::default()),
        Err(error) => Err(SettingsError::Read(error)),
    }
}

pub fn save_settings_to(path: &Path, settings: GuiSettings) -> Result<(), SettingsError> {
    let settings = settings.normalized()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(SettingsError::Write)?;
    }
    let bytes = serde_json::to_vec_pretty(&settings).map_err(SettingsError::Serialize)?;
    fs::write(path, bytes).map_err(SettingsError::Write)
}

fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[tauri::command]
pub fn gui_load_settings() -> Result<GuiSettings, String> {
    load_settings().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn gui_save_settings(settings: GuiSettings) -> Result<(), String> {
    save_settings(settings).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_start_in_remote_mode_without_secret_token() {
        let settings = GuiSettings::default();

        assert_eq!(settings.mode, GuiMode::Remote);
        assert!(settings.remote_endpoint.is_none());
        assert!(settings.api_token.is_none());
    }

    #[test]
    fn remote_endpoint_must_be_http_or_https() {
        assert!(GuiSettings::validate_endpoint("http://127.0.0.1:53317").is_ok());
        assert!(GuiSettings::validate_endpoint("https://nas.example.test").is_ok());
        assert!(GuiSettings::validate_endpoint("file:///tmp/socket").is_err());
    }

    #[test]
    fn missing_settings_file_returns_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let settings = load_settings_from(&dir.path().join(SETTINGS_FILE_NAME)).unwrap();

        assert_eq!(settings, GuiSettings::default());
    }

    #[test]
    fn save_then_load_roundtrips_normalized_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join(SETTINGS_FILE_NAME);
        let settings = GuiSettings {
            mode: GuiMode::Remote,
            remote_endpoint: Some(" http://127.0.0.1:53317/ ".to_string()),
            api_token: Some(" secret-token ".to_string()),
        };

        save_settings_to(&path, settings).unwrap();
        let loaded = load_settings_from(&path).unwrap();

        assert_eq!(
            loaded,
            GuiSettings {
                mode: GuiMode::Remote,
                remote_endpoint: Some("http://127.0.0.1:53317".to_string()),
                api_token: Some("secret-token".to_string()),
            }
        );
    }
}
