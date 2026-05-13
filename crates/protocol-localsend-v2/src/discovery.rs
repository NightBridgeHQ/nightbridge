//! UDP multicast discovery for LocalSend v2.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use socket2::{Domain, Protocol as SocketProtocol, Socket, Type};
use tokio::net::UdpSocket;
use tokio::time;

use crate::dto::DeviceInfo;
use crate::Result;

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
    /// Whether this datagram is a multicast announcement or a unicast response.
    pub announce: bool,
}

impl Announcement {
    /// Creates a multicast discovery announcement.
    pub fn announce(info: DeviceInfo) -> Self {
        Self { info, announce: true }
    }

    /// Creates a discovery response for a received announcement.
    pub fn response(info: DeviceInfo) -> Self {
        Self { info, announce: false }
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
}

impl DiscoveryAnnouncer {
    /// Creates an announcer using the default LocalSend v2 multicast endpoint.
    pub fn new(info: DeviceInfo) -> Self {
        Self { info, multicast_addr: DEFAULT_DISCOVERY_ADDR, interval: Duration::from_secs(5) }
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

    /// Sends a single multicast announcement datagram.
    pub async fn announce_once(&self) -> Result<usize> {
        let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0)).await?;
        socket.set_multicast_loop_v4(true)?;

        let payload = serde_json::to_vec(&Announcement::announce(self.info.clone()))?;
        Ok(socket.send_to(&payload, self.multicast_addr).await?)
    }

    /// Sends discovery announcements forever at the configured interval.
    pub async fn run(&self) -> Result<()> {
        loop {
            self.announce_once().await?;
            time::sleep(self.interval).await;
        }
    }
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

    use crate::discovery::{Announcement, DiscoveryBrowser};
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

        let json = serde_json::to_value(&announcement).unwrap();

        assert_eq!(json["alias"], "NAS");
        assert_eq!(json["version"], "2.0");
        assert_eq!(json["deviceModel"], "Raspberry Pi");
        assert_eq!(json["deviceType"], "server");
        assert_eq!(json["fingerprint"], "abc");
        assert_eq!(json["port"], 53317);
        assert_eq!(json["protocol"], "https");
        assert_eq!(json["download"], true);
        assert_eq!(json["announce"], true);
    }

    #[test]
    fn response_sets_announce_false() {
        let response = Announcement::response(device_info("def"));

        let json = serde_json::to_value(&response).unwrap();

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
}
