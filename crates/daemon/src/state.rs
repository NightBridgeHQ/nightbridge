use std::path::PathBuf;

use lsi_core::{api_token::ApiToken, config::AppConfig, identity::Keypair, paths};
use metrics_exporter_prometheus::PrometheusHandle;

use crate::{events::EventBus, Args};

#[derive(Clone)]
pub(crate) struct DaemonState {
    pub(crate) alias: String,
    pub(crate) identity: Keypair,
    pub(crate) fingerprint: String,
    pub(crate) trust_db_path: PathBuf,
    pub(crate) inbox_dir: PathBuf,
    pub(crate) localsend_port: u16,
    pub(crate) native_port: u16,
    pub(crate) api_token: ApiToken,
    pub(crate) config: AppConfig,
    pub(crate) metrics: Option<PrometheusHandle>,
    pub(crate) events: EventBus,
}

impl DaemonState {
    #[cfg(test)]
    pub(crate) fn from_args(
        args: &Args,
        identity: Keypair,
        fingerprint: String,
        api_token: ApiToken,
        config: AppConfig,
    ) -> Self {
        Self::from_args_with_metrics(args, identity, fingerprint, api_token, config, None)
    }

    pub(crate) fn from_args_with_metrics(
        args: &Args,
        identity: Keypair,
        fingerprint: String,
        api_token: ApiToken,
        config: AppConfig,
        metrics: Option<PrometheusHandle>,
    ) -> Self {
        Self {
            alias: args.alias.clone(),
            identity,
            fingerprint,
            trust_db_path: args.trust_db.clone().unwrap_or_else(paths::trust_db_file),
            inbox_dir: args.inbox.clone(),
            localsend_port: args.localsend_port,
            native_port: args.native_port,
            api_token,
            config,
            metrics,
            events: EventBus::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use clap::Parser;
    use lsi_core::{
        api_token::ApiToken,
        config::AppConfig,
        identity::{Fingerprint, Keypair},
    };

    use crate::{paths, state::DaemonState, Args};

    #[test]
    fn state_from_args_preserves_runtime_inputs() {
        let args = Args::parse_from([
            "daemon",
            "--alias",
            "workstation",
            "--trust-db",
            "/tmp/lsi-trust.db",
            "--inbox",
            "/tmp/lsi-inbox",
            "--localsend-port",
            "4444",
            "--native-port",
            "4445",
        ]);
        let identity = Keypair::generate();
        let fingerprint = Fingerprint::from_pubkey(&identity.public_bytes()).to_string();
        let api_token = ApiToken::new("temporary-token").unwrap();

        let state = DaemonState::from_args(
            &args,
            identity,
            fingerprint.clone(),
            api_token,
            AppConfig::default(),
        );

        assert_eq!(state.alias, "workstation");
        assert_eq!(state.fingerprint, fingerprint);
        assert_eq!(state.trust_db_path, PathBuf::from("/tmp/lsi-trust.db"));
        assert_eq!(state.inbox_dir, PathBuf::from("/tmp/lsi-inbox"));
        assert_eq!(state.localsend_port, 4444);
        assert_eq!(state.native_port, 4445);
        assert_eq!(state.api_token.expose_secret(), "temporary-token");
        assert_eq!(state.config, AppConfig::default());
        assert_eq!(state.events.subscriber_count(), 0);
    }

    #[test]
    fn state_from_args_uses_default_trust_db() {
        let args = Args::parse_from(["daemon"]);
        let identity = Keypair::generate();
        let fingerprint = Fingerprint::from_pubkey(&identity.public_bytes()).to_string();

        let state = DaemonState::from_args(
            &args,
            identity,
            fingerprint,
            ApiToken::new("token").unwrap(),
            AppConfig::default(),
        );

        assert_eq!(state.trust_db_path, paths::trust_db_file());
    }
}
