use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use lsi_protocol_native_v1::candidates::{
    gather_candidates, CandidateKind as NativeCandidateKind, NativeCandidate,
};
use lsi_rendezvous::client::RendezvousClient;
use lsi_rendezvous::protocol::{Candidate, CandidateKind, RegisterRequest, RegisteredResponse};
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::state::DaemonState;

/// Background WAN rendezvous registration task.
pub(crate) struct WanRuntime {
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    task: JoinHandle<Result<()>>,
}

/// Starts WAN rendezvous registration when enabled in daemon config.
pub(crate) async fn start_wan_runtime(
    state: Arc<DaemonState>,
    native_port: u16,
) -> Result<Option<WanRuntime>> {
    if !state.config.wan.enabled {
        return Ok(None);
    }

    let rendezvous_url = state
        .config
        .wan
        .rendezvous_url
        .clone()
        .context("rendezvous_url is required when WAN is enabled")?;
    let stun_servers = resolve_stun_servers(&state.config.wan.stun_servers).await?;
    let register_interval = Duration::from_secs(state.config.wan.register_interval_seconds);
    let ttl_seconds = registration_ttl_seconds(state.config.wan.register_interval_seconds);
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let task = tokio::spawn(async move {
        run_register_loop(
            state,
            native_port,
            rendezvous_url,
            stun_servers,
            ttl_seconds,
            register_interval,
            shutdown_rx,
        )
        .await
    });

    Ok(Some(WanRuntime { shutdown_tx, task }))
}

/// Stops the WAN rendezvous registration task.
pub(crate) async fn stop_wan_runtime(runtime: WanRuntime) -> Result<()> {
    let _ = runtime.shutdown_tx.send(true);
    runtime.task.await.context("WAN rendezvous task panicked")?
}

async fn resolve_stun_servers(servers: &[String]) -> Result<Vec<SocketAddr>> {
    let mut resolved = Vec::new();
    for server in servers {
        let addrs = tokio::net::lookup_host(server)
            .await
            .with_context(|| format!("failed to resolve STUN server {server}"))?
            .collect::<Vec<_>>();
        if addrs.is_empty() {
            bail!("STUN server resolved no addresses: {server}");
        }
        resolved.extend(addrs);
    }
    Ok(resolved)
}

fn registration_ttl_seconds(register_interval_seconds: u64) -> u64 {
    register_interval_seconds.saturating_mul(3).max(1)
}

async fn run_register_loop(
    state: Arc<DaemonState>,
    native_port: u16,
    rendezvous_url: String,
    stun_servers: Vec<SocketAddr>,
    ttl_seconds: u64,
    register_interval: Duration,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    run_register_loop_with_register(
        state,
        native_port,
        stun_servers,
        ttl_seconds,
        register_interval,
        shutdown_rx,
        move |request| {
            let rendezvous_url = rendezvous_url.clone();
            async move {
                let client = RendezvousClient::connect(&rendezvous_url).await?;
                let response = client.register(request).await;
                client.close().await;
                response
            }
        },
    )
    .await
}

async fn run_register_loop_with_register<F, Fut>(
    state: Arc<DaemonState>,
    native_port: u16,
    stun_servers: Vec<SocketAddr>,
    ttl_seconds: u64,
    register_interval: Duration,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    mut register: F,
) -> Result<()>
where
    F: FnMut(RegisterRequest) -> Fut,
    Fut: Future<Output = Result<RegisteredResponse>>,
{
    while !*shutdown_rx.borrow() {
        let candidates = gather_candidates(native_port, &stun_servers).await?;
        let request = build_register_request(&state, &candidates, ttl_seconds);
        let candidate_set = request.candidates.clone();

        match register(request).await {
            Ok(response) => {
                info!(
                    candidates = ?candidate_set,
                    ttl_seconds = response.ttl_seconds,
                    expires_at_unix_seconds = response.expires_at_unix_seconds,
                    "registered WAN candidates with rendezvous"
                );
            }
            Err(error) => {
                warn!(%error, "failed to register WAN candidates with rendezvous");
            }
        }

        if wait_for_register_interval(register_interval, &mut shutdown_rx).await {
            return Ok(());
        }
    }

    Ok(())
}

async fn wait_for_register_interval(
    register_interval: Duration,
    shutdown_rx: &mut tokio::sync::watch::Receiver<bool>,
) -> bool {
    tokio::select! {
        changed = shutdown_rx.changed() => changed.is_err() || *shutdown_rx.borrow(),
        _ = tokio::time::sleep(register_interval) => false,
    }
}

fn build_register_request(
    state: &DaemonState,
    candidates: &[NativeCandidate],
    ttl_seconds: u64,
) -> RegisterRequest {
    RegisterRequest {
        alias: state.alias.clone(),
        pubkey: state.identity.public_bytes(),
        candidates: candidates.iter().map(candidate_to_rendezvous).collect(),
        ttl_seconds,
    }
}

fn candidate_to_rendezvous(candidate: &NativeCandidate) -> Candidate {
    Candidate {
        kind: match candidate.kind {
            NativeCandidateKind::Local => CandidateKind::Local,
            NativeCandidateKind::ServerReflexive => CandidateKind::ServerReflexive,
        },
        address: candidate.address.to_string(),
        priority: candidate.priority,
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use anyhow::anyhow;
    use clap::Parser;
    use lsi_core::api_token::ApiToken;
    use lsi_core::config::{AppConfig, WanConfig};
    use lsi_core::identity::{Fingerprint, Keypair};

    use crate::Args;

    use super::*;

    #[tokio::test]
    async fn disabled_wan_starts_no_task() {
        let state = Arc::new(test_state(AppConfig::default()));

        let runtime = start_wan_runtime(state, 53400).await.unwrap();

        assert!(runtime.is_none());
    }

    #[tokio::test]
    async fn stun_servers_resolve_socket_addresses() {
        let servers = vec!["127.0.0.1:3478".to_string()];

        let resolved = resolve_stun_servers(&servers).await.unwrap();

        assert_eq!(resolved, vec![SocketAddr::from((Ipv4Addr::LOCALHOST, 3478))]);
    }

    #[test]
    fn enabled_wan_builds_register_request_from_identity_and_candidates() {
        let state = test_state(enabled_config());
        let candidate = NativeCandidate {
            kind: NativeCandidateKind::ServerReflexive,
            address: SocketAddr::from((Ipv4Addr::new(203, 0, 113, 10), 53400)),
            priority: 80,
        };

        let request = build_register_request(&state, &[candidate], 180);

        assert_eq!(request.alias, state.alias);
        assert_eq!(request.pubkey, state.identity.public_bytes());
        assert_eq!(request.ttl_seconds, 180);
        assert_eq!(request.candidates.len(), 1);
        assert_eq!(request.candidates[0].kind, CandidateKind::ServerReflexive);
        assert_eq!(request.candidates[0].address, "203.0.113.10:53400");
        assert_eq!(request.candidates[0].priority, 80);
    }

    #[tokio::test]
    async fn register_loop_retries_after_transient_failure() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let state = Arc::new(test_state(enabled_config()));
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let register_attempts = Arc::clone(&attempts);

        run_register_loop_with_register(
            state,
            53400,
            Vec::new(),
            180,
            Duration::from_millis(1),
            shutdown_rx,
            move |_request| {
                let attempts = Arc::clone(&register_attempts);
                let shutdown_tx = shutdown_tx.clone();
                async move {
                    let attempt = attempts.fetch_add(1, Ordering::SeqCst) + 1;
                    if attempt == 1 {
                        return Err(anyhow!("transient rendezvous failure"));
                    }
                    let _ = shutdown_tx.send(true);
                    Ok(RegisteredResponse { ttl_seconds: 180, expires_at_unix_seconds: 123 })
                }
            },
        )
        .await
        .unwrap();

        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn shutdown_stops_register_loop() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let state = Arc::new(test_state(enabled_config()));
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(true);
        let register_attempts = Arc::clone(&attempts);

        run_register_loop_with_register(
            state,
            53400,
            Vec::new(),
            180,
            Duration::from_millis(1),
            shutdown_rx,
            move |_request| {
                let attempts = Arc::clone(&register_attempts);
                async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Ok(RegisteredResponse { ttl_seconds: 180, expires_at_unix_seconds: 123 })
                }
            },
        )
        .await
        .unwrap();

        drop(shutdown_tx);
        assert_eq!(attempts.load(Ordering::SeqCst), 0);
    }

    fn enabled_config() -> AppConfig {
        AppConfig {
            wan: WanConfig {
                enabled: true,
                rendezvous_url: Some("quic://127.0.0.1:53410".to_string()),
                register_interval_seconds: 60,
                stun_servers: Vec::new(),
            },
            ..AppConfig::default()
        }
    }

    fn test_state(config: AppConfig) -> DaemonState {
        let args = Args::parse_from([
            "daemon",
            "--alias",
            "wan-peer",
            "--native-port",
            "53400",
            "--disable-native-discovery",
        ]);
        let identity = Keypair::generate();
        let fingerprint = Fingerprint::from_pubkey(&identity.public_bytes()).to_string();
        DaemonState::from_args(
            &args,
            identity,
            fingerprint,
            ApiToken::new("test-token").unwrap(),
            config,
        )
    }
}
