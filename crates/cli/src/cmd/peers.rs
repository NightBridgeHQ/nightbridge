//! `night-bridge peers ...` subcommands.

use crate::daemon_client::{self, DaemonClientConfig};
use anyhow::{bail, Context, Result};
use clap::Subcommand;
use lsi_core::identity::{Fingerprint, FsVault, IdentityVault, Keypair};
use lsi_core::paths;
use lsi_core::trust::TrustStore;
use lsi_proto::peers::v1::{PeerPolicy, TrustedPeer};
use lsi_protocol_localsend_v2::discovery::DiscoveryBrowser;
use lsi_rendezvous::client::RendezvousClient;
use lsi_rendezvous::protocol::{CandidateKind, LookupRequest};
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
    /// Lookup one trusted peer's WAN candidates through rendezvous.
    LookupWan {
        /// Target peer public key hex, or a trusted peer fingerprint.
        target: String,
        /// WAN rendezvous server URL, such as quic://host:53410.
        #[arg(long)]
        rendezvous: String,
    },
}

pub fn run(command: Cmd, daemon_config: &DaemonClientConfig) -> Result<()> {
    match command {
        Cmd::ListLan { timeout_ms } => return list_lan(timeout_ms),
        Cmd::LookupWan { target, rendezvous } => return lookup_wan(&target, &rendezvous),
        Cmd::List => {}
    }

    match command {
        Cmd::List => list(daemon_config),
        Cmd::ListLan { .. } => unreachable!("list-lan is handled before connecting to daemon"),
        Cmd::LookupWan { .. } => unreachable!("lookup-wan is handled before connecting to daemon"),
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

fn lookup_wan(target: &str, rendezvous_url: &str) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("creating async runtime")?;
    let identity = load_or_create_identity()?;
    let target_pubkey = resolve_target_pubkey(target)?;
    let response = runtime.block_on(async {
        let client = RendezvousClient::connect(rendezvous_url).await?;
        let response = client
            .lookup(LookupRequest { requester_pubkey: identity.public_bytes(), target_pubkey })
            .await;
        client.close().await;
        response
    })?;
    let peer = response.peer.context("WAN peer is not registered with rendezvous")?;

    println!("alias: {}", peer.alias);
    println!("{:<18} {:<22} PRIORITY", "KIND", "ADDRESS");
    for candidate in peer.candidates {
        println!(
            "{:<18} {:<22} {}",
            candidate_kind_label(&candidate.kind),
            candidate.address,
            candidate.priority
        );
    }
    Ok(())
}

fn candidate_kind_label(kind: &CandidateKind) -> &'static str {
    match kind {
        CandidateKind::Local => "local",
        CandidateKind::ServerReflexive => "server_reflexive",
    }
}

fn load_or_create_identity() -> Result<Keypair> {
    let vault = FsVault::new(paths::identity_file());
    if let Some(keypair) = vault.load()? {
        return Ok(keypair);
    }
    let keypair = Keypair::generate();
    vault.save(&keypair).context("saving fresh identity")?;
    Ok(keypair)
}

fn resolve_target_pubkey(target: &str) -> Result<[u8; 32]> {
    if target.chars().filter(|char| *char != '-').count() == 64 {
        return decode_pubkey_hex(target);
    }

    let fingerprint: Fingerprint = target
        .parse()
        .with_context(|| "target must be a 64-character pubkey hex or trusted peer fingerprint")?;
    let trust_db_path = paths::trust_db_file();
    let trust_store = TrustStore::open(&trust_db_path)
        .with_context(|| format!("opening trust store at {}", trust_db_path.display()))?;
    let peer = trust_store
        .get(&fingerprint)?
        .with_context(|| format!("trusted peer not found for fingerprint {fingerprint}"))?;
    Ok(peer.pubkey)
}

fn decode_pubkey_hex(value: &str) -> Result<[u8; 32]> {
    let cleaned: String = value.chars().filter(|char| *char != '-').collect();
    if cleaned.len() != 64 {
        bail!("pubkey hex must be 64 hex characters");
    }

    let mut pubkey = [0_u8; 32];
    for (index, byte) in pubkey.iter_mut().enumerate() {
        let start = index * 2;
        let end = start + 2;
        *byte = u8::from_str_radix(&cleaned[start..end], 16)
            .with_context(|| "pubkey hex contains non-hex characters")?;
    }
    Ok(pubkey)
}
