use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use lsi_core::trust::{LocalSendLanPeer, TrustStore};
use serde::Deserialize;
use tokio::task::JoinSet;

const LOCALSEND_PORT: u16 = 53317;

pub(crate) async fn scan_and_cache(trust_db_path: &Path) -> Result<Vec<LocalSendLanPeer>> {
    let local_ip = local_ipv4().context("failed to determine local IPv4 address")?;
    let octets = local_ip.octets();
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_millis(350))
        .build()
        .context("creating LocalSend discovery client")?;
    let mut tasks = JoinSet::new();

    for host in 1..=254_u8 {
        let candidate = Ipv4Addr::new(octets[0], octets[1], octets[2], host);
        if candidate == local_ip {
            continue;
        }
        let client = client.clone();
        tasks.spawn(async move { probe_candidate(client, candidate).await });
    }

    let store = TrustStore::open(trust_db_path)?;
    let mut discovered = Vec::new();
    while let Some(result) = tasks.join_next().await {
        if let Ok(Ok(peer)) = result {
            if let Ok(recorded) = store.record_localsend_lan_peer(peer.clone()) {
                discovered.push(recorded);
            } else {
                discovered.push(peer);
            }
        }
    }

    if discovered.is_empty() {
        Ok(store.list_localsend_lan_peers()?)
    } else {
        Ok(discovered)
    }
}

async fn probe_candidate(client: reqwest::Client, ip: Ipv4Addr) -> Result<LocalSendLanPeer> {
    let (info, protocol) = match probe_info(&client, "https", ip).await {
        Ok(info) => (info, "https"),
        Err(_) => (probe_info(&client, "http", ip).await?, "http"),
    };
    Ok(LocalSendLanPeer {
        fingerprint: info.fingerprint,
        alias: info.alias,
        source_ip: ip.to_string(),
        port: info.port.unwrap_or(LOCALSEND_PORT),
        protocol: info.protocol.unwrap_or_else(|| protocol.to_string()),
        device_model: info.device_model,
        device_type: info.device_type,
        download: info.download.unwrap_or(true),
        first_seen: 0,
        last_seen: 0,
    })
}

async fn probe_info(
    client: &reqwest::Client,
    protocol: &str,
    ip: Ipv4Addr,
) -> Result<InfoResponse> {
    let url = format!("{protocol}://{ip}:{LOCALSEND_PORT}/api/localsend/v2/info");
    Ok(client.get(url).send().await?.error_for_status()?.json().await?)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InfoResponse {
    alias: String,
    device_model: Option<String>,
    #[serde(rename = "deviceType", default)]
    device_type: Option<String>,
    fingerprint: String,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    protocol: Option<String>,
    #[serde(default)]
    download: Option<bool>,
}

fn local_ipv4() -> Result<Ipv4Addr> {
    let socket = std::net::UdpSocket::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)))?;
    socket.connect(SocketAddr::from((Ipv4Addr::new(8, 8, 8, 8), 80)))?;
    match socket.local_addr()?.ip() {
        IpAddr::V4(ip) => Ok(ip),
        IpAddr::V6(_) => anyhow::bail!("local route did not resolve to IPv4"),
    }
}
