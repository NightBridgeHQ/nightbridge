use std::net::{Ipv4Addr, SocketAddr};
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
use tokio::task::JoinHandle;
use tracing::{info, warn};
use tracing_subscriber::{fmt, EnvFilter};

const DEFAULT_LOCALSEND_PORT: u16 = 53317;
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
}

#[derive(Debug)]
struct LocalSendRuntime {
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    server_task: JoinHandle<Result<()>>,
    announcer_task: JoinHandle<Result<()>>,
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

    wait_for_shutdown().await?;

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
}
