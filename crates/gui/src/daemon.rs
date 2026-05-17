use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use directories::ProjectDirs;
use serde::Serialize;
use thiserror::Error;

const API_HTTP_PORT: u16 = 53561;
const API_GRPC_PORT: u16 = 53562;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DaemonSpec {
    pub executable: PathBuf,
    pub state_root: PathBuf,
    pub inbox_dir: PathBuf,
    pub api_http_port: u16,
    pub api_grpc_port: u16,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

impl DaemonSpec {
    pub fn new() -> Result<Self, DaemonError> {
        let dirs = ProjectDirs::from("com", "localsendimproved", "LocalSend Improved")
            .ok_or(DaemonError::MissingStateDir)?;
        Ok(Self::from_root(dirs.data_dir().join("standalone-daemon")))
    }

    pub fn from_root(root: impl Into<PathBuf>) -> Self {
        let state_root = root.into();
        let inbox_dir = state_root.join("inbox");
        let executable = daemon_executable_path();
        let args = vec![
            "--alias".to_string(),
            "LocalSend Improved Desktop".to_string(),
            "--api-http-port".to_string(),
            API_HTTP_PORT.to_string(),
            "--api-grpc-port".to_string(),
            API_GRPC_PORT.to_string(),
            "--inbox".to_string(),
            inbox_dir.to_string_lossy().to_string(),
        ];
        let root_string = state_root.to_string_lossy().to_string();
        let env = vec![
            ("HOME".to_string(), root_string.clone()),
            ("XDG_CONFIG_HOME".to_string(), format!("{root_string}/config")),
            ("XDG_DATA_HOME".to_string(), format!("{root_string}/data")),
        ];

        Self {
            executable,
            state_root,
            inbox_dir,
            api_http_port: API_HTTP_PORT,
            api_grpc_port: API_GRPC_PORT,
            args,
            env,
        }
    }

    #[cfg(test)]
    fn new_for_test(root: &str) -> Self {
        Self::from_root(root)
    }

    fn api_token_path(&self) -> PathBuf {
        self.state_root
            .join("Library")
            .join("Application Support")
            .join("dev.lsi.localsend-improved")
            .join("api.token")
    }

    fn endpoint(&self) -> String {
        format!("http://127.0.0.1:{}", self.api_http_port)
    }
}

#[derive(Debug, Error)]
pub enum DaemonError {
    #[error("could not determine LocalSend Improved standalone daemon directory")]
    MissingStateDir,
    #[error("embedded daemon is already running")]
    AlreadyRunning,
    #[error("embedded daemon is not running")]
    NotRunning,
    #[error("prepare embedded daemon directories")]
    Prepare(#[source] std::io::Error),
    #[error("start embedded daemon at {path}")]
    Start {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("stop embedded daemon")]
    Stop(#[source] std::io::Error),
    #[error("read embedded daemon API token")]
    ReadToken(#[source] std::io::Error),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EmbeddedDaemonStatus {
    pub running: bool,
    pub endpoint: Option<String>,
    pub api_token: Option<String>,
}

#[derive(Default)]
pub struct EmbeddedDaemonManager {
    child: Option<Child>,
    spec: Option<DaemonSpec>,
}

impl EmbeddedDaemonManager {
    pub fn start(&mut self) -> Result<EmbeddedDaemonStatus, DaemonError> {
        if self.child.is_some() {
            return self.status();
        }

        let spec = DaemonSpec::new()?;
        fs::create_dir_all(&spec.inbox_dir).map_err(DaemonError::Prepare)?;
        let mut command = Command::new(&spec.executable);
        command.args(&spec.args).stdout(Stdio::null()).stderr(Stdio::null());
        for (key, value) in &spec.env {
            command.env(key, value);
        }
        let child = command.spawn().map_err(|source| DaemonError::Start {
            path: spec.executable.display().to_string(),
            source,
        })?;
        let status = EmbeddedDaemonStatus {
            running: true,
            endpoint: Some(spec.endpoint()),
            api_token: read_optional_token(&spec.api_token_path())?,
        };
        self.child = Some(child);
        self.spec = Some(spec);
        Ok(status)
    }

    pub fn stop(&mut self) -> Result<(), DaemonError> {
        let Some(mut child) = self.child.take() else {
            self.spec = None;
            return Ok(());
        };
        child.kill().map_err(DaemonError::Stop)?;
        child.wait().map_err(DaemonError::Stop)?;
        self.spec = None;
        Ok(())
    }

    pub fn status(&mut self) -> Result<EmbeddedDaemonStatus, DaemonError> {
        if let Some(child) = self.child.as_mut() {
            if child.try_wait().map_err(DaemonError::Stop)?.is_some() {
                self.child = None;
                self.spec = None;
                return Ok(EmbeddedDaemonStatus {
                    running: false,
                    endpoint: None,
                    api_token: None,
                });
            }
            let spec = self.spec.as_ref().ok_or(DaemonError::NotRunning)?;
            return Ok(EmbeddedDaemonStatus {
                running: true,
                endpoint: Some(spec.endpoint()),
                api_token: read_optional_token(&spec.api_token_path())?,
            });
        }

        Ok(EmbeddedDaemonStatus { running: false, endpoint: None, api_token: None })
    }
}

impl Drop for EmbeddedDaemonManager {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[tauri::command]
pub fn gui_start_embedded_daemon(
    manager: tauri::State<'_, std::sync::Mutex<EmbeddedDaemonManager>>,
) -> Result<EmbeddedDaemonStatus, String> {
    manager
        .lock()
        .map_err(|_| "embedded daemon manager lock poisoned".to_string())?
        .start()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn gui_stop_embedded_daemon(
    manager: tauri::State<'_, std::sync::Mutex<EmbeddedDaemonManager>>,
) -> Result<(), String> {
    manager
        .lock()
        .map_err(|_| "embedded daemon manager lock poisoned".to_string())?
        .stop()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn gui_embedded_daemon_status(
    manager: tauri::State<'_, std::sync::Mutex<EmbeddedDaemonManager>>,
) -> Result<EmbeddedDaemonStatus, String> {
    manager
        .lock()
        .map_err(|_| "embedded daemon manager lock poisoned".to_string())?
        .status()
        .map_err(|error| error.to_string())
}

fn daemon_executable_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .map(|dir| dir.join(format!("localsend-improved-daemon{}", std::env::consts::EXE_SUFFIX)))
        .unwrap_or_else(|| {
            PathBuf::from(format!("localsend-improved-daemon{}", std::env::consts::EXE_SUFFIX))
        })
}

fn read_optional_token(path: &Path) -> Result<Option<String>, DaemonError> {
    match fs::read_to_string(path) {
        Ok(token) => Ok(Some(token.trim().to_string()).filter(|token| !token.is_empty())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(DaemonError::ReadToken(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_args_use_loopback_api_and_private_state_root() {
        let spec = DaemonSpec::new_for_test("/tmp/lsi-gui-test");

        assert!(spec.args.contains(&"--api-http-port".to_string()));
        assert!(spec.args.contains(&"--api-grpc-port".to_string()));
        assert!(spec.env.iter().any(|(key, value)| key == "HOME" && value == "/tmp/lsi-gui-test"));
        assert!(spec.args.iter().any(|arg| arg == "/tmp/lsi-gui-test/inbox"));
    }

    #[test]
    fn manager_status_starts_stopped() {
        let mut manager = EmbeddedDaemonManager::default();

        assert_eq!(
            manager.status().unwrap(),
            EmbeddedDaemonStatus { running: false, endpoint: None, api_token: None }
        );
    }
}
