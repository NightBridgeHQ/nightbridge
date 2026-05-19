use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Duration;

use anyhow::{Context, Result};
use lsi_core::trust::{Peer, PeerPolicy, TrustStore};
use lsi_protocol_native_v1::discovery::{NativeDiscoveryBrowser, ResolvedNativePeer};
use lsi_protocol_native_v1::pairing::validate_peer_info_fingerprint;

#[derive(Debug, Clone)]
pub(crate) struct NativeLanPeer {
    pub(crate) alias: String,
    pub(crate) address: IpAddr,
    pub(crate) port: u16,
    pub(crate) fingerprint: String,
    pub(crate) pubkey: [u8; 32],
    pub(crate) native_certificate_fingerprint: Option<String>,
    pub(crate) extensions: Vec<String>,
}

pub(crate) async fn discover(
    timeout: Duration,
    local_fingerprint: &str,
) -> Result<Vec<NativeLanPeer>> {
    let local_fingerprint = normalize_peer_ref(local_fingerprint);
    tokio::task::spawn_blocking(move || {
        let browser = NativeDiscoveryBrowser::browse().context("starting native LAN discovery")?;
        let resolved =
            browser.recv_resolved_all(timeout).context("discovering native LAN peers")?;
        let _ = browser.shutdown();

        let mut peers = HashMap::<String, NativeLanPeer>::new();
        for peer in resolved.into_iter().filter_map(map_resolved_peer) {
            if normalize_peer_ref(&peer.fingerprint) == local_fingerprint {
                continue;
            }
            peers.insert(peer.fingerprint.clone(), peer);
        }
        Ok(peers.into_values().collect())
    })
    .await
    .context("native LAN discovery task panicked")?
}

pub(crate) fn trust_discovered_peer(
    trust_db_path: &std::path::Path,
    peer: &NativeLanPeer,
    label: Option<&str>,
    policy: PeerPolicy,
) -> Result<Peer> {
    let cert = peer.native_certificate_fingerprint.as_deref().ok_or_else(|| {
        anyhow::anyhow!(
            "native peer {} did not advertise certificate metadata; restart it with alpha native discovery",
            peer.alias
        )
    })?;
    let label = label.unwrap_or(&peer.alias);
    let store = TrustStore::open(trust_db_path)
        .with_context(|| format!("failed to open trust store {}", trust_db_path.display()))?;
    store
        .trust_with_native_certificate(peer.pubkey, label, policy, cert)
        .context("trusting native LAN peer")
}

pub(crate) fn resolve_peer_ref(peers: Vec<NativeLanPeer>, peer_ref: &str) -> Result<NativeLanPeer> {
    let needle = normalize_peer_ref(peer_ref);
    if needle.is_empty() {
        anyhow::bail!("native peer is required");
    }

    let mut matches = peers
        .into_iter()
        .filter(|peer| {
            normalize_peer_ref(&peer.fingerprint) == needle
                || peer.alias.eq_ignore_ascii_case(peer_ref)
        })
        .collect::<Vec<_>>();

    match matches.len() {
        0 => anyhow::bail!(
            "native LAN peer {peer_ref:?} was not discovered; keep the daemon running and run `night-bridge peers list-native`"
        ),
        1 => Ok(matches.remove(0)),
        _ => anyhow::bail!("native LAN peer {peer_ref:?} is ambiguous; use its fingerprint"),
    }
}

pub(crate) fn normalize_peer_ref(value: &str) -> String {
    value.chars().filter(|ch| *ch != ':' && *ch != '-').flat_map(char::to_lowercase).collect()
}

fn map_resolved_peer(peer: ResolvedNativePeer) -> Option<NativeLanPeer> {
    let info = peer.info;
    if info.quic_port == 0 || validate_peer_info_fingerprint(&info).is_err() {
        return None;
    }
    Some(NativeLanPeer {
        alias: info.alias,
        address: peer.address,
        port: info.quic_port,
        fingerprint: info.fingerprint,
        pubkey: info.pubkey,
        native_certificate_fingerprint: info.native_certificate_fingerprint,
        extensions: info.extensions,
    })
}
