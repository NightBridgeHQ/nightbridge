use std::net::SocketAddr;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use lsi_rendezvous::server::{RendezvousServer, RendezvousServerConfig};
use tracing::info;
use tracing_subscriber::{fmt, EnvFilter};

/// Self-hosted WAN rendezvous server for NightBridge.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// UDP address to bind for QUIC rendezvous traffic.
    #[arg(long = "bind", default_value = "0.0.0.0:53410")]
    bind: SocketAddr,

    /// Maximum registration TTL accepted from clients.
    #[arg(long = "max-ttl-seconds", default_value_t = 300)]
    max_ttl_seconds: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let args = Args::parse();
    let server = RendezvousServer::bind(RendezvousServerConfig {
        bind_addr: args.bind,
        max_ttl: Duration::from_secs(args.max_ttl_seconds),
        prune_interval: Duration::from_secs(30),
    })
    .await
    .context("start rendezvous server")?;

    info!(addr = %server.local_addr(), "rendezvous server ready");
    wait_for_shutdown().await?;
    server.stop().await?;
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).with_writer(std::io::stderr).init();
}

#[cfg(unix)]
async fn wait_for_shutdown() -> Result<()> {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigint = signal(SignalKind::interrupt()).context("install SIGINT handler")?;
    let mut sigterm = signal(SignalKind::terminate()).context("install SIGTERM handler")?;

    tokio::select! {
        _ = sigint.recv() => {}
        _ = sigterm.recv() => {}
    }

    Ok(())
}

#[cfg(not(unix))]
async fn wait_for_shutdown() -> Result<()> {
    tokio::signal::ctrl_c().await.context("install Ctrl-C handler")
}
