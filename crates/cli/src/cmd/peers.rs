//! `localsend-improved peers ...` subcommands.

use crate::daemon_client::{self, DaemonClientConfig};
use anyhow::{Context, Result};
use clap::Subcommand;
use lsi_proto::peers::v1::{PeerPolicy, TrustedPeer};
use lsi_protocol_localsend_v2::discovery::DiscoveryBrowser;
use std::time::Duration;

#[derive(Subcommand)]
pub enum Cmd {
    /// List all trusted peers.
    List,
    /// List LocalSend peers discovered on the LAN.
    ListLan {
        /// Maximum discovery wait time in milliseconds.
        #[arg(long, default_value_t = 1500)]
        timeout_ms: u64,
    },
}

pub fn run(command: Cmd, daemon_config: &DaemonClientConfig) -> Result<()> {
    if let Cmd::ListLan { timeout_ms } = command {
        return list_lan(timeout_ms);
    }

    match command {
        Cmd::List => list(daemon_config),
        Cmd::ListLan { .. } => unreachable!("list-lan is handled before connecting to daemon"),
    }
}

fn list(config: &DaemonClientConfig) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("creating async runtime")?;
    let peers = runtime.block_on(daemon_client::list_trusted_peers(config))?;
    if peers.is_empty() {
        println!("no trusted peers (run `pair` to add some; Sprint 2)");
        return Ok(());
    }

    println!("{:<22} {:<24} {:<11} {:<19}", "FINGERPRINT", "LABEL", "POLICY", "LAST SEEN");
    for peer in peers {
        print_trusted_peer(peer);
    }
    Ok(())
}

fn print_trusted_peer(peer: TrustedPeer) {
    let fingerprint = peer.fingerprint.map(|value| value.value).unwrap_or_else(|| "-".into());
    let policy = match PeerPolicy::try_from(peer.policy).unwrap_or(PeerPolicy::Unspecified) {
        PeerPolicy::AutoAccept => "auto_accept",
        PeerPolicy::Prompt => "prompt",
        PeerPolicy::Block => "block",
        PeerPolicy::Unspecified => "unspecified",
    };
    let last_seen =
        peer.last_seen_unix_seconds.map(|timestamp| format!("{timestamp}")).unwrap_or("-".into());
    println!("{:<22} {:<24} {:<11} {:<19}", fingerprint, peer.label, policy, last_seen);
}

fn list_lan(timeout_ms: u64) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("creating async runtime")?;
    let browser = DiscoveryBrowser::new("cli");
    let peer = runtime
        .block_on(browser.with_timeout(Duration::from_millis(timeout_ms)).listen_once())
        .context("discovering LocalSend LAN peers")?;

    let Some(peer) = peer else {
        println!("no LocalSend peers found");
        return Ok(());
    };

    println!("{:<21} {:<22} {:<8} FINGERPRINT", "ALIAS", "ADDRESS", "PROTOCOL");
    println!(
        "{:<21} {:<22} {:<8} {}",
        peer.info.alias,
        peer.address,
        peer.info.protocol.as_str(),
        peer.info.fingerprint
    );
    Ok(())
}
