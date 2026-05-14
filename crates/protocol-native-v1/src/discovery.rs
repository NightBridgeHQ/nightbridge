//! mDNS discovery for the native LAN protocol.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use mdns_sd::{ResolvedService, ServiceDaemon, ServiceEvent, ServiceInfo, TxtProperties};

use crate::dto::NativePeerInfo;
use crate::{NativeError, Result};

/// Native protocol mDNS service type.
///
/// The service label is intentionally short enough for the conservative DNS-SD
/// 15-byte service-name limit enforced by `mdns-sd`.
pub const NATIVE_MDNS_SERVICE_TYPE: &str = "_lsi-native._udp.local.";

const TXT_ALIAS: &str = "alias";
const TXT_FINGERPRINT: &str = "fingerprint";
const TXT_PUBKEY: &str = "pubkey";
const TXT_QUIC_PORT: &str = "quic_port";
const TXT_EXTENSIONS: &str = "extensions";

/// Builds mDNS TXT properties for a native peer advertisement.
pub fn peer_info_to_txt_properties(info: &NativePeerInfo) -> Result<HashMap<String, String>> {
    validate_peer_info(info)?;

    let mut properties = HashMap::new();
    properties.insert(TXT_ALIAS.to_string(), info.alias.trim().to_string());
    properties.insert(TXT_FINGERPRINT.to_string(), info.fingerprint.trim().to_string());
    properties.insert(TXT_PUBKEY.to_string(), encode_hex(&info.pubkey));
    properties.insert(TXT_QUIC_PORT.to_string(), info.quic_port.to_string());
    properties.insert(TXT_EXTENSIONS.to_string(), info.extensions.join(","));
    Ok(properties)
}

/// Parses native peer information from mDNS TXT properties.
pub fn peer_info_from_txt_properties(
    properties: &HashMap<String, String>,
) -> Result<NativePeerInfo> {
    let properties = normalized_properties(properties);
    let alias = required_property(&properties, TXT_ALIAS)?;
    validate_txt_value(TXT_ALIAS, alias)?;

    let fingerprint = required_property(&properties, TXT_FINGERPRINT)?;
    validate_txt_value(TXT_FINGERPRINT, fingerprint)?;

    let pubkey = decode_pubkey(required_property(&properties, TXT_PUBKEY)?)?;
    let quic_port = decode_quic_port(required_property(&properties, TXT_QUIC_PORT)?)?;
    let extensions = parse_extensions(required_property(&properties, TXT_EXTENSIONS)?)?;

    Ok(NativePeerInfo {
        alias: alias.to_string(),
        fingerprint: fingerprint.to_string(),
        pubkey,
        quic_port,
        extensions,
    })
}

/// Converts mdns-sd TXT properties into native peer information.
pub fn peer_info_from_mdns_txt(properties: &TxtProperties) -> Result<NativePeerInfo> {
    peer_info_from_txt_properties(&txt_properties_to_map(properties))
}

/// Converts a resolved mDNS service into native peer information.
pub fn peer_info_from_resolved_service(
    service: &ResolvedService,
) -> Result<Option<NativePeerInfo>> {
    if service.ty_domain != NATIVE_MDNS_SERVICE_TYPE {
        return Ok(None);
    }

    Ok(Some(peer_info_from_mdns_txt(service.get_properties())?))
}

/// Builds a native mDNS service record for registration.
pub fn service_info_for_peer<Ip: mdns_sd::AsIpAddrs>(
    info: &NativePeerInfo,
    instance_name: &str,
    host_name: &str,
    ip: Ip,
) -> Result<ServiceInfo> {
    if instance_name.trim().is_empty() {
        return Err(discovery_error("instance_name is required"));
    }
    if host_name.trim().is_empty() {
        return Err(discovery_error("host_name is required"));
    }

    let service = ServiceInfo::new(
        NATIVE_MDNS_SERVICE_TYPE,
        instance_name,
        host_name,
        ip,
        info.quic_port,
        peer_info_to_txt_properties(info)?,
    )
    .map_err(mdns_error)?;

    Ok(service)
}

/// Registered native mDNS announcer.
pub struct NativeDiscoveryAnnouncer {
    daemon: ServiceDaemon,
    fullname: String,
}

impl NativeDiscoveryAnnouncer {
    /// Registers a native peer advertisement with the local mDNS daemon.
    pub fn register<Ip: mdns_sd::AsIpAddrs>(
        info: &NativePeerInfo,
        instance_name: &str,
        host_name: &str,
        ip: Ip,
    ) -> Result<Self> {
        let service = service_info_for_peer(info, instance_name, host_name, ip)?;
        let fullname = service.get_fullname().to_string();
        let daemon = ServiceDaemon::new().map_err(mdns_error)?;
        daemon.register(service).map_err(mdns_error)?;

        Ok(Self { daemon, fullname })
    }

    /// Returns the full DNS-SD service instance name being advertised.
    pub fn fullname(&self) -> &str {
        &self.fullname
    }

    /// Requests mDNS daemon shutdown for this announcer.
    pub fn shutdown(&self) -> Result<()> {
        self.daemon.shutdown().map(|_| ()).map_err(mdns_error)
    }
}

/// Browser for native mDNS peer advertisements.
pub struct NativeDiscoveryBrowser {
    daemon: ServiceDaemon,
    receiver: mdns_sd::Receiver<ServiceEvent>,
}

impl NativeDiscoveryBrowser {
    /// Starts browsing for native LAN peers.
    pub fn browse() -> Result<Self> {
        let daemon = ServiceDaemon::new().map_err(mdns_error)?;
        let receiver = daemon.browse(NATIVE_MDNS_SERVICE_TYPE).map_err(mdns_error)?;
        Ok(Self { daemon, receiver })
    }

    /// Waits for one resolved peer advertisement.
    pub fn recv_timeout(&self, timeout: Duration) -> Result<Option<NativePeerInfo>> {
        let deadline = Instant::now() + timeout;

        loop {
            let now = Instant::now();
            if now >= deadline {
                return Ok(None);
            }

            let remaining = deadline.saturating_duration_since(now);
            let event = match self.receiver.recv_timeout(remaining) {
                Ok(event) => event,
                Err(_) => return Ok(None),
            };

            if let ServiceEvent::ServiceResolved(service) = event {
                if let Some(peer) = peer_info_from_resolved_service(&service)? {
                    return Ok(Some(peer));
                }
            }
        }
    }

    /// Requests mDNS daemon shutdown for this browser.
    pub fn shutdown(&self) -> Result<()> {
        self.daemon.shutdown().map(|_| ()).map_err(mdns_error)
    }
}

fn validate_peer_info(info: &NativePeerInfo) -> Result<()> {
    validate_txt_value(TXT_ALIAS, &info.alias)?;
    validate_txt_value(TXT_FINGERPRINT, &info.fingerprint)?;
    validate_quic_port(info.quic_port)?;
    validate_extensions(&info.extensions)
}

fn validate_txt_value(field: &str, value: &str) -> Result<()> {
    let value = value.trim();
    if value.is_empty() {
        return Err(discovery_error(format!("{field} is required")));
    }
    if !value.is_ascii() {
        return Err(discovery_error(format!("{field} must be ASCII")));
    }
    if value.len() > u8::MAX as usize {
        return Err(discovery_error(format!("{field} must fit in one TXT record")));
    }
    Ok(())
}

fn validate_quic_port(port: u16) -> Result<()> {
    if port == 0 {
        return Err(discovery_error("quic_port must be non-zero"));
    }
    Ok(())
}

fn validate_extensions(extensions: &[String]) -> Result<()> {
    for extension in extensions {
        let extension = extension.trim();
        if extension.is_empty() {
            return Err(discovery_error("extension value is required"));
        }
        if !extension.is_ascii() || extension.contains(',') || extension.len() > 64 {
            return Err(discovery_error(format!("invalid extension: {extension}")));
        }
    }
    Ok(())
}

fn parse_extensions(value: &str) -> Result<Vec<String>> {
    let extensions = value
        .split(',')
        .map(str::trim)
        .filter(|extension| !extension.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();

    validate_extensions(&extensions)?;
    Ok(extensions)
}

fn decode_quic_port(value: &str) -> Result<u16> {
    let port = value
        .parse::<u16>()
        .map_err(|_| discovery_error(format!("quic_port is invalid: {value}")))?;
    validate_quic_port(port)?;
    Ok(port)
}

fn decode_pubkey(value: &str) -> Result<[u8; 32]> {
    if value.len() != 64 {
        return Err(discovery_error("pubkey must be 64 hex characters"));
    }

    let mut pubkey = [0_u8; 32];
    for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_nibble(chunk[0])?;
        let low = hex_nibble(chunk[1])?;
        pubkey[index] = (high << 4) | low;
    }

    Ok(pubkey)
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn hex_nibble(byte: u8) -> Result<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(discovery_error("pubkey contains non-hex characters")),
    }
}

fn txt_properties_to_map(properties: &TxtProperties) -> HashMap<String, String> {
    properties
        .iter()
        .map(|property| (property.key().to_string(), property.val_str().to_string()))
        .collect()
}

fn normalized_properties(properties: &HashMap<String, String>) -> HashMap<String, String> {
    properties
        .iter()
        .map(|(key, value)| (key.to_ascii_lowercase(), value.trim().to_string()))
        .collect()
}

fn required_property<'a>(properties: &'a HashMap<String, String>, key: &str) -> Result<&'a str> {
    properties
        .get(key)
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| discovery_error(format!("{key} is required")))
}

fn mdns_error(error: mdns_sd::Error) -> NativeError {
    NativeError::Transport(format!("mdns discovery: {error}"))
}

fn discovery_error(message: impl Into<String>) -> NativeError {
    NativeError::Protocol(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::{default_extensions, NativePeerInfo, EXT_BLAKE3_V1, EXT_RESUME_V1};

    fn peer() -> NativePeerInfo {
        NativePeerInfo {
            alias: "workstation".to_string(),
            fingerprint: "fp-123456".to_string(),
            pubkey: [7; 32],
            quic_port: 48_900,
            extensions: default_extensions(),
        }
    }

    #[test]
    fn txt_properties_roundtrip_peer_info() {
        let info = peer();

        let properties = peer_info_to_txt_properties(&info).unwrap();
        let decoded = peer_info_from_txt_properties(&properties).unwrap();

        assert_eq!(decoded, info);
    }

    #[test]
    fn parsing_rejects_missing_required_fields() {
        let mut properties = peer_info_to_txt_properties(&peer()).unwrap();
        properties.remove("fingerprint");

        let error = peer_info_from_txt_properties(&properties).unwrap_err();

        assert!(error.to_string().contains("fingerprint"));
    }

    #[test]
    fn building_rejects_empty_alias() {
        let mut info = peer();
        info.alias = " ".to_string();

        let error = peer_info_to_txt_properties(&info).unwrap_err();

        assert!(error.to_string().contains("alias"));
    }

    #[test]
    fn parsing_rejects_malformed_pubkey_hex() {
        let mut properties = peer_info_to_txt_properties(&peer()).unwrap();
        properties.insert("pubkey".to_string(), "abcd".to_string());

        let error = peer_info_from_txt_properties(&properties).unwrap_err();

        assert!(error.to_string().contains("pubkey"));
    }

    #[test]
    fn parsing_rejects_malformed_quic_port() {
        let mut properties = peer_info_to_txt_properties(&peer()).unwrap();
        properties.insert("quic_port".to_string(), "not-a-port".to_string());

        let error = peer_info_from_txt_properties(&properties).unwrap_err();

        assert!(error.to_string().contains("quic_port"));
    }

    #[test]
    fn parsing_splits_extensions_and_drops_empty_entries() {
        let mut properties = peer_info_to_txt_properties(&peer()).unwrap();
        properties.insert("extensions".to_string(), format!("{EXT_RESUME_V1},,{EXT_BLAKE3_V1}, "));

        let decoded = peer_info_from_txt_properties(&properties).unwrap();

        assert_eq!(decoded.extensions, vec![EXT_RESUME_V1.to_string(), EXT_BLAKE3_V1.to_string()]);
    }
}
