//! UDP multicast discovery for LocalSend v2.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use socket2::{Domain, Protocol as SocketProtocol, Socket, Type};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::time;
use tracing::{info, trace, warn};

use crate::dto::{DeviceInfo, Protocol};
use crate::{LocalSendError, Result};

/// Default LocalSend v2 multicast group.
pub const DEFAULT_MULTICAST_IP: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 167);

/// Default LocalSend v2 UDP discovery port.
pub const DEFAULT_DISCOVERY_PORT: u16 = 53317;

/// Default LocalSend v2 UDP discovery socket address.
pub const DEFAULT_DISCOVERY_ADDR: SocketAddrV4 =
    SocketAddrV4::new(DEFAULT_MULTICAST_IP, DEFAULT_DISCOVERY_PORT);

/// LocalSend v2 UDP discovery payload.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Announcement {
    /// Device information advertised by the peer.
    #[serde(flatten)]
    pub info: DeviceInfo,
    /// Legacy v1 announcement flag kept for LocalSend app compatibility.
    #[serde(default)]
    pub announcement: bool,
    /// Whether this datagram is a multicast announcement or a unicast response.
    #[serde(default)]
    pub announce: bool,
}

impl Announcement {
    /// Creates a multicast discovery announcement.
    pub fn announce(info: DeviceInfo) -> Self {
        Self { info, announcement: true, announce: true }
    }

    /// Creates a discovery response for a received announcement.
    pub fn response(info: DeviceInfo) -> Self {
        Self { info, announcement: false, announce: false }
    }
}

/// Peer found through UDP discovery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LanPeer {
    /// Device information advertised by the peer.
    pub info: DeviceInfo,
    /// Socket address that should be used to reach the peer HTTP(S) API.
    pub address: SocketAddr,
    /// Source UDP socket address that sent the discovery datagram.
    pub source: SocketAddr,
    /// Local monotonic time when the peer was discovered.
    pub discovered_at: Instant,
}

/// Sends LocalSend v2 discovery announcements.
#[derive(Clone, Debug)]
pub struct DiscoveryAnnouncer {
    info: DeviceInfo,
    multicast_addr: SocketAddrV4,
    interval: Duration,
    peer_tx: Option<mpsc::UnboundedSender<LanPeer>>,
}

impl DiscoveryAnnouncer {
    /// Creates an announcer using the default LocalSend v2 multicast endpoint.
    pub fn new(info: DeviceInfo) -> Self {
        Self {
            info,
            multicast_addr: DEFAULT_DISCOVERY_ADDR,
            interval: Duration::from_secs(5),
            peer_tx: None,
        }
    }

    /// Overrides the multicast endpoint, primarily for integration tests.
    pub fn with_multicast_addr(mut self, multicast_addr: SocketAddrV4) -> Self {
        self.multicast_addr = multicast_addr;
        self
    }

    /// Overrides the periodic announcement interval.
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    /// Sends observed foreign LocalSend peers to the supplied channel.
    pub fn with_peer_tx(mut self, peer_tx: mpsc::UnboundedSender<LanPeer>) -> Self {
        self.peer_tx = Some(peer_tx);
        self
    }

    /// Sends a single multicast announcement datagram.
    pub async fn announce_once(&self) -> Result<usize> {
        let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0)).await?;
        socket.set_multicast_loop_v4(true)?;

        let payload = serde_json::to_vec(&Announcement::announce(self.info.clone()))?;
        Ok(socket.send_to(&payload, self.multicast_addr).await?)
    }

    /// Sends discovery announcements forever at the configured interval.
    pub async fn run(&self) -> Result<()> {
        let socket = bind_multicast_socket(self.multicast_addr)?;
        let socket = UdpSocket::from_std(socket.into())?;
        let mut buf = vec![0; 16 * 1024];
        let mut interval = time::interval(self.interval);

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let payload = serde_json::to_vec(&Announcement::announce(self.info.clone()))?;
                    socket.send_to(&payload, self.multicast_addr).await?;
                }
                received = socket.recv_from(&mut buf) => {
                    let (len, source) = received?;
                    let payload = &buf[..len];
                    match self.parse_peer(payload, source) {
                        Ok(Some(peer)) => {
                            if let Some(peer_tx) = &self.peer_tx {
                                let _ = peer_tx.send(peer);
                            }
                        }
                        Ok(None) => {}
                        Err(error) => {
                            warn!(source = %source, error = %error, "failed to parse LocalSend discovery peer");
                        }
                    }
                    match self.should_answer(payload) {
                        Ok(true) => {
                            let url = self.register_url(payload, source)?;
                            info!(source = %source, register_url = %url, "received LocalSend discovery announcement");
                            if post_register_url(&self.info, &url).await.is_ok() {
                                info!(source = %source, "responded to LocalSend announcement via TCP register");
                            } else {
                                warn!(source = %source, "TCP register response failed; sending UDP fallback");
                                let fallback = serde_json::to_vec(&Announcement::response(self.info.clone()))?;
                                socket.send_to(&fallback, self.multicast_addr).await?;
                            }
                        }
                        Ok(false) => trace!(source = %source, "ignored LocalSend discovery packet"),
                        Err(error) => {
                            warn!(source = %source, error = %error, "failed to parse LocalSend discovery packet");
                        }
                    }
                }
            }
        }
    }

    fn should_answer(&self, payload: &[u8]) -> Result<bool> {
        let announcement: Announcement = serde_json::from_slice(payload)?;
        Ok((announcement.announcement || announcement.announce)
            && announcement.info.fingerprint != self.info.fingerprint)
    }

    fn parse_peer(&self, payload: &[u8], source: SocketAddr) -> Result<Option<LanPeer>> {
        let announcement: Announcement = serde_json::from_slice(payload)?;
        if announcement.info.fingerprint == self.info.fingerprint {
            return Ok(None);
        }
        let address = peer_api_address(source, announcement.info.port);
        Ok(Some(LanPeer {
            info: announcement.info,
            address,
            source,
            discovered_at: Instant::now(),
        }))
    }

    #[cfg(test)]
    fn fallback_response_payload_for(&self, payload: &[u8]) -> Result<Option<Vec<u8>>> {
        if !self.should_answer(payload)? {
            return Ok(None);
        }

        Ok(Some(serde_json::to_vec(&Announcement::response(self.info.clone()))?))
    }

    fn register_url(&self, payload: &[u8], source: SocketAddr) -> Result<String> {
        let announcement: Announcement = serde_json::from_slice(payload)?;
        let target = peer_api_address(source, announcement.info.port);
        Ok(register_url(target, &announcement.info.protocol))
    }
}

async fn post_register_url(info: &DeviceInfo, url: &str) -> Result<()> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|error| LocalSendError::Http(error.to_string()))?;
    let response = client
        .post(url)
        .json(info)
        .send()
        .await
        .map_err(|error| LocalSendError::Http(error.to_string()))?;

    if response.status().is_success() {
        Ok(())
    } else {
        Err(LocalSendError::Http(format!("register response status {}", response.status())))
    }
}

fn register_url(target: SocketAddr, protocol: &Protocol) -> String {
    format!("{}://{target}/api/localsend/v2/register", protocol.as_str())
}

/// Listens for LocalSend v2 UDP discovery announcements.
#[derive(Clone, Debug)]
pub struct DiscoveryBrowser {
    self_fingerprint: String,
    multicast_addr: SocketAddrV4,
    timeout: Duration,
}

impl DiscoveryBrowser {
    /// Creates a browser using the default LocalSend v2 multicast endpoint.
    pub fn new(self_fingerprint: impl Into<String>) -> Self {
        Self {
            self_fingerprint: self_fingerprint.into(),
            multicast_addr: DEFAULT_DISCOVERY_ADDR,
            timeout: Duration::from_secs(5),
        }
    }

    /// Overrides the multicast endpoint, primarily for integration tests.
    pub fn with_multicast_addr(mut self, multicast_addr: SocketAddrV4) -> Self {
        self.multicast_addr = multicast_addr;
        self
    }

    /// Overrides the receive timeout for one listen attempt.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Waits for one peer announcement or response.
    pub async fn listen_once(&self) -> Result<Option<LanPeer>> {
        let socket = bind_multicast_socket(self.multicast_addr)?;
        let socket = UdpSocket::from_std(socket.into())?;
        let mut buf = vec![0; 16 * 1024];

        loop {
            let received = time::timeout(self.timeout, socket.recv_from(&mut buf)).await;
            let (len, source) = match received {
                Ok(result) => result?,
                Err(_) => return Ok(None),
            };

            if let Some(peer) = self.parse_peer(&buf[..len], source)? {
                return Ok(Some(peer));
            }
        }
    }

    fn parse_peer(&self, payload: &[u8], source: SocketAddr) -> Result<Option<LanPeer>> {
        let announcement: Announcement = serde_json::from_slice(payload)?;

        if announcement.info.fingerprint == self.self_fingerprint {
            return Ok(None);
        }

        let address = peer_api_address(source, announcement.info.port);

        Ok(Some(LanPeer {
            info: announcement.info,
            address,
            source,
            discovered_at: Instant::now(),
        }))
    }
}

fn bind_multicast_socket(multicast_addr: SocketAddrV4) -> Result<Socket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(SocketProtocol::UDP))?;
    socket.set_reuse_address(true)?;

    #[cfg(unix)]
    socket.set_reuse_port(true)?;

    socket.bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, multicast_addr.port()).into())?;
    socket.join_multicast_v4(multicast_addr.ip(), &Ipv4Addr::UNSPECIFIED)?;
    socket.set_nonblocking(true)?;

    Ok(socket)
}

fn peer_api_address(source: SocketAddr, advertised_port: u16) -> SocketAddr {
    match source {
        SocketAddr::V4(addr) => SocketAddr::V4(SocketAddrV4::new(*addr.ip(), advertised_port)),
        SocketAddr::V6(addr) => SocketAddr::V6(std::net::SocketAddrV6::new(
            *addr.ip(),
            advertised_port,
            addr.flowinfo(),
            addr.scope_id(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

    use crate::discovery::{Announcement, DiscoveryAnnouncer, DiscoveryBrowser};
    use crate::dto::{DeviceInfo, Protocol};

    fn device_info(fingerprint: &str) -> DeviceInfo {
        DeviceInfo {
            alias: "NAS".to_string(),
            version: "2.0".to_string(),
            device_model: Some("Raspberry Pi".to_string()),
            device_type: Some("server".to_string()),
            fingerprint: fingerprint.to_string(),
            port: 53317,
            protocol: Protocol::from("https"),
            download: true,
        }
    }

    #[test]
    fn announcement_matches_official_fields() {
        let announcement = Announcement::announce(device_info("abc"));

        let json = serde_json::to_value(announcement).unwrap();

        assert_eq!(json["alias"], "NAS");
        assert_eq!(json["version"], "2.0");
        assert_eq!(json["deviceModel"], "Raspberry Pi");
        assert_eq!(json["deviceType"], "server");
        assert_eq!(json["fingerprint"], "abc");
        assert_eq!(json["port"], 53317);
        assert_eq!(json["protocol"], "https");
        assert_eq!(json["download"], true);
        assert_eq!(json["announcement"], true);
        assert_eq!(json["announce"], true);
    }

    #[test]
    fn response_sets_announce_false() {
        let response = Announcement::response(device_info("def"));

        let json = serde_json::to_value(response).unwrap();

        assert_eq!(json["announcement"], false);
        assert_eq!(json["announce"], false);
    }

    #[test]
    fn self_fingerprint_is_ignored_by_browser() {
        let browser = DiscoveryBrowser::new("self-fingerprint");
        let sender = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 50123));
        let payload =
            serde_json::to_vec(&Announcement::announce(device_info("self-fingerprint"))).unwrap();

        let peer = browser.parse_peer(&payload, sender).unwrap();

        assert!(peer.is_none());
    }

    #[test]
    fn announcer_responds_to_foreign_announce() {
        let announcer = DiscoveryAnnouncer::new(device_info("self-fingerprint"));
        let payload =
            serde_json::to_vec(&Announcement::announce(device_info("peer-fingerprint"))).unwrap();

        let response = announcer.fallback_response_payload_for(&payload).unwrap().unwrap();
        let announcement: Announcement = serde_json::from_slice(&response).unwrap();

        assert_eq!(announcement.info.fingerprint, "self-fingerprint");
        assert!(!announcement.announce);
    }

    #[test]
    fn announcer_ignores_self_or_response_packets() {
        let announcer = DiscoveryAnnouncer::new(device_info("self-fingerprint"));
        let self_payload =
            serde_json::to_vec(&Announcement::announce(device_info("self-fingerprint"))).unwrap();
        let response_payload =
            serde_json::to_vec(&Announcement::response(device_info("peer-fingerprint"))).unwrap();

        assert!(announcer.fallback_response_payload_for(&self_payload).unwrap().is_none());
        assert!(announcer.fallback_response_payload_for(&response_payload).unwrap().is_none());
    }

    #[test]
    fn announcer_builds_tcp_register_url_from_peer_announcement() {
        let announcer = DiscoveryAnnouncer::new(device_info("self-fingerprint"));
        let sender = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 50), 50123));
        let payload =
            serde_json::to_vec(&Announcement::announce(device_info("peer-fingerprint"))).unwrap();

        let url = announcer.register_url(&payload, sender).unwrap();

        assert_eq!(url, "https://192.0.2.50:53317/api/localsend/v2/register");
    }

    #[test]
    fn announcer_parses_foreign_response_as_observed_peer() {
        let announcer = DiscoveryAnnouncer::new(device_info("self-fingerprint"));
        let sender = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 81), 50123));
        let payload =
            serde_json::to_vec(&Announcement::response(device_info("peer-fingerprint"))).unwrap();

        let peer = announcer.parse_peer(&payload, sender).unwrap().unwrap();

        assert_eq!(peer.info.fingerprint, "peer-fingerprint");
        assert_eq!(peer.address.to_string(), "192.0.2.81:53317");
    }
}
