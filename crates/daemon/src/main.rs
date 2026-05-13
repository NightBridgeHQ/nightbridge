use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Parser;
use lsi_core::{
    identity::{Fingerprint, FsVault, IdentityVault, Keypair},
    paths,
    trust::TrustStore,
};
use tracing::{info, warn};
use tracing_subscriber::{fmt, EnvFilter};

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
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let args = Args::parse();
    let identity_path = args.identity.unwrap_or_else(paths::identity_file);
    let trust_db_path = args.trust_db.unwrap_or_else(paths::trust_db_file);

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
        "daemon idle"
    );

    wait_for_shutdown().await?;

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
