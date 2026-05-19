use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use lsi_core::{
    api_token::{ApiTokenVault, FsApiTokenVault},
    config::{load_config_or_default, AppConfig, LocalSendConfig, LoggingConfig},
    identity::{Fingerprint, FsVault, IdentityVault, Keypair},
    paths,
    trust::{LocalSendLanPeer, PeerPolicy, TrustStore},
};
use lsi_protocol_localsend_v2::{
    discovery::{DiscoveryAnnouncer, LanPeer},
    dto::{DeviceInfo, Protocol},
    server::{LocalSendReceivePolicy, LocalSendServer, LocalSendServerConfig},
    tls::{TlsIdentity, TlsIdentityVault},
};
use lsi_protocol_native_v1::{
    discovery::NativeDiscoveryAnnouncer,
    dto::{
        default_extensions, negotiate_extensions, HelloAck, NativePeerInfo, RequestTransfer,
        PROTOCOL_VERSION,
    },
    framing::ControlMessage,
    manifest::ManifestStore,
    tls::NativeTlsIdentity,
    transfer::{read_chunk_frame, NativeTransferReceiver},
    transport::{
        bind_server_endpoint, NativeControlStream, NativeServerBind, NativeTransportConfig,
    },
};
use tokio::task::JoinHandle;
use tracing::{info, warn};
use tracing_subscriber::{fmt, EnvFilter};

mod api;
mod events;
mod hooks;
mod localsend_lan;
mod metrics;
mod native_lan;
mod state;
mod wan;

use state::DaemonState;

const DEFAULT_LOCALSEND_PORT: u16 = 53317;
const DEFAULT_NATIVE_PORT: u16 = 53400;
const DEFAULT_API_GRPC_PORT: u16 = 53500;
const DEFAULT_API_HTTP_PORT: u16 = 53501;
const DEFAULT_ALIAS: &str = "night-bridge";
const LOCALSEND_VERSION: &str = "2.0";
const LOCALSEND_SESSION_TTL: Duration = Duration::from_secs(60 * 60);

/// Headless daemon for NightBridge.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// Path to the node identity keypair file.
    #[arg(long)]
    identity: Option<PathBuf>,

    /// Path to the trust store SQLite database.
    #[arg(long = "trust-db")]
    trust_db: Option<PathBuf>,

    /// HTTPS port for the LocalSend v2 receiver.
    #[arg(long = "localsend-port", default_value_t = DEFAULT_LOCALSEND_PORT)]
    localsend_port: u16,

    /// Alias advertised to LocalSend v2 peers.
    #[arg(long, default_value_t = default_alias())]
    alias: String,

    /// Directory where received LocalSend v2 files are stored.
    #[arg(long, default_value_os_t = paths::default_inbox())]
    inbox: PathBuf,

    /// Disable the LocalSend v2 receiver and LAN discovery announcer.
    #[arg(long = "disable-localsend-v2")]
    disable_localsend_v2: bool,

    /// Incoming LocalSend v2 receive policy: prompt rejects until approved, trusted accepts only allowlisted fingerprints, auto accepts all LAN senders.
    #[arg(long = "localsend-receive-policy", value_enum)]
    localsend_receive_policy: Option<LocalSendReceivePolicyArg>,

    /// LocalSend peer fingerprint allowed when --localsend-receive-policy=trusted. May be repeated.
    #[arg(long = "trusted-localsend-fingerprint")]
    trusted_localsend_fingerprint: Vec<String>,

    /// UDP port for the native QUIC listener.
    #[arg(long = "native-port", default_value_t = DEFAULT_NATIVE_PORT)]
    native_port: u16,

    /// Disable the native QUIC listener.
    #[arg(long = "disable-native")]
    disable_native: bool,

    /// Disable native mDNS discovery advertisement.
    #[arg(long = "disable-native-discovery")]
    disable_native_discovery: bool,

    /// Path to the native transfer manifest SQLite database.
    #[arg(long = "native-manifest-db")]
    native_manifest_db: Option<PathBuf>,

    /// Path to the daemon configuration file.
    #[arg(long = "config")]
    config: Option<PathBuf>,

    /// WAN rendezvous server URL, such as quic://host:53410.
    #[arg(long = "rendezvous")]
    rendezvous: Option<String>,

    /// Loopback gRPC API port.
    #[arg(long = "api-grpc-port", default_value_t = DEFAULT_API_GRPC_PORT)]
    api_grpc_port: u16,

    /// Loopback HTTP+SSE API port.
    #[arg(long = "api-http-port", default_value_t = DEFAULT_API_HTTP_PORT)]
    api_http_port: u16,

    /// Disable the local daemon API.
    #[arg(long = "disable-api")]
    disable_api: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum LocalSendReceivePolicyArg {
    Prompt,
    Trusted,
    Auto,
}

impl From<LocalSendReceivePolicyArg> for LocalSendReceivePolicy {
    fn from(value: LocalSendReceivePolicyArg) -> Self {
        match value {
            LocalSendReceivePolicyArg::Prompt => Self::Prompt,
            LocalSendReceivePolicyArg::Trusted => Self::Trusted,
            LocalSendReceivePolicyArg::Auto => Self::Auto,
        }
    }
}

#[derive(Debug)]
struct LocalSendRuntime {
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    server_task: JoinHandle<Result<()>>,
    announcer_task: JoinHandle<Result<()>>,
    peer_recorder_task: JoinHandle<()>,
}

struct NativeRuntime {
    local_addr: SocketAddr,
    endpoint: quinn::Endpoint,
    discovery_announcer: Option<NativeDiscoveryAnnouncer>,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    server_task: JoinHandle<Result<()>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let config_path = config_path(&args);
    let config = load_daemon_config(&args)
        .with_context(|| format!("failed to load daemon config at {}", config_path.display()))?;
    init_tracing(&config.logging);
    let identity_path = args.identity.clone().unwrap_or_else(paths::identity_file);

    let vault = FsVault::new(&identity_path);
    let identity = load_or_create_identity(&vault).with_context(|| {
        format!("failed to load or create identity at {}", identity_path.display())
    })?;
    let fingerprint = Fingerprint::from_pubkey(&identity.public_bytes()).to_string();
    let api_token_path = paths::api_token_file();
    let api_token =
        FsApiTokenVault::new(&api_token_path).load_or_generate().with_context(|| {
            format!("failed to load or create API token at {}", api_token_path.display())
        })?;
    let metrics = if config.metrics.enabled {
        Some(metrics::install().with_context(|| "failed to install Prometheus recorder")?.handle)
    } else {
        None
    };
    let state = Arc::new(DaemonState::from_args_with_metrics(
        &args,
        identity,
        fingerprint,
        api_token,
        config,
        metrics,
    ));
    let api_auth_enabled = !state.api_token.expose_secret().is_empty();
    let event_subscribers = state.events.subscriber_count();

    ensure_parent_dir(&state.trust_db_path).with_context(|| {
        format!("failed to prepare trust store at {}", state.trust_db_path.display())
    })?;
    let _trust_store = TrustStore::open(&state.trust_db_path).with_context(|| {
        format!("failed to open trust store at {}", state.trust_db_path.display())
    })?;

    info!(fp = %state.fingerprint, "identity loaded");
    info!(path = %state.trust_db_path.display(), "trust store opened");
    info!(path = %config_path.display(), "daemon config loaded");
    info!(path = %api_token_path.display(), enabled = api_auth_enabled, "API token loaded");
    info!(
        identity = %identity_path.display(),
        trust_db = %state.trust_db_path.display(),
        event_subscribers,
        "daemon initialized"
    );

    let hook_runtime = hooks::start_hook_dispatcher(&state.config, &state.events);
    info!(hooks = state.config.hooks.len(), "hook dispatcher started");

    let api_runtime = if args.disable_api {
        info!("local daemon API disabled");
        None
    } else {
        let grpc_runtime = api::grpc::start_grpc_runtime(Arc::clone(&state), args.api_grpc_port)
            .await
            .with_context(|| "failed to start gRPC API")?;
        let http_runtime = api::http::start_http_runtime(Arc::clone(&state), args.api_http_port)
            .await
            .with_context(|| "failed to start HTTP API")?;
        info!(addr = %grpc_runtime.local_addr(), "gRPC API started");
        info!(addr = %http_runtime.local_addr(), "HTTP API started");
        Some((grpc_runtime, http_runtime))
    };

    let localsend_runtime = if args.disable_localsend_v2 {
        info!("LocalSend v2 receiver disabled");
        None
    } else {
        Some(
            start_localsend_v2(&state)
                .await
                .with_context(|| "failed to start LocalSend v2 receiver")?,
        )
    };

    let native_runtime = if args.disable_native {
        info!("native LAN listener disabled");
        None
    } else {
        Some(
            start_native_runtime(&args, &state)
                .await
                .with_context(|| "failed to start native LAN listener")?,
        )
    };
    let wan_runtime = if let Some(runtime) = &native_runtime {
        wan::start_wan_runtime(Arc::clone(&state), runtime.local_addr.port())
            .await
            .with_context(|| "failed to start WAN rendezvous runtime")?
    } else {
        if state.config.wan.enabled {
            warn!("WAN rendezvous disabled because native listener is disabled");
        }
        None
    };

    wait_for_shutdown().await?;

    if let Some(runtime) = wan_runtime {
        wan::stop_wan_runtime(runtime).await?;
    }

    if let Some(runtime) = native_runtime {
        stop_native_runtime(runtime).await?;
    }

    if let Some(runtime) = localsend_runtime {
        stop_localsend_v2(runtime).await?;
    }

    if let Some((grpc_runtime, http_runtime)) = api_runtime {
        api::http::stop_http_runtime(http_runtime).await?;
        api::grpc::stop_grpc_runtime(grpc_runtime).await?;
    }

    hooks::stop_hook_dispatcher(hook_runtime).await;

    info!("shutdown");
    Ok(())
}

fn init_tracing(logging: &LoggingConfig) {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(logging.level.clone()));
    let subscriber = fmt().with_env_filter(filter).with_writer(std::io::stderr);

    match logging.format.as_str() {
        "pretty" => subscriber.pretty().init(),
        "compact" => subscriber.compact().init(),
        _ => subscriber.json().init(),
    }
}

fn load_or_create_identity(vault: &FsVault) -> Result<Keypair> {
    if let Some(identity) = vault.load()? {
        info!(path = %vault.path().display(), "loaded existing identity");
        return Ok(identity);
    }

    let identity = Keypair::generate();
    vault.save(&identity)?;
    info!(path = %vault.path().display(), "created new identity");
    Ok(identity)
}

fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    } else {
        warn!(path = %path.display(), "trust store path has no parent directory");
    }
    Ok(())
}

fn config_path(args: &Args) -> PathBuf {
    args.config.clone().unwrap_or_else(|| paths::config_dir().join("config.toml"))
}

fn load_daemon_config(args: &Args) -> lsi_core::Result<AppConfig> {
    let mut config = load_config_or_default(&config_path(args))?;
    if let Some(url) = &args.rendezvous {
        config.wan.enabled = true;
        config.wan.rendezvous_url = Some(url.clone());
        config.validate()?;
    }
    Ok(config)
}

fn localsend_receive_policy(
    args_policy: Option<LocalSendReceivePolicyArg>,
    config: &LocalSendConfig,
) -> Result<LocalSendReceivePolicy> {
    if let Some(policy) = args_policy {
        return Ok(policy.into());
    }

    match config.receive_policy.as_str() {
        "prompt" => Ok(LocalSendReceivePolicy::Prompt),
        "trusted" => Ok(LocalSendReceivePolicy::Trusted),
        "auto" => Ok(LocalSendReceivePolicy::Auto),
        other => anyhow::bail!("unsupported LocalSend receive policy {other:?}"),
    }
}

async fn start_localsend_v2(state: &DaemonState) -> Result<LocalSendRuntime> {
    let tls_identity = TlsIdentityVault::new(paths::config_dir())
        .load_or_generate(&state.alias)
        .context("failed to load or generate LocalSend v2 TLS identity")?;
    let device_info = device_info(&state.alias, state.localsend_port, &tls_identity);
    let server = LocalSendServer::bind(LocalSendServerConfig {
        bind_addr: SocketAddr::from((Ipv4Addr::UNSPECIFIED, state.localsend_port)),
        info: device_info.clone(),
        inbox_dir: state.inbox_dir.clone(),
        session_ttl: LOCALSEND_SESSION_TTL,
        receive_policy: state.localsend_receive_policy,
        trusted_fingerprints: state.trusted_localsend_fingerprints.clone(),
        trusted_fingerprints_file: state.trusted_localsend_fingerprints_file.clone(),
        trust_db_path: Some(state.trust_db_path.clone()),
        tls_identity: Some(tls_identity),
    })
    .await?;
    let bound_addr = server.local_addr();
    let (peer_tx, peer_rx) = tokio::sync::mpsc::unbounded_channel();
    let announcer = DiscoveryAnnouncer::new(device_info).with_peer_tx(peer_tx);
    let peer_recorder_task =
        tokio::spawn(record_localsend_lan_peers(peer_rx, state.trust_db_path.clone()));
    let (shutdown_tx, server_shutdown_rx) = tokio::sync::watch::channel(false);
    let announcer_shutdown_rx = shutdown_tx.subscribe();

    let server_task = tokio::spawn(async move {
        server
            .serve_until_shutdown(async move {
                wait_for_shutdown_signal(server_shutdown_rx).await;
            })
            .await
            .map_err(Into::into)
    });

    let announcer_task = tokio::spawn(async move {
        tokio::select! {
            result = announcer.run() => result.map_err(Into::into),
            _ = wait_for_shutdown_signal(announcer_shutdown_rx) => Ok(()),
        }
    });

    info!(
        alias = %state.alias,
        inbox = %state.inbox_dir.display(),
        addr = %bound_addr,
        "LocalSend v2 receiver started"
    );

    Ok(LocalSendRuntime { shutdown_tx, server_task, announcer_task, peer_recorder_task })
}

async fn stop_localsend_v2(runtime: LocalSendRuntime) -> Result<()> {
    let _ = runtime.shutdown_tx.send(true);
    let server_result = runtime.server_task.await.context("LocalSend v2 server task panicked")?;
    let announcer_result =
        runtime.announcer_task.await.context("LocalSend v2 announcer task panicked")?;
    runtime.peer_recorder_task.abort();

    server_result?;
    announcer_result?;
    Ok(())
}

async fn record_localsend_lan_peers(
    mut peer_rx: tokio::sync::mpsc::UnboundedReceiver<LanPeer>,
    trust_db_path: PathBuf,
) {
    while let Some(peer) = peer_rx.recv().await {
        if let Err(error) = record_localsend_lan_peer(&trust_db_path, peer) {
            tracing::warn!(%error, "failed to record discovered LocalSend LAN peer");
        }
    }
}

fn record_localsend_lan_peer(trust_db_path: &Path, peer: LanPeer) -> Result<()> {
    let store = TrustStore::open(trust_db_path)?;
    store.record_localsend_lan_peer(LocalSendLanPeer {
        fingerprint: peer.info.fingerprint,
        alias: peer.info.alias,
        source_ip: peer.address.ip().to_string(),
        port: peer.address.port(),
        protocol: peer.info.protocol.as_str().to_string(),
        device_model: peer.info.device_model,
        device_type: peer.info.device_type,
        download: peer.info.download,
        first_seen: 0,
        last_seen: 0,
    })?;
    Ok(())
}

async fn start_native_runtime(args: &Args, state: &DaemonState) -> Result<NativeRuntime> {
    let tls_identity = NativeTlsIdentity::generate(&state.alias)
        .context("failed to generate native TLS identity")?;
    let server_config = NativeTransportConfig::default()
        .apply_to_server_config(tls_identity.quinn_server_config()?)
        .context("failed to build native QUIC server config")?;
    let endpoint = bind_native_endpoint(state.native_port, server_config)?;
    let local_addr = endpoint.local_addr().context("failed to read native listener address")?;
    let peer_info = native_peer_info(
        &state.alias,
        &state.identity,
        local_addr.port(),
        tls_identity.fingerprint_sha256_hex(),
    );
    let discovery_announcer = start_native_discovery(args, &peer_info);
    let (shutdown_tx, server_shutdown_rx) = tokio::sync::watch::channel(false);
    let server_endpoint = endpoint.clone();
    let server_alias = state.alias.clone();
    let server_pubkey = state.identity.public_bytes();
    let inbox_dir = state.inbox_dir.clone();
    let trust_db_path = state.trust_db_path.clone();
    let manifest_path = args
        .native_manifest_db
        .clone()
        .unwrap_or_else(|| paths::state_dir().join("native-manifests.db"));
    ensure_parent_dir(&manifest_path).with_context(|| {
        format!("failed to prepare native manifest db at {}", manifest_path.display())
    })?;

    let server_task = tokio::spawn(async move {
        native_accept_loop(
            server_endpoint,
            server_alias,
            server_pubkey,
            inbox_dir,
            trust_db_path,
            manifest_path,
            server_shutdown_rx,
        )
        .await
    });

    info!(
        alias = %state.alias,
        addr = %local_addr,
        discovery = !args.disable_native_discovery,
        "native LAN listener started"
    );

    Ok(NativeRuntime { local_addr, endpoint, discovery_announcer, shutdown_tx, server_task })
}

fn bind_native_endpoint(
    native_port: u16,
    server_config: quinn::ServerConfig,
) -> Result<quinn::Endpoint> {
    if native_port == 0 {
        return quinn::Endpoint::server(
            server_config,
            SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)),
        )
        .context("failed to bind native QUIC endpoint");
    }

    let bind = NativeServerBind { host: Ipv4Addr::UNSPECIFIED.to_string(), port: native_port };
    Ok(bind_server_endpoint(&bind, server_config)?.into_quinn())
}

fn native_peer_info(
    alias: &str,
    identity: &Keypair,
    quic_port: u16,
    native_certificate_fingerprint: String,
) -> NativePeerInfo {
    NativePeerInfo {
        alias: alias.to_string(),
        fingerprint: Fingerprint::from_pubkey(&identity.public_bytes()).to_string(),
        pubkey: identity.public_bytes(),
        quic_port,
        native_certificate_fingerprint: Some(native_certificate_fingerprint),
        extensions: default_extensions(),
    }
}

fn mdns_hostname(alias: &str) -> String {
    let trimmed = alias.trim();
    let base = trimmed
        .strip_suffix(".local.")
        .or_else(|| trimmed.strip_suffix(".local"))
        .unwrap_or(trimmed);
    let mut host = base
        .trim()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() || ch == '-' { ch } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if host.is_empty() {
        host = DEFAULT_ALIAS.to_string();
    }
    format!("{host}.local.")
}

fn start_native_discovery(
    args: &Args,
    peer_info: &NativePeerInfo,
) -> Option<NativeDiscoveryAnnouncer> {
    if args.disable_native_discovery {
        return None;
    }

    let advertise_ip = match local_advertise_ipv4() {
        Ok(ip) => IpAddr::V4(ip),
        Err(error) => {
            warn!(%error, "failed to determine native mDNS advertise address");
            return None;
        }
    };

    match NativeDiscoveryAnnouncer::register(
        peer_info,
        &args.alias,
        &mdns_hostname(&args.alias),
        advertise_ip,
    ) {
        Ok(announcer) => Some(announcer),
        Err(error) => {
            warn!(%error, "failed to start native mDNS discovery advertisement");
            None
        }
    }
}

fn local_advertise_ipv4() -> Result<Ipv4Addr> {
    let socket = std::net::UdpSocket::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)))
        .context("bind native advertise probe socket")?;
    socket
        .connect(SocketAddr::from((Ipv4Addr::new(8, 8, 8, 8), 80)))
        .context("resolve native advertise route")?;
    match socket.local_addr().context("read native advertise address")?.ip() {
        IpAddr::V4(ip) if !ip.is_unspecified() && !ip.is_loopback() => Ok(ip),
        IpAddr::V4(ip) => anyhow::bail!("native advertise route resolved to unusable IPv4 {ip}"),
        IpAddr::V6(ip) => anyhow::bail!("native advertise route resolved to IPv6 {ip}"),
    }
}

async fn native_accept_loop(
    endpoint: quinn::Endpoint,
    alias: String,
    pubkey: [u8; 32],
    inbox_dir: PathBuf,
    trust_db_path: PathBuf,
    manifest_path: PathBuf,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    loop {
        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    return Ok(());
                }
            }
            incoming = endpoint.accept() => {
                let Some(incoming) = incoming else {
                    return Ok(());
                };
                let alias = alias.clone();
                let inbox_dir = inbox_dir.clone();
                let trust_db_path = trust_db_path.clone();
                let manifest_path = manifest_path.clone();
                tokio::spawn(async move {
                    if let Err(error) = handle_native_connection(
                        incoming,
                        alias,
                        pubkey,
                        inbox_dir,
                        trust_db_path,
                        manifest_path,
                    ).await {
                        warn!(%error, "native connection failed");
                    }
                });
            }
        }
    }
}

async fn handle_native_connection(
    incoming: quinn::Incoming,
    alias: String,
    pubkey: [u8; 32],
    inbox_dir: PathBuf,
    trust_db_path: PathBuf,
    manifest_path: PathBuf,
) -> Result<()> {
    let connection = incoming.await.context("failed to accept native QUIC connection")?;
    let mut stream = NativeControlStream::accept(&connection)
        .await
        .context("failed to accept native control stream")?;
    let message = stream.read().await.context("failed to read native control message")?;

    let ControlMessage::Hello(hello) = message else {
        return Ok(());
    };
    let peer = NativePeerInfo {
        alias: hello.alias.clone(),
        fingerprint: Fingerprint::from_pubkey(&hello.pubkey).to_string(),
        pubkey: hello.pubkey,
        quic_port: 0,
        native_certificate_fingerprint: None,
        extensions: hello.extensions.clone(),
    };
    ensure_native_peer_trusted(&trust_db_path, &peer)?;
    let ack = HelloAck {
        protocol_version: PROTOCOL_VERSION,
        alias,
        pubkey,
        nonce: b"daemon".to_vec(),
        accepted_extensions: negotiate_extensions(&default_extensions(), &hello.extensions),
    };
    stream
        .write(&ControlMessage::HelloAck(ack))
        .await
        .context("failed to write native hello ack")?;

    let message = stream.read().await.context("failed to read native transfer request")?;
    let ControlMessage::RequestTransfer(request) = message else {
        return Ok(());
    };
    receive_native_transfer(&mut stream, inbox_dir, manifest_path, &peer, request).await?;
    let (mut send, _recv) = stream.into_inner();
    send.finish().context("failed to finish native stream")?;
    let _ = tokio::time::timeout(Duration::from_millis(250), connection.closed()).await;

    Ok(())
}

fn ensure_native_peer_trusted(trust_db_path: &Path, peer: &NativePeerInfo) -> Result<()> {
    let fingerprint = peer
        .fingerprint
        .parse::<Fingerprint>()
        .with_context(|| format!("invalid native peer fingerprint {}", peer.fingerprint))?;
    let store = TrustStore::open(trust_db_path)
        .with_context(|| format!("failed to open trust store {}", trust_db_path.display()))?;
    let Some(trusted) = store.get(&fingerprint)? else {
        anyhow::bail!("native peer is not trusted: {}", peer.fingerprint);
    };
    match trusted.policy {
        PeerPolicy::AutoAccept => Ok(()),
        PeerPolicy::Prompt => {
            anyhow::bail!("native peer requires prompt before receive: {}", peer.fingerprint)
        }
        PeerPolicy::Block => anyhow::bail!("native peer is blocked: {}", peer.fingerprint),
    }
}

async fn receive_native_transfer(
    stream: &mut NativeControlStream,
    inbox_dir: PathBuf,
    manifest_path: PathBuf,
    peer: &NativePeerInfo,
    request: RequestTransfer,
) -> Result<()> {
    let manifest = ManifestStore::open(&manifest_path)
        .with_context(|| format!("failed to open native manifest {}", manifest_path.display()))?;
    let receiver = NativeTransferReceiver::trusted(inbox_dir, manifest, peer.fingerprint.clone());
    let accepted = receiver
        .accept_transfer(peer, &request)
        .await
        .context("failed to accept native transfer")?;
    stream
        .write(&ControlMessage::Accept(accepted.clone()))
        .await
        .context("failed to write native transfer acceptance")?;

    let expected_chunks = expected_chunk_count(&request, accepted.chunk_size);
    for _ in 0..expected_chunks {
        let chunk =
            read_chunk_frame(stream.recv_mut()).await.context("failed to read native chunk")?;
        receiver
            .receive_chunk_with_hash(
                &request.transfer_id,
                &chunk.file_id,
                chunk.offset,
                &chunk.bytes,
                &chunk.blake3,
            )
            .await
            .context("failed to receive native chunk")?;
    }

    let done = match stream.read().await.context("failed to read native transfer completion")? {
        ControlMessage::Done(done) if done.transfer_id == request.transfer_id => done,
        _ => return Err(anyhow::anyhow!("native transfer did not finish with Done")),
    };
    receiver
        .finish_transfer(&request.transfer_id)
        .await
        .context("failed to publish native files")?;
    stream.write(&ControlMessage::Done(done)).await.context("failed to write native completion ack")
}

fn expected_chunk_count(request: &RequestTransfer, chunk_size: u64) -> u64 {
    request
        .files
        .iter()
        .map(|file| if file.size == 0 { 0 } else { file.size.div_ceil(chunk_size) })
        .sum()
}

async fn stop_native_runtime(runtime: NativeRuntime) -> Result<()> {
    let _ = runtime.shutdown_tx.send(true);
    runtime.endpoint.close(0_u32.into(), b"daemon shutdown");
    if let Some(announcer) = &runtime.discovery_announcer {
        if let Err(error) = announcer.shutdown() {
            warn!(%error, "failed to stop native mDNS discovery advertisement");
        }
    }
    runtime.server_task.await.context("native listener task panicked")??;
    runtime.endpoint.wait_idle().await;
    info!(addr = %runtime.local_addr, "native LAN listener stopped");
    Ok(())
}

async fn wait_for_shutdown_signal(mut shutdown_rx: tokio::sync::watch::Receiver<bool>) {
    while !*shutdown_rx.borrow_and_update() {
        if shutdown_rx.changed().await.is_err() {
            break;
        }
    }
}

fn device_info(alias: &str, port: u16, tls_identity: &TlsIdentity) -> DeviceInfo {
    DeviceInfo {
        alias: alias.to_string(),
        version: LOCALSEND_VERSION.to_string(),
        device_model: None,
        device_type: Some("server".to_string()),
        fingerprint: tls_identity.fingerprint_sha256_hex(),
        port,
        protocol: Protocol::from("https"),
        download: true,
    }
}

fn default_alias() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .ok()
        .map(|alias| alias.trim().to_string())
        .filter(|alias| !alias.is_empty())
        .unwrap_or_else(|| DEFAULT_ALIAS.to_string())
}

#[cfg(unix)]
async fn wait_for_shutdown() -> Result<()> {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigint = signal(SignalKind::interrupt()).context("failed to install SIGINT handler")?;
    let mut sigterm =
        signal(SignalKind::terminate()).context("failed to install SIGTERM handler")?;

    tokio::select! {
        _ = sigint.recv() => {}
        _ = sigterm.recv() => {}
    }

    Ok(())
}

#[cfg(not(unix))]
async fn wait_for_shutdown() -> Result<()> {
    tokio::signal::ctrl_c().await.context("failed to install Ctrl-C handler")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mdns_hostname_adds_local_suffix() {
        assert_eq!(mdns_hostname("localhost"), "localhost.local.");
    }

    #[test]
    fn mdns_hostname_preserves_existing_local_suffix() {
        assert_eq!(mdns_hostname("server.local."), "server.local.");
    }

    #[test]
    fn mdns_hostname_sanitizes_alias_fallback() {
        assert_eq!(mdns_hostname("my server"), "my-server.local.");
    }

    #[test]
    fn args_default_localsend_receive_options() {
        let args = Args::parse_from(["daemon"]);

        assert_eq!(args.localsend_port, 53317);
        assert_eq!(args.alias, default_alias());
        assert_eq!(args.inbox, paths::default_inbox());
        assert!(!args.disable_localsend_v2);
        assert_eq!(args.localsend_receive_policy, None);
        assert!(args.trusted_localsend_fingerprint.is_empty());
    }

    #[test]
    fn args_default_native_options() {
        let args = Args::parse_from(["daemon"]);

        assert_eq!(args.native_port, DEFAULT_NATIVE_PORT);
        assert!(!args.disable_native);
        assert!(!args.disable_native_discovery);
    }

    #[test]
    fn args_default_api_options() {
        let args = Args::parse_from(["daemon"]);

        assert_eq!(args.api_grpc_port, DEFAULT_API_GRPC_PORT);
        assert_eq!(args.api_http_port, DEFAULT_API_HTTP_PORT);
        assert!(!args.disable_api);
    }

    #[test]
    fn args_parse_localsend_receive_overrides() {
        let args = Args::parse_from([
            "daemon",
            "--localsend-port",
            "4444",
            "--alias",
            "workstation",
            "--inbox",
            "/tmp/lsi-inbox",
            "--localsend-receive-policy",
            "trusted",
            "--trusted-localsend-fingerprint",
            "ios-fingerprint",
            "--disable-localsend-v2",
        ]);

        assert_eq!(args.localsend_port, 4444);
        assert_eq!(args.alias, "workstation");
        assert_eq!(args.inbox, PathBuf::from("/tmp/lsi-inbox"));
        assert_eq!(args.localsend_receive_policy, Some(LocalSendReceivePolicyArg::Trusted));
        assert_eq!(args.trusted_localsend_fingerprint, vec!["ios-fingerprint"]);
        assert!(args.disable_localsend_v2);
    }

    #[test]
    fn localsend_receive_policy_prefers_arg_over_config() {
        let mut config = LocalSendConfig {
            receive_policy: "trusted".to_string(),
            trusted_fingerprints: Vec::new(),
            trusted_fingerprints_file: None,
        };

        assert_eq!(
            localsend_receive_policy(None, &config).unwrap(),
            LocalSendReceivePolicy::Trusted
        );
        assert_eq!(
            localsend_receive_policy(Some(LocalSendReceivePolicyArg::Auto), &config).unwrap(),
            LocalSendReceivePolicy::Auto
        );

        config.receive_policy = "prompt".to_string();
        assert_eq!(
            localsend_receive_policy(None, &config).unwrap(),
            LocalSendReceivePolicy::Prompt
        );
    }

    #[test]
    fn args_parse_native_overrides() {
        let args = Args::parse_from([
            "daemon",
            "--native-port",
            "4445",
            "--disable-native",
            "--disable-native-discovery",
        ]);

        assert_eq!(args.native_port, 4445);
        assert!(args.disable_native);
        assert!(args.disable_native_discovery);
    }

    #[test]
    fn args_parse_api_overrides() {
        let args = Args::parse_from([
            "daemon",
            "--api-grpc-port",
            "53555",
            "--api-http-port",
            "53556",
            "--disable-api",
        ]);

        assert_eq!(args.api_grpc_port, 53555);
        assert_eq!(args.api_http_port, 53556);
        assert!(args.disable_api);
    }

    #[test]
    fn args_parse_config_override() {
        let args = Args::parse_from(["daemon", "--config", "/tmp/lsi-config.toml"]);

        assert_eq!(args.config, Some(PathBuf::from("/tmp/lsi-config.toml")));
        assert_eq!(config_path(&args), PathBuf::from("/tmp/lsi-config.toml"));
    }

    #[test]
    fn args_parse_rendezvous_override() {
        let args = Args::parse_from(["daemon", "--rendezvous", "quic://127.0.0.1:53410"]);

        assert_eq!(args.rendezvous.as_deref(), Some("quic://127.0.0.1:53410"));
    }

    #[test]
    fn rendezvous_arg_enables_and_overrides_config() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("config.toml");
        std::fs::write(
            &path,
            "[wan]\nenabled = false\nrendezvous_url = \"quic://old.example:53410\"\n",
        )
        .unwrap();
        let args = Args::parse_from([
            "daemon",
            "--config",
            path.to_str().unwrap(),
            "--rendezvous",
            "quic://127.0.0.1:53410",
        ]);

        let config = load_daemon_config(&args).unwrap();

        assert!(config.wan.enabled);
        assert_eq!(config.wan.rendezvous_url.as_deref(), Some("quic://127.0.0.1:53410"));
    }

    #[test]
    fn missing_config_path_uses_defaults() {
        let temp = tempfile::TempDir::new().unwrap();
        let args = Args::parse_from([
            "daemon",
            "--config",
            temp.path().join("missing.toml").to_str().unwrap(),
        ]);

        let config = load_daemon_config(&args).unwrap();

        assert_eq!(config, AppConfig::default());
    }

    #[test]
    fn invalid_config_returns_clear_error() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("config.toml");
        std::fs::write(&path, "[[hooks]]\ntype = \"exec\"\ncommand = \"\"\n").unwrap();
        let args = Args::parse_from(["daemon", "--config", path.to_str().unwrap()]);

        let error = load_daemon_config(&args).unwrap_err();

        assert!(error.to_string().contains("exec command is required"), "{error}");
    }

    #[tokio::test]
    async fn native_runtime_accepts_hello_and_receives_file_on_ephemeral_port() {
        let temp = tempfile::TempDir::new().unwrap();
        let args = Args::parse_from([
            "daemon",
            "--native-port",
            "0",
            "--inbox",
            temp.path().join("inbox").to_str().unwrap(),
            "--trust-db",
            temp.path().join("trust.db").to_str().unwrap(),
            "--disable-native-discovery",
            "--disable-localsend-v2",
        ]);
        let identity = Keypair::generate();
        let fingerprint = Fingerprint::from_pubkey(&identity.public_bytes()).to_string();
        let token = lsi_core::api_token::ApiToken::new("temporary-api-token").unwrap();
        let state =
            DaemonState::from_args(&args, identity, fingerprint, token, AppConfig::default());
        TrustStore::open(&state.trust_db_path)
            .unwrap()
            .trust([9; 32], "client", PeerPolicy::AutoAccept)
            .unwrap();
        let runtime = start_native_runtime(&args, &state).await.unwrap();
        let addr = runtime.local_addr;

        assert_ne!(addr.port(), 0);

        let client_endpoint = native_test_client_endpoint();
        let target = SocketAddr::from((Ipv4Addr::LOCALHOST, addr.port()));
        let connection = client_endpoint.connect(target, "localhost").unwrap().await.unwrap();
        let (mut send, mut recv) = connection.open_bi().await.unwrap();
        let hello = lsi_protocol_native_v1::dto::Hello {
            protocol_version: lsi_protocol_native_v1::dto::PROTOCOL_VERSION,
            alias: "client".to_string(),
            pubkey: [9; 32],
            nonce: b"test-nonce".to_vec(),
            extensions: lsi_protocol_native_v1::dto::default_extensions(),
        };

        lsi_protocol_native_v1::framing::write_control(
            &mut send,
            &lsi_protocol_native_v1::framing::ControlMessage::Hello(hello),
        )
        .await
        .unwrap();
        let ack = lsi_protocol_native_v1::framing::read_control(&mut recv).await.unwrap();

        assert!(matches!(ack, lsi_protocol_native_v1::framing::ControlMessage::HelloAck(_)));

        let body = b"native hello file";
        let request = lsi_protocol_native_v1::dto::RequestTransfer {
            transfer_id: "test-transfer".to_string(),
            files: vec![lsi_protocol_native_v1::dto::FileOffer {
                file_id: "file-0".to_string(),
                file_name: "native.txt".to_string(),
                size: body.len() as u64,
                blake3: Some(lsi_protocol_native_v1::chunk::blake3_hex(body)),
            }],
        };
        lsi_protocol_native_v1::framing::write_control(
            &mut send,
            &lsi_protocol_native_v1::framing::ControlMessage::RequestTransfer(request),
        )
        .await
        .unwrap();
        let accepted = lsi_protocol_native_v1::framing::read_control(&mut recv).await.unwrap();
        assert!(matches!(accepted, lsi_protocol_native_v1::framing::ControlMessage::Accept(_)));
        lsi_protocol_native_v1::transfer::write_chunk_frame(
            &mut send,
            &lsi_protocol_native_v1::transfer::TransferChunk {
                file_id: "file-0".to_string(),
                offset: 0,
                blake3: lsi_protocol_native_v1::chunk::blake3_hex(body),
                bytes: body.to_vec(),
            },
        )
        .await
        .unwrap();
        lsi_protocol_native_v1::framing::write_control(
            &mut send,
            &lsi_protocol_native_v1::framing::ControlMessage::Done(
                lsi_protocol_native_v1::dto::DoneTransfer {
                    transfer_id: "test-transfer".to_string(),
                },
            ),
        )
        .await
        .unwrap();
        let done = lsi_protocol_native_v1::framing::read_control(&mut recv).await.unwrap();
        assert!(matches!(done, lsi_protocol_native_v1::framing::ControlMessage::Done(_)));
        send.finish().unwrap();

        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if let Ok(bytes) = tokio::fs::read(temp.path().join("inbox/native.txt")).await {
                    break bytes;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        let written = tokio::fs::read(temp.path().join("inbox/native.txt")).await.unwrap();
        assert_eq!(written, body);

        stop_native_runtime(runtime).await.unwrap();
    }

    fn native_test_client_endpoint() -> quinn::Endpoint {
        use std::sync::Arc;

        use quinn::crypto::rustls::QuicClientConfig;
        use rustls::client::danger::{
            HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
        };
        use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
        use rustls::{DigitallySignedStruct, SignatureScheme};

        #[derive(Debug)]
        struct TrustAnyServer;

        impl ServerCertVerifier for TrustAnyServer {
            fn verify_server_cert(
                &self,
                _end_entity: &CertificateDer<'_>,
                _intermediates: &[CertificateDer<'_>],
                _server_name: &ServerName<'_>,
                _ocsp_response: &[u8],
                _now: UnixTime,
            ) -> std::result::Result<ServerCertVerified, rustls::Error> {
                Ok(ServerCertVerified::assertion())
            }

            fn verify_tls12_signature(
                &self,
                _message: &[u8],
                _cert: &CertificateDer<'_>,
                _dss: &DigitallySignedStruct,
            ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
                Ok(HandshakeSignatureValid::assertion())
            }

            fn verify_tls13_signature(
                &self,
                _message: &[u8],
                _cert: &CertificateDer<'_>,
                _dss: &DigitallySignedStruct,
            ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
                Ok(HandshakeSignatureValid::assertion())
            }

            fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
                vec![
                    SignatureScheme::ECDSA_NISTP256_SHA256,
                    SignatureScheme::ED25519,
                    SignatureScheme::RSA_PSS_SHA256,
                ]
            }
        }

        lsi_protocol_native_v1::tls::ensure_ring_crypto_provider();
        let mut rustls_config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(TrustAnyServer))
            .with_no_client_auth();
        rustls_config.alpn_protocols =
            vec![lsi_protocol_native_v1::transport::NATIVE_ALPN.to_vec()];
        let quic_crypto = QuicClientConfig::try_from(rustls_config).unwrap();
        let client_config = quinn::ClientConfig::new(Arc::new(quic_crypto));
        let mut endpoint = quinn::Endpoint::client(SocketAddr::from(([127, 0, 0, 1], 0))).unwrap();
        endpoint.set_default_client_config(client_config);
        endpoint
    }
}
