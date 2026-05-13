//! `localsend-improved peers ...` subcommands.

use anyhow::{Context, Result};
use clap::Subcommand;
use lsi_core::paths;
use lsi_core::trust::{PeerPolicy, TrustStore};
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

pub fn run(command: Cmd) -> Result<()> {
    if let Cmd::ListLan { timeout_ms } = command {
        return list_lan(timeout_ms);
    }

    let trust_db_path = paths::trust_db_file();
    if let Some(parent) = trust_db_path.parent() {
        std::fs::create_dir_all(parent).context("creating state dir")?;
    }

    let store = TrustStore::open(&trust_db_path).context("opening trust store")?;
    match command {
        Cmd::List => list(&store),
        Cmd::ListLan { .. } => unreachable!("list-lan is handled before opening trust store"),
    }
}

fn list(store: &TrustStore) -> Result<()> {
    let peers = store.list()?;
    if peers.is_empty() {
        println!("no trusted peers (run `pair` to add some; Sprint 2)");
        return Ok(());
    }

    println!("{:<22} {:<24} {:<11} {:<19}", "FINGERPRINT", "LABEL", "POLICY", "LAST SEEN");
    for peer in peers {
        let policy = match peer.policy {
            PeerPolicy::AutoAccept => "auto_accept",
            PeerPolicy::Prompt => "prompt",
            PeerPolicy::Block => "block",
        };
        let last_seen =
            peer.last_seen.map(|timestamp| format!("{timestamp}")).unwrap_or("-".into());
        println!("{:<22} {:<24} {:<11} {:<19}", peer.fingerprint, peer.label, policy, last_seen);
    }
    Ok(())
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
