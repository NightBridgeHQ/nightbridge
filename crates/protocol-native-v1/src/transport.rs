//! QUIC transport skeleton for the native protocol.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use crate::framing::{read_control, write_control, ControlMessage};
use crate::{NativeError, Result};

/// ALPN identifier for LocalSend Improvements native LAN protocol v1.
pub const NATIVE_ALPN: &[u8] = b"lsi-native-v1";

/// Transport-level defaults for native QUIC connections.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeTransportConfig {
    /// Application protocol identifier advertised through TLS ALPN.
    pub alpn: Vec<u8>,
    /// Maximum concurrently-open bidirectional streams allowed from the peer.
    pub max_concurrent_bidi_streams: u32,
    /// Maximum concurrently-open unidirectional streams allowed from the peer.
    pub max_concurrent_uni_streams: u32,
    /// QUIC idle timeout in milliseconds.
    pub max_idle_timeout_ms: u64,
    /// Optional keep-alive interval in milliseconds.
    pub keep_alive_interval_ms: Option<u64>,
}

impl Default for NativeTransportConfig {
    fn default() -> Self {
        Self {
            alpn: NATIVE_ALPN.to_vec(),
            max_concurrent_bidi_streams: 64,
            max_concurrent_uni_streams: 16,
            max_idle_timeout_ms: 30_000,
            keep_alive_interval_ms: Some(10_000),
        }
    }
}

impl NativeTransportConfig {
    /// Validates transport configuration before it is applied to Quinn.
    pub fn validate(&self) -> Result<()> {
        if self.alpn.is_empty() {
            return Err(transport_error("ALPN must not be empty"));
        }
        if self.max_concurrent_bidi_streams == 0 {
            return Err(transport_error("max_concurrent_bidi_streams must be greater than zero"));
        }
        if self.max_idle_timeout_ms == 0 {
            return Err(transport_error("max_idle_timeout_ms must be greater than zero"));
        }
        if matches!(self.keep_alive_interval_ms, Some(0)) {
            return Err(transport_error(
                "keep_alive_interval_ms must be greater than zero when configured",
            ));
        }

        Ok(())
    }

    /// Builds the Quinn transport config shared by client and server endpoints.
    pub fn build_quinn_transport_config(&self) -> Result<quinn::TransportConfig> {
        self.validate()?;

        let idle_timeout = Duration::from_millis(self.max_idle_timeout_ms)
            .try_into()
            .map_err(|error| transport_error(format!("invalid idle timeout: {error}")))?;

        let mut transport = quinn::TransportConfig::default();
        transport
            .max_concurrent_bidi_streams(quinn::VarInt::from_u32(self.max_concurrent_bidi_streams));
        transport
            .max_concurrent_uni_streams(quinn::VarInt::from_u32(self.max_concurrent_uni_streams));
        transport.max_idle_timeout(Some(idle_timeout));
        transport.keep_alive_interval(self.keep_alive_interval_ms.map(Duration::from_millis));
        Ok(transport)
    }

    /// Applies native transport settings to a prebuilt Quinn server config.
    pub fn apply_to_server_config(
        &self,
        mut server_config: quinn::ServerConfig,
    ) -> Result<quinn::ServerConfig> {
        server_config.transport_config(Arc::new(self.build_quinn_transport_config()?));
        Ok(server_config)
    }

    /// Applies native transport settings to a prebuilt Quinn client config.
    pub fn apply_to_client_config(
        &self,
        mut client_config: quinn::ClientConfig,
    ) -> Result<quinn::ClientConfig> {
        client_config.transport_config(Arc::new(self.build_quinn_transport_config()?));
        Ok(client_config)
    }
}

/// Local bind address for a native QUIC server endpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeServerBind {
    /// IP address to bind.
    pub host: String,
    /// UDP port to bind.
    pub port: u16,
}

impl NativeServerBind {
    /// Validates the bind address without opening sockets.
    pub fn validate(&self) -> Result<()> {
        validate_host_port(&self.host, self.port)
    }

    /// Converts the bind address to a socket address.
    pub fn socket_addr(&self) -> Result<SocketAddr> {
        socket_addr(&self.host, self.port)
    }
}

/// Remote address for a native QUIC client connection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeClientTarget {
    /// Peer IP address.
    pub host: String,
    /// Peer UDP port.
    pub port: u16,
}

impl NativeClientTarget {
    /// Validates the target address without opening sockets.
    pub fn validate(&self) -> Result<()> {
        validate_host_port(&self.host, self.port)
    }

    /// Converts the target address to a socket address.
    pub fn socket_addr(&self) -> Result<SocketAddr> {
        socket_addr(&self.host, self.port)
    }
}

/// Thin Quinn endpoint wrapper for native protocol callers.
#[derive(Debug)]
pub struct NativeEndpoint {
    endpoint: quinn::Endpoint,
}

impl NativeEndpoint {
    /// Wraps an existing Quinn endpoint.
    pub fn from_quinn(endpoint: quinn::Endpoint) -> Self {
        Self { endpoint }
    }

    /// Returns the wrapped Quinn endpoint.
    pub fn into_quinn(self) -> quinn::Endpoint {
        self.endpoint
    }

    /// Borrows the wrapped Quinn endpoint.
    pub fn endpoint(&self) -> &quinn::Endpoint {
        &self.endpoint
    }
}

/// Binds a native server endpoint using an externally-built TLS/QUIC server config.
pub fn bind_server_endpoint(
    bind: &NativeServerBind,
    server_config: quinn::ServerConfig,
) -> Result<NativeEndpoint> {
    let endpoint = quinn::Endpoint::server(server_config, bind.socket_addr()?)?;
    Ok(NativeEndpoint::from_quinn(endpoint))
}

/// Binds a native client endpoint with an externally-built TLS/QUIC client config.
pub fn bind_client_endpoint(client_config: quinn::ClientConfig) -> Result<NativeEndpoint> {
    let mut endpoint = quinn::Endpoint::client(([0, 0, 0, 0], 0).into())?;
    endpoint.set_default_client_config(client_config);
    Ok(NativeEndpoint::from_quinn(endpoint))
}

/// Connects an existing native endpoint to a remote target.
pub async fn connect(
    endpoint: &NativeEndpoint,
    target: &NativeClientTarget,
    server_name: &str,
) -> Result<quinn::Connection> {
    if server_name.trim().is_empty() {
        return Err(transport_error("server_name must not be empty"));
    }

    let connecting = endpoint
        .endpoint
        .connect(target.socket_addr()?, server_name)
        .map_err(map_transport_error)?;

    connecting.await.map_err(map_transport_error)
}

/// Bidirectional QUIC stream carrying native control messages.
#[derive(Debug)]
pub struct NativeControlStream {
    send: quinn::SendStream,
    recv: quinn::RecvStream,
}

impl NativeControlStream {
    /// Creates a control stream from Quinn bidirectional stream halves.
    pub fn new(send: quinn::SendStream, recv: quinn::RecvStream) -> Self {
        Self { send, recv }
    }

    /// Opens a new outgoing control stream on a connection.
    pub async fn open(connection: &quinn::Connection) -> Result<Self> {
        let (send, recv) = connection.open_bi().await.map_err(map_transport_error)?;
        Ok(Self::new(send, recv))
    }

    /// Accepts the next incoming control stream on a connection.
    pub async fn accept(connection: &quinn::Connection) -> Result<Self> {
        let (send, recv) = connection.accept_bi().await.map_err(map_transport_error)?;
        Ok(Self::new(send, recv))
    }

    /// Writes one control message to the stream.
    pub async fn write(&mut self, message: &ControlMessage) -> Result<()> {
        write_control(&mut self.send, message).await
    }

    /// Reads one control message from the stream.
    pub async fn read(&mut self) -> Result<ControlMessage> {
        read_control(&mut self.recv).await
    }

    /// Borrows the send half.
    pub fn send_mut(&mut self) -> &mut quinn::SendStream {
        &mut self.send
    }

    /// Borrows the receive half.
    pub fn recv_mut(&mut self) -> &mut quinn::RecvStream {
        &mut self.recv
    }

    /// Splits the wrapper back into Quinn stream halves.
    pub fn into_inner(self) -> (quinn::SendStream, quinn::RecvStream) {
        (self.send, self.recv)
    }
}

fn validate_host_port(host: &str, port: u16) -> Result<()> {
    if host.trim().is_empty() {
        return Err(transport_error("host must not be empty"));
    }
    if port == 0 {
        return Err(transport_error("port must be greater than zero"));
    }
    let _ = parse_ip(host)?;
    Ok(())
}

fn socket_addr(host: &str, port: u16) -> Result<SocketAddr> {
    validate_host_port(host, port)?;
    Ok(SocketAddr::new(parse_ip(host)?, port))
}

fn parse_ip(host: &str) -> Result<IpAddr> {
    host.parse().map_err(|error| transport_error(format!("invalid IP address {host}: {error}")))
}

fn map_transport_error(error: impl std::error::Error) -> NativeError {
    transport_error(error.to_string())
}

fn transport_error(message: impl Into<String>) -> NativeError {
    NativeError::Transport(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NativeError;

    #[test]
    fn default_config_uses_native_alpn_and_safe_stream_limits() {
        let config = NativeTransportConfig::default();

        assert_eq!(config.alpn, NATIVE_ALPN);
        assert!(config.max_concurrent_bidi_streams > 0);
        assert!(config.max_idle_timeout_ms > 0);
    }

    #[test]
    fn config_validation_rejects_empty_alpn() {
        let mut config = NativeTransportConfig::default();
        config.alpn.clear();

        let error = config.validate().unwrap_err();

        assert!(error.to_string().contains("ALPN"));
    }

    #[test]
    fn client_target_validation_requires_nonzero_port() {
        let target = NativeClientTarget { host: "127.0.0.1".to_string(), port: 0 };

        let error = target.validate().unwrap_err();

        assert!(error.to_string().contains("port"));
    }

    #[test]
    fn listener_bind_validation_requires_nonzero_port() {
        let bind = NativeServerBind { host: "0.0.0.0".to_string(), port: 0 };

        let error = bind.validate().unwrap_err();

        assert!(error.to_string().contains("port"));
    }

    #[test]
    fn target_socket_addr_accepts_literal_ip_addresses() {
        let target = NativeClientTarget { host: "127.0.0.1".to_string(), port: 53318 };

        assert_eq!(target.socket_addr().unwrap(), SocketAddr::from(([127, 0, 0, 1], 53318)));
    }

    #[test]
    fn build_quinn_transport_config_rejects_invalid_config() {
        let config = NativeTransportConfig {
            max_concurrent_bidi_streams: 0,
            ..NativeTransportConfig::default()
        };

        let error = config.build_quinn_transport_config().unwrap_err();

        assert!(error.to_string().contains("max_concurrent_bidi_streams"));
    }

    #[test]
    fn transport_error_helper_maps_to_native_transport_error() {
        let error = transport_error("endpoint unavailable");

        assert!(matches!(error, NativeError::Transport(_)));
        assert_eq!(error.to_string(), "transport: endpoint unavailable");
    }
}
