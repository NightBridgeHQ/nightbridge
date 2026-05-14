use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use lsi_core::{
    identity::{Fingerprint, FsVault, IdentityVault, Keypair},
    paths,
    trust::TrustStore,
};
use lsi_protocol_localsend_v2::{
    discovery::DiscoveryAnnouncer,
    dto::{DeviceInfo, Protocol},
    server::{LocalSendServer, LocalSendServerConfig},
    tls::{TlsIdentity, TlsIdentityVault},
};
use lsi_protocol_native_v1::{
    discovery::NativeDiscoveryAnnouncer,
    dto::{default_extensions, negotiate_extensions, HelloAck, NativePeerInfo, PROTOCOL_VERSION},
    framing::ControlMessage,
    tls::NativeTlsIdentity,
    transport::{
        bind_server_endpoint, NativeControlStream, NativeServerBind, NativeTransportConfig,
    },
};
use tokio::task::JoinHandle;
use tracing::{info, warn};
use tracing_subscriber::{fmt, EnvFilter};

const DEFAULT_LOCALSEND_PORT: u16 = 53317;
const DEFAULT_NATIVE_PORT: u16 = 53400;
const DEFAULT_ALIAS: &str = "localsend-improved";
const LOCALSEND_VERSION: &str = "2.0";
const LOCALSEND_SESSION_TTL: Duration = Duration::from_secs(60 * 60);

/// Headless daemon for LocalSend Improved.
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

    /// UDP port for the native QUIC listener.
    #[arg(long = "native-port", default_value_t = DEFAULT_NATIVE_PORT)]
    native_port: u16,

    /// Disable the native QUIC listener.
    #[arg(long = "disable-native")]
    disable_native: bool,

    /// Disable native mDNS discovery advertisement.
    #[arg(long = "disable-native-discovery")]
    disable_native_discovery: bool,
}

#[derive(Debug)]
struct LocalSendRuntime {
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    server_task: JoinHandle<Result<()>>,
    announcer_task: JoinHandle<Result<()>>,
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
    init_tracing();

    let args = Args::parse();
    let identity_path = args.identity.clone().unwrap_or_else(paths::identity_file);
    let trust_db_path = args.trust_db.clone().unwrap_or_else(paths::trust_db_file);

    let vault = FsVault::new(&identity_path);
    let identity = load_or_create_identity(&vault).with_context(|| {
        format!("failed to load or create identity at {}", identity_path.display())
    })?;
    let fingerprint = Fingerprint::from_pubkey(&identity.public_bytes());

    ensure_parent_dir(&trust_db_path)
        .with_context(|| format!("failed to prepare trust store at {}", trust_db_path.display()))?;
    let _trust_store = TrustStore::open(&trust_db_path)
        .with_context(|| format!("failed to open trust store at {}", trust_db_path.display()))?;

    info!(fp = %fingerprint, "identity loaded");
    info!(path = %trust_db_path.display(), "trust store opened");
    info!(
        identity = %identity_path.display(),
        trust_db = %trust_db_path.display(),
        "daemon initialized"
    );

    let localsend_runtime = if args.disable_localsend_v2 {
        info!("LocalSend v2 receiver disabled");
        None
    } else {
        Some(
            start_localsend_v2(&args)
                .await
                .with_context(|| "failed to start LocalSend v2 receiver")?,
        )
    };

    let native_runtime = if args.disable_native {
        info!("native LAN listener disabled");
        None
    } else {
        Some(
            start_native_runtime(&args, &identity)
                .await
                .with_context(|| "failed to start native LAN listener")?,
        )
    };

    wait_for_shutdown().await?;

    if let Some(runtime) = native_runtime {
        stop_native_runtime(runtime).await?;
    }

    if let Some(runtime) = localsend_runtime {
        stop_localsend_v2(runtime).await?;
    }

    info!("shutdown");
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().json().with_env_filter(filter).with_writer(std::io::stderr).init();
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

async fn start_localsend_v2(args: &Args) -> Result<LocalSendRuntime> {
    let tls_identity = TlsIdentityVault::new(paths::config_dir())
        .load_or_generate(&args.alias)
        .context("failed to load or generate LocalSend v2 TLS identity")?;
    let device_info = device_info(&args.alias, args.localsend_port, &tls_identity);
    let server = LocalSendServer::bind(LocalSendServerConfig {
        bind_addr: SocketAddr::from((Ipv4Addr::UNSPECIFIED, args.localsend_port)),
        info: device_info.clone(),
        inbox_dir: args.inbox.clone(),
        session_ttl: LOCALSEND_SESSION_TTL,
        tls_identity: Some(tls_identity),
    })
    .await?;
    let bound_addr = server.local_addr();
    let announcer = DiscoveryAnnouncer::new(device_info);
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
        alias = %args.alias,
        inbox = %args.inbox.display(),
        addr = %bound_addr,
        "LocalSend v2 receiver started"
    );

    Ok(LocalSendRuntime { shutdown_tx, server_task, announcer_task })
}

async fn stop_localsend_v2(runtime: LocalSendRuntime) -> Result<()> {
    let _ = runtime.shutdown_tx.send(true);
    let server_result = runtime.server_task.await.context("LocalSend v2 server task panicked")?;
    let announcer_result =
        runtime.announcer_task.await.context("LocalSend v2 announcer task panicked")?;

    server_result?;
    announcer_result?;
    Ok(())
}

async fn start_native_runtime(args: &Args, identity: &Keypair) -> Result<NativeRuntime> {
    let tls_identity = NativeTlsIdentity::generate(&args.alias)
        .context("failed to generate native TLS identity")?;
    let server_config = NativeTransportConfig::default()
        .apply_to_server_config(tls_identity.quinn_server_config()?)
        .context("failed to build native QUIC server config")?;
    let endpoint = bind_native_endpoint(args.native_port, server_config)?;
    let local_addr = endpoint.local_addr().context("failed to read native listener address")?;
    let peer_info = native_peer_info(&args.alias, identity, local_addr.port());
    let discovery_announcer = start_native_discovery(args, &peer_info);
    let (shutdown_tx, server_shutdown_rx) = tokio::sync::watch::channel(false);
    let server_endpoint = endpoint.clone();
    let server_alias = args.alias.clone();
    let server_pubkey = identity.public_bytes();

    let server_task = tokio::spawn(async move {
        native_accept_loop(server_endpoint, server_alias, server_pubkey, server_shutdown_rx).await
    });

    info!(
        alias = %args.alias,
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

fn native_peer_info(alias: &str, identity: &Keypair, quic_port: u16) -> NativePeerInfo {
    NativePeerInfo {
        alias: alias.to_string(),
        fingerprint: Fingerprint::from_pubkey(&identity.public_bytes()).to_string(),
        pubkey: identity.public_bytes(),
        quic_port,
        extensions: default_extensions(),
    }
}

fn start_native_discovery(
    args: &Args,
    peer_info: &NativePeerInfo,
) -> Option<NativeDiscoveryAnnouncer> {
    if args.disable_native_discovery {
        return None;
    }

    match NativeDiscoveryAnnouncer::register(
        peer_info,
        &args.alias,
        "localhost",
        IpAddr::V4(Ipv4Addr::UNSPECIFIED),
    ) {
        Ok(announcer) => Some(announcer),
        Err(error) => {
            warn!(%error, "failed to start native mDNS discovery advertisement");
            None
        }
    }
}

async fn native_accept_loop(
    endpoint: quinn::Endpoint,
    alias: String,
    pubkey: [u8; 32],
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
                tokio::spawn(async move {
                    if let Err(error) = handle_native_connection(incoming, alias, pubkey).await {
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
) -> Result<()> {
    let connection = incoming.await.context("failed to accept native QUIC connection")?;
    let mut stream = NativeControlStream::accept(&connection)
        .await
        .context("failed to accept native control stream")?;
    let message = stream.read().await.context("failed to read native control message")?;

    if let ControlMessage::Hello(hello) = message {
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
        let (mut send, _recv) = stream.into_inner();
        send.finish().context("failed to finish native hello ack stream")?;
        let _ = tokio::time::timeout(Duration::from_millis(250), connection.closed()).await;
    }

    Ok(())
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
    fn args_default_localsend_receive_options() {
        let args = Args::parse_from(["daemon"]);

        assert_eq!(args.localsend_port, 53317);
        assert_eq!(args.alias, default_alias());
        assert_eq!(args.inbox, paths::default_inbox());
        assert!(!args.disable_localsend_v2);
    }

    #[test]
    fn args_default_native_options() {
        let args = Args::parse_from(["daemon"]);

        assert_eq!(args.native_port, DEFAULT_NATIVE_PORT);
        assert!(!args.disable_native);
        assert!(!args.disable_native_discovery);
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
            "--disable-localsend-v2",
        ]);

        assert_eq!(args.localsend_port, 4444);
        assert_eq!(args.alias, "workstation");
        assert_eq!(args.inbox, PathBuf::from("/tmp/lsi-inbox"));
        assert!(args.disable_localsend_v2);
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

    #[tokio::test]
    async fn native_runtime_accepts_hello_on_ephemeral_port() {
        let args = Args::parse_from([
            "daemon",
            "--native-port",
            "0",
            "--disable-native-discovery",
            "--disable-localsend-v2",
        ]);
        let identity = Keypair::generate();
        let runtime = start_native_runtime(&args, &identity).await.unwrap();
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
