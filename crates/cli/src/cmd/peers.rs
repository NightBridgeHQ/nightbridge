//! `localsend-improved peers ...` subcommands.

use anyhow::{Context, Result};
use clap::Subcommand;
use lsi_core::paths;
use lsi_core::trust::{PeerPolicy, TrustStore};

#[derive(Subcommand)]
pub enum Cmd {
    /// List all trusted peers.
    List,
}

pub fn run(command: Cmd) -> Result<()> {
    let trust_db_path = paths::trust_db_file();
    if let Some(parent) = trust_db_path.parent() {
        std::fs::create_dir_all(parent).context("creating state dir")?;
    }

    let store = TrustStore::open(&trust_db_path).context("opening trust store")?;
    match command {
        Cmd::List => list(&store),
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
