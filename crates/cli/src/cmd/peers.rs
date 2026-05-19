//! `night-bridge peers ...` subcommands.

use crate::daemon_client::{self, DaemonClientConfig};
use anyhow::{bail, Context, Result};
use clap::Subcommand;
use lsi_core::identity::{Fingerprint, FsVault, IdentityVault, Keypair};
use lsi_core::paths;
use lsi_core::trust::TrustStore;
use lsi_proto::peers::v1::{
    LanPeer, LocalSendPeer, LocalSendPeerStatus, PeerPolicy, PeerProtocol, TrustedPeer,
};
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
    /// List NightBridge native peers discovered on the LAN.
    ListNative {
        /// Maximum discovery wait time in milliseconds.
        #[arg(long, default_value_t = 1500)]
        timeout_ms: u64,
    },
    /// Trust a discovered NightBridge native LAN peer for mesh transfers.
    ApproveNative {
        /// Native peer alias or fingerprint.
        peer: String,
        /// Optional admin label.
        #[arg(long)]
        label: Option<String>,
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
    /// List official LocalSend peers waiting for receive approval.
    PendingLocalSend,
    /// Approve an official LocalSend fingerprint for future receives.
    ApproveLocalSend {
        /// LocalSend fingerprint to approve.
        fingerprint: String,
        /// Optional admin label.
        #[arg(long)]
        label: Option<String>,
    },
    /// Deny an official LocalSend fingerprint.
    DenyLocalSend {
        /// LocalSend fingerprint to deny.
        fingerprint: String,
    },
}

pub fn run(command: Cmd, daemon_config: &DaemonClientConfig) -> Result<()> {
    match command {
        Cmd::ListLan { timeout_ms } => return list_lan(timeout_ms, daemon_config),
        Cmd::ListNative { timeout_ms } => return list_native(timeout_ms, daemon_config),
        Cmd::ApproveNative { peer, label, timeout_ms } => {
            return approve_native(daemon_config, peer, label, timeout_ms);
        }
        Cmd::LookupWan { target, rendezvous } => return lookup_wan(&target, &rendezvous),
        Cmd::List
        | Cmd::PendingLocalSend
        | Cmd::ApproveLocalSend { .. }
        | Cmd::DenyLocalSend { .. } => {}
    }

    match command {
        Cmd::List => list(daemon_config),
        Cmd::PendingLocalSend => list_pending_localsend(daemon_config),
        Cmd::ApproveLocalSend { fingerprint, label } => {
            approve_localsend(daemon_config, fingerprint, label)
        }
        Cmd::DenyLocalSend { fingerprint } => deny_localsend(daemon_config, fingerprint),
        Cmd::ListLan { .. } => unreachable!("list-lan is handled before connecting to daemon"),
        Cmd::ListNative { .. } => {
            unreachable!("list-native is handled before connecting to daemon")
        }
        Cmd::ApproveNative { .. } => {
            unreachable!("approve-native is handled before connecting to daemon")
        }
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

fn list_pending_localsend(config: &DaemonClientConfig) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("creating async runtime")?;
    let peers = runtime.block_on(daemon_client::list_pending_localsend_peers(config))?;
    if peers.is_empty() {
        println!("no pending LocalSend peers");
        return Ok(());
    }

    println!("{:<66} {:<24} {:<8} {:<8} LAST SEEN", "FINGERPRINT", "ALIAS", "STATUS", "ATTEMPTS");
    for peer in peers {
        print_localsend_peer(peer);
    }
    Ok(())
}

fn approve_localsend(
    config: &DaemonClientConfig,
    fingerprint: String,
    label: Option<String>,
) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("creating async runtime")?;
    let peer =
        runtime.block_on(daemon_client::approve_localsend_peer(config, fingerprint, label))?;
    println!("approved LocalSend peer:");
    print_localsend_peer(peer);
    Ok(())
}

fn deny_localsend(config: &DaemonClientConfig, fingerprint: String) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("creating async runtime")?;
    let peer = runtime.block_on(daemon_client::deny_localsend_peer(config, fingerprint))?;
    println!("denied LocalSend peer:");
    print_localsend_peer(peer);
    Ok(())
}

fn print_localsend_peer(peer: LocalSendPeer) {
    let status = match LocalSendPeerStatus::try_from(peer.status)
        .unwrap_or(LocalSendPeerStatus::LocalsendPeerStatusUnspecified)
    {
        LocalSendPeerStatus::LocalsendPeerStatusPending => "pending",
        LocalSendPeerStatus::LocalsendPeerStatusTrusted => "trusted",
        LocalSendPeerStatus::LocalsendPeerStatusBlocked => "blocked",
        LocalSendPeerStatus::LocalsendPeerStatusUnspecified => "unknown",
    };
    println!(
        "{:<66} {:<24} {:<8} {:<8} {}",
        peer.fingerprint, peer.alias, status, peer.attempt_count, peer.last_seen_unix_seconds
    );
}

fn list_lan(timeout_ms: u64, config: &DaemonClientConfig) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("creating async runtime")?;

    let daemon_peers = runtime.block_on(daemon_client::list_lan_peers(
        config,
        u32::try_from(timeout_ms).unwrap_or(u32::MAX),
    ));
    if let Ok(peers) = daemon_peers {
        if peers.is_empty() {
            println!("no LocalSend peers found");
        } else {
            print_lan_peers(peers);
        }
        return Ok(());
    }

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

fn list_native(timeout_ms: u64, config: &DaemonClientConfig) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("creating async runtime")?;
    let peers = runtime.block_on(daemon_client::list_native_lan_peers(
        config,
        u32::try_from(timeout_ms).unwrap_or(u32::MAX),
    ))?;
    if peers.is_empty() {
        println!("no native NightBridge peers found");
    } else {
        print_lan_peers(peers);
    }
    Ok(())
}

fn approve_native(
    config: &DaemonClientConfig,
    peer: String,
    label: Option<String>,
    timeout_ms: u64,
) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("creating async runtime")?;
    let peer = runtime.block_on(daemon_client::trust_native_lan_peer(
        config,
        peer,
        label,
        u32::try_from(timeout_ms).unwrap_or(u32::MAX),
    ))?;
    println!("approved native peer:");
    print_trusted_peer(peer);
    Ok(())
}

fn print_lan_peers(peers: Vec<LanPeer>) {
    println!("{:<21} {:<22} {:<8} FINGERPRINT", "ALIAS", "ADDRESS", "PROTOCOL");
    for peer in peers {
        let protocol =
            match PeerProtocol::try_from(peer.protocol).unwrap_or(PeerProtocol::Unspecified) {
                PeerProtocol::LocalsendV2 => "localsend_v2",
                PeerProtocol::NativeV1 => "native_v1",
                PeerProtocol::Unspecified => "unknown",
            };
        let fingerprint = peer.fingerprint.map(|value| value.value).unwrap_or_else(|| "-".into());
        println!(
            "{:<21} {:<22} {:<8} {}",
            peer.alias,
            format!("{}:{}", peer.address, peer.port),
            protocol,
            fingerprint
        );
    }
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
