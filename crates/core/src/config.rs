//! Daemon configuration loading and validation.

use std::path::Path;

use serde::Deserialize;

use crate::{CoreError, Result};

/// Root application configuration loaded from `config.toml`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    /// Hook sinks that receive daemon events.
    #[serde(default)]
    pub hooks: Vec<HookConfig>,
    /// Metrics endpoint settings.
    #[serde(default)]
    pub metrics: MetricsConfig,
    /// Logging settings.
    #[serde(default)]
    pub logging: LoggingConfig,
    /// WAN rendezvous settings.
    #[serde(default)]
    pub wan: WanConfig,
    /// LocalSend compatibility settings.
    #[serde(default)]
    pub localsend: LocalSendConfig,
}

impl AppConfig {
    /// Validate semantic constraints after TOML deserialization.
    pub fn validate(&self) -> Result<()> {
        for hook in &self.hooks {
            hook.validate()?;
        }
        self.metrics.validate()?;
        self.logging.validate()?;
        self.wan.validate()?;
        self.localsend.validate()?;
        Ok(())
    }
}

/// Hook sink configuration.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HookConfig {
    /// HTTP webhook sink.
    Webhook {
        /// Destination URL.
        url: String,
        /// HMAC signing secret.
        secret: String,
        /// Maximum delivery attempts for retryable failures.
        #[serde(default = "default_webhook_max_attempts")]
        max_attempts: u8,
    },
    /// Local process execution sink.
    Exec {
        /// Executable path or command name.
        command: String,
        /// Timeout for each hook invocation.
        #[serde(default = "default_exec_timeout_seconds")]
        timeout_seconds: u64,
    },
}

impl HookConfig {
    fn validate(&self) -> Result<()> {
        match self {
            Self::Webhook { url, secret, max_attempts } => {
                require_nonblank("webhook url", url)?;
                require_nonblank("webhook secret", secret)?;
                if *max_attempts == 0 {
                    return Err(CoreError::Config(
                        "webhook max_attempts must be greater than zero".to_string(),
                    ));
                }
            }
            Self::Exec { command, timeout_seconds } => {
                require_nonblank("exec command", command)?;
                if *timeout_seconds == 0 {
                    return Err(CoreError::Config(
                        "exec timeout_seconds must be greater than zero".to_string(),
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Metrics endpoint configuration.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct MetricsConfig {
    /// Whether the Prometheus metrics endpoint is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Metrics bind host.
    #[serde(default = "default_metrics_host")]
    pub host: String,
    /// Metrics bind port.
    #[serde(default = "default_metrics_port")]
    pub port: u16,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self { enabled: false, host: default_metrics_host(), port: default_metrics_port() }
    }
}

impl MetricsConfig {
    fn validate(&self) -> Result<()> {
        require_nonblank("metrics host", &self.host)?;
        Ok(())
    }
}

/// Logging configuration.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct LoggingConfig {
    /// Log format: `json`, `pretty`, or `compact`.
    #[serde(default = "default_logging_format")]
    pub format: String,
    /// Default log level used when `RUST_LOG` is not set.
    #[serde(default = "default_logging_level")]
    pub level: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self { format: default_logging_format(), level: default_logging_level() }
    }
}

impl LoggingConfig {
    fn validate(&self) -> Result<()> {
        require_nonblank("logging level", &self.level)?;
        match self.format.as_str() {
            "json" | "pretty" | "compact" => Ok(()),
            other => Err(CoreError::Config(format!(
                "unsupported logging format {other:?}; expected json, pretty, or compact"
            ))),
        }
    }
}

/// WAN rendezvous configuration.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct WanConfig {
    /// Whether WAN rendezvous registration and lookup are enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Rendezvous server URL, such as `quic://rendezvous.example.net:53410`.
    #[serde(default)]
    pub rendezvous_url: Option<String>,
    /// Interval between daemon registration refreshes.
    #[serde(default = "default_wan_register_interval_seconds")]
    pub register_interval_seconds: u64,
    /// STUN servers used to discover server-reflexive candidates.
    #[serde(default)]
    pub stun_servers: Vec<String>,
}

impl Default for WanConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            rendezvous_url: None,
            register_interval_seconds: default_wan_register_interval_seconds(),
            stun_servers: Vec::new(),
        }
    }
}

impl WanConfig {
    fn validate(&self) -> Result<()> {
        if self.enabled {
            match &self.rendezvous_url {
                Some(url) => require_nonblank("rendezvous_url", url)?,
                None => return Err(CoreError::Config("rendezvous_url is required".to_string())),
            }
        }
        if let Some(url) = &self.rendezvous_url {
            require_nonblank("rendezvous_url", url)?;
        }
        if self.register_interval_seconds == 0 {
            return Err(CoreError::Config(
                "wan register_interval_seconds must be greater than zero".to_string(),
            ));
        }
        for stun_server in &self.stun_servers {
            require_nonblank("stun server", stun_server)?;
        }
        Ok(())
    }
}

/// LocalSend compatibility configuration.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct LocalSendConfig {
    /// Receive policy: `prompt`, `trusted`, or `auto`.
    #[serde(default = "default_localsend_receive_policy")]
    pub receive_policy: String,
    /// Inline LocalSend fingerprints allowed when receive policy is `trusted`.
    #[serde(default)]
    pub trusted_fingerprints: Vec<String>,
    /// Optional path to a newline-delimited trusted LocalSend fingerprint file.
    #[serde(default)]
    pub trusted_fingerprints_file: Option<String>,
}

impl Default for LocalSendConfig {
    fn default() -> Self {
        Self {
            receive_policy: default_localsend_receive_policy(),
            trusted_fingerprints: Vec::new(),
            trusted_fingerprints_file: None,
        }
    }
}

impl LocalSendConfig {
    fn validate(&self) -> Result<()> {
        match self.receive_policy.as_str() {
            "prompt" | "trusted" | "auto" => {}
            other => {
                return Err(CoreError::Config(format!(
                    "unsupported localsend receive_policy {other:?}; expected prompt, trusted, or auto"
                )));
            }
        }

        for fingerprint in &self.trusted_fingerprints {
            require_nonblank("trusted LocalSend fingerprint", fingerprint)?;
        }
        if let Some(path) = &self.trusted_fingerprints_file {
            require_nonblank("trusted LocalSend fingerprints file", path)?;
        }

        Ok(())
    }
}

/// Load and validate configuration from an existing path.
pub fn load_config(path: &Path) -> Result<AppConfig> {
    let contents = std::fs::read_to_string(path)?;
    let config: AppConfig =
        toml::from_str(&contents).map_err(|error| CoreError::Config(error.to_string()))?;
    config.validate()?;
    Ok(config)
}

/// Load configuration or return defaults when the file is absent.
pub fn load_config_or_default(path: &Path) -> Result<AppConfig> {
    match load_config(path) {
        Ok(config) => Ok(config),
        Err(CoreError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(AppConfig::default())
        }
        Err(error) => Err(error),
    }
}

fn require_nonblank(name: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(CoreError::Config(format!("{name} is required")));
    }
    Ok(())
}

fn default_webhook_max_attempts() -> u8 {
    3
}

fn default_exec_timeout_seconds() -> u64 {
    10
}

fn default_metrics_host() -> String {
    "127.0.0.1".to_string()
}

fn default_metrics_port() -> u16 {
    53502
}

fn default_logging_format() -> String {
    "json".to_string()
}

fn default_logging_level() -> String {
    "info".to_string()
}

fn default_wan_register_interval_seconds() -> u64 {
    60
}

fn default_localsend_receive_policy() -> String {
    "prompt".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_config_returns_defaults() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = load_config_or_default(&dir.path().join("missing.toml")).unwrap();

        assert_eq!(config, AppConfig::default());
    }

    #[test]
    fn webhook_hook_requires_url_and_secret() {
        let error =
            parse("[[hooks]]\ntype = \"webhook\"\nurl = \"\"\nsecret = \"secret\"\n").unwrap_err();
        assert!(error.to_string().contains("webhook url is required"), "{error}");

        let error =
            parse("[[hooks]]\ntype = \"webhook\"\nurl = \"http://127.0.0.1\"\nsecret = \"\"\n")
                .unwrap_err();
        assert!(error.to_string().contains("webhook secret is required"), "{error}");
    }

    #[test]
    fn exec_hook_requires_command() {
        let error = parse("[[hooks]]\ntype = \"exec\"\ncommand = \"\"\n").unwrap_err();

        assert!(error.to_string().contains("exec command is required"), "{error}");
    }

    #[test]
    fn metrics_bind_defaults_to_disabled_loopback_config() {
        let config = parse("").unwrap();

        assert!(!config.metrics.enabled);
        assert_eq!(config.metrics.host, "127.0.0.1");
        assert_eq!(config.metrics.port, 53502);
    }

    #[test]
    fn wan_config_defaults_to_disabled() {
        let config = parse("").unwrap();

        assert!(!config.wan.enabled);
        assert!(config.wan.rendezvous_url.is_none());
    }

    #[test]
    fn wan_enabled_requires_rendezvous_url() {
        let error = parse("[wan]\nenabled = true\n").unwrap_err();

        assert!(error.to_string().contains("rendezvous_url is required"), "{error}");
    }

    #[test]
    fn wan_config_accepts_rendezvous_and_stun_servers() {
        let config = parse(
            r#"
[wan]
enabled = true
rendezvous_url = "quic://127.0.0.1:53410"
register_interval_seconds = 30
stun_servers = ["stun.example.net:3478"]
"#,
        )
        .unwrap();

        assert!(config.wan.enabled);
        assert_eq!(config.wan.rendezvous_url.as_deref(), Some("quic://127.0.0.1:53410"));
        assert_eq!(config.wan.register_interval_seconds, 30);
        assert_eq!(config.wan.stun_servers, vec!["stun.example.net:3478"]);
    }

    #[test]
    fn wan_config_rejects_zero_register_interval() {
        let error = parse("[wan]\nregister_interval_seconds = 0\n").unwrap_err();

        assert!(
            error.to_string().contains("wan register_interval_seconds must be greater than zero"),
            "{error}"
        );
    }

    #[test]
    fn wan_config_rejects_blank_stun_server() {
        let error = parse("[wan]\nstun_servers = [\"\"]\n").unwrap_err();

        assert!(error.to_string().contains("stun server is required"), "{error}");
    }

    #[test]
    fn localsend_config_defaults_to_prompt() {
        let config = parse("").unwrap();

        assert_eq!(config.localsend.receive_policy, "prompt");
        assert!(config.localsend.trusted_fingerprints.is_empty());
        assert!(config.localsend.trusted_fingerprints_file.is_none());
    }

    #[test]
    fn localsend_config_accepts_trusted_fingerprints_file() {
        let config = parse(
            r#"
[localsend]
receive_policy = "trusted"
trusted_fingerprints = ["ios-fingerprint"]
trusted_fingerprints_file = "/etc/night-bridge/localsend-trusted.txt"
"#,
        )
        .unwrap();

        assert_eq!(config.localsend.receive_policy, "trusted");
        assert_eq!(config.localsend.trusted_fingerprints, vec!["ios-fingerprint"]);
        assert_eq!(
            config.localsend.trusted_fingerprints_file.as_deref(),
            Some("/etc/night-bridge/localsend-trusted.txt")
        );
    }

    #[test]
    fn localsend_config_rejects_invalid_receive_policy() {
        let error = parse("[localsend]\nreceive_policy = \"open\"\n").unwrap_err();

        assert!(error.to_string().contains("unsupported localsend receive_policy"), "{error}");
    }

    #[test]
    fn config_logging_defaults_to_json_info() {
        let config = parse("").unwrap();

        assert_eq!(config.logging.format, "json");
        assert_eq!(config.logging.level, "info");
    }

    #[test]
    fn config_logging_accepts_supported_formats() {
        for format in ["json", "pretty", "compact"] {
            let config =
                parse(&format!("[logging]\nformat = \"{format}\"\nlevel = \"debug\"\n")).unwrap();

            assert_eq!(config.logging.format, format);
            assert_eq!(config.logging.level, "debug");
        }
    }

    #[test]
    fn config_logging_rejects_invalid_format() {
        let error = parse("[logging]\nformat = \"plain\"\n").unwrap_err();

        assert!(error.to_string().contains("unsupported logging format"), "{error}");
    }

    #[test]
    fn unknown_hook_kind_fails_clearly() {
        let error = parse("[[hooks]]\ntype = \"smtp\"\n").unwrap_err();

        assert!(error.to_string().contains("unknown variant `smtp`"), "{error}");
    }

    #[test]
    fn parses_valid_hooks() {
        let config = parse(
            r#"
[[hooks]]
type = "webhook"
url = "http://127.0.0.1:8080/hook"
secret = "secret"

[[hooks]]
type = "exec"
command = "/usr/local/bin/hook"

[metrics]
enabled = true
"#,
        )
        .unwrap();

        assert_eq!(config.hooks.len(), 2);
        assert!(config.metrics.enabled);
    }

    fn parse(contents: &str) -> Result<AppConfig> {
        let config: AppConfig =
            toml::from_str(contents).map_err(|error| CoreError::Config(error.to_string()))?;
        config.validate()?;
        Ok(config)
    }
}
