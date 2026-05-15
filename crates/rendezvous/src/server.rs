use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use quinn::crypto::rustls::QuicServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::protocol::{ClientMessage, ErrorResponse, ServerMessage};
use crate::registry::{Registry, RegistryError};

/// ALPN identifier for the LocalSend Improved rendezvous protocol.
pub const RENDEZVOUS_ALPN: &[u8] = b"lsi-rendezvous-v1";

const MAX_FRAME_BYTES: usize = 1024 * 1024;

/// Runtime configuration for a rendezvous server.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RendezvousServerConfig {
    /// UDP address to bind for QUIC.
    pub bind_addr: SocketAddr,
    /// Maximum TTL accepted for registry entries.
    pub max_ttl: Duration,
    /// Interval used to prune expired registry entries.
    pub prune_interval: Duration,
}

impl Default for RendezvousServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: SocketAddr::from((Ipv4Addr::UNSPECIFIED, 53410)),
            max_ttl: Duration::from_secs(300),
            prune_interval: Duration::from_secs(30),
        }
    }
}

/// Running rendezvous server handle.
#[derive(Debug)]
pub struct RendezvousServer {
    local_addr: SocketAddr,
    endpoint: quinn::Endpoint,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    task: JoinHandle<Result<()>>,
}

impl RendezvousServer {
    /// Binds and starts a rendezvous server.
    pub async fn bind(config: RendezvousServerConfig) -> Result<Self> {
        ensure_ring_crypto_provider();
        let server_config = rendezvous_server_config()?;
        let endpoint =
            quinn::Endpoint::server(server_config, config.bind_addr).context("bind rendezvous")?;
        let local_addr = endpoint.local_addr().context("read rendezvous bind address")?;
        let registry = Arc::new(tokio::sync::Mutex::new(Registry::new(config.max_ttl)));
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let server_endpoint = endpoint.clone();
        let task =
            tokio::spawn(run_server(server_endpoint, registry, config.prune_interval, shutdown_rx));

        info!(addr = %local_addr, "rendezvous server started");

        Ok(Self { local_addr, endpoint, shutdown_tx, task })
    }

    /// Binds a test server to an ephemeral loopback port.
    #[cfg(test)]
    pub async fn bind_test() -> Result<Self> {
        Self::bind(RendezvousServerConfig {
            bind_addr: SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
            max_ttl: Duration::from_secs(120),
            prune_interval: Duration::from_millis(50),
        })
        .await
    }

    /// Returns the local UDP address used by the server.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Stops the server and waits for the accept task to finish.
    pub async fn stop(self) -> Result<()> {
        let _ = self.shutdown_tx.send(true);
        self.endpoint.close(0_u32.into(), b"rendezvous shutdown");
        self.task.await.context("rendezvous server task panicked")?
    }
}

async fn run_server(
    endpoint: quinn::Endpoint,
    registry: Arc<tokio::sync::Mutex<Registry>>,
    prune_interval: Duration,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    let mut prune_tick = tokio::time::interval(prune_interval);
    loop {
        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    return Ok(());
                }
            }
            _ = prune_tick.tick() => {
                let pruned = registry.lock().await.prune_expired(Instant::now());
                if pruned > 0 {
                    info!(pruned, "pruned expired rendezvous entries");
                }
            }
            incoming = endpoint.accept() => {
                let Some(incoming) = incoming else {
                    return Ok(());
                };
                let registry = Arc::clone(&registry);
                tokio::spawn(async move {
                    if let Err(error) = handle_connection(incoming, registry).await {
                        warn!(%error, "rendezvous connection failed");
                    }
                });
            }
        }
    }
}

async fn handle_connection(
    incoming: quinn::Incoming,
    registry: Arc<tokio::sync::Mutex<Registry>>,
) -> Result<()> {
    let connection = incoming.await.context("accept rendezvous QUIC connection")?;

    while let Ok((mut send, mut recv)) = connection.accept_bi().await {
        let registry = Arc::clone(&registry);
        tokio::spawn(async move {
            let response = match read_client_message(&mut recv).await {
                Ok(message) => handle_message(message, registry).await,
                Err(error) => ServerMessage::Error(ErrorResponse {
                    code: "bad_request".to_string(),
                    message: error.to_string(),
                }),
            };
            if let Err(error) = write_server_message(&mut send, &response).await {
                warn!(%error, "failed to write rendezvous response");
            }
            let _ = send.finish();
        });
    }

    Ok(())
}

async fn handle_message(
    message: ClientMessage,
    registry: Arc<tokio::sync::Mutex<Registry>>,
) -> ServerMessage {
    let now = Instant::now();
    match message {
        ClientMessage::Register(request) => match registry.lock().await.register(now, request) {
            Ok(response) => ServerMessage::Registered(response),
            Err(error) => registry_error_response(error),
        },
        ClientMessage::Lookup(request) => {
            match registry.lock().await.lookup_request(now, request) {
                Ok(response) => ServerMessage::LookupResult(response),
                Err(error) => registry_error_response(error),
            }
        }
        ClientMessage::Notify(request) => match registry.lock().await.queue_notify(now, request) {
            Ok(response) => ServerMessage::NotifyQueued(response),
            Err(error) => registry_error_response(error),
        },
        ClientMessage::Ping => ServerMessage::Pong,
    }
}

fn registry_error_response(error: RegistryError) -> ServerMessage {
    ServerMessage::Error(ErrorResponse {
        code: "registry_error".to_string(),
        message: error.to_string(),
    })
}

async fn read_client_message(recv: &mut quinn::RecvStream) -> Result<ClientMessage> {
    let bytes = read_frame(recv).await?;
    serde_json::from_slice(&bytes).context("decode rendezvous client message")
}

async fn write_server_message(send: &mut quinn::SendStream, message: &ServerMessage) -> Result<()> {
    let bytes = serde_json::to_vec(message).context("encode rendezvous server message")?;
    write_frame(send, &bytes).await
}

#[cfg(test)]
async fn write_client_message(send: &mut quinn::SendStream, message: &ClientMessage) -> Result<()> {
    let bytes = serde_json::to_vec(message).context("encode rendezvous client message")?;
    write_frame(send, &bytes).await
}

#[cfg(test)]
async fn read_server_message(recv: &mut quinn::RecvStream) -> Result<ServerMessage> {
    let bytes = read_frame(recv).await?;
    serde_json::from_slice(&bytes).context("decode rendezvous server message")
}

async fn read_frame(recv: &mut quinn::RecvStream) -> Result<Vec<u8>> {
    let mut len = [0_u8; 4];
    recv.read_exact(&mut len).await.context("read rendezvous frame length")?;
    let len = u32::from_be_bytes(len) as usize;
    if len > MAX_FRAME_BYTES {
        anyhow::bail!("rendezvous frame exceeds {MAX_FRAME_BYTES} bytes");
    }

    let mut bytes = vec![0_u8; len];
    recv.read_exact(&mut bytes).await.context("read rendezvous frame body")?;
    Ok(bytes)
}

async fn write_frame(send: &mut quinn::SendStream, bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_FRAME_BYTES {
        anyhow::bail!("rendezvous frame exceeds {MAX_FRAME_BYTES} bytes");
    }

    send.write_all(&(bytes.len() as u32).to_be_bytes())
        .await
        .context("write rendezvous frame length")?;
    send.write_all(bytes).await.context("write rendezvous frame body")
}

fn rendezvous_server_config() -> Result<quinn::ServerConfig> {
    let certified = rcgen::generate_simple_self_signed(["localhost".to_string()])
        .context("generate rendezvous self-signed certificate")?;
    let cert_chain = vec![CertificateDer::from(certified.cert.der().to_vec())];
    let key_der =
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(certified.key_pair.serialize_der()));
    let mut server_crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, key_der)
        .context("build rendezvous TLS config")?;
    server_crypto.alpn_protocols = vec![RENDEZVOUS_ALPN.to_vec()];
    let quic_crypto =
        QuicServerConfig::try_from(server_crypto).context("build rendezvous QUIC TLS config")?;
    Ok(quinn::ServerConfig::with_crypto(Arc::new(quic_crypto)))
}

fn ensure_ring_crypto_provider() {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::sync::Arc;

    use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
    use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
    use rustls::{DigitallySignedStruct, SignatureScheme};

    use crate::protocol::{
        Candidate, CandidateKind, ClientMessage, LookupRequest, LookupResponse, RegisterRequest,
        ServerMessage,
    };

    use super::*;

    #[tokio::test]
    async fn server_register_then_lookup_returns_peer() {
        let server = RendezvousServer::bind_test().await.unwrap();
        let client = TestClient::connect(server.local_addr()).await.unwrap();

        client.register(register_request("alice", [1; 32], 60)).await.unwrap();
        let response = client.lookup([1; 32], [1; 32]).await.unwrap();

        assert_eq!(response.peer.unwrap().alias, "alice");

        client.close().await;
        server.stop().await.unwrap();
    }

    fn register_request(alias: &str, pubkey: [u8; 32], ttl_seconds: u64) -> RegisterRequest {
        RegisterRequest {
            alias: alias.to_string(),
            pubkey,
            candidates: vec![Candidate {
                kind: CandidateKind::Local,
                address: "127.0.0.1:53400".to_string(),
                priority: 100,
            }],
            ttl_seconds,
        }
    }

    struct TestClient {
        endpoint: quinn::Endpoint,
        connection: quinn::Connection,
    }

    impl TestClient {
        async fn connect(addr: SocketAddr) -> Result<Self> {
            ensure_ring_crypto_provider();
            let mut rustls_config = rustls::ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(TrustAnyServer))
                .with_no_client_auth();
            rustls_config.alpn_protocols = vec![RENDEZVOUS_ALPN.to_vec()];
            let quic_crypto = quinn::crypto::rustls::QuicClientConfig::try_from(rustls_config)
                .context("build test rendezvous client TLS config")?;
            let client_config = quinn::ClientConfig::new(Arc::new(quic_crypto));
            let mut endpoint = quinn::Endpoint::client(SocketAddr::from(([0, 0, 0, 0], 0)))
                .context("bind test rendezvous client")?;
            endpoint.set_default_client_config(client_config);
            let connection = endpoint
                .connect(addr, "localhost")
                .context("connect test rendezvous client")?
                .await
                .context("establish test rendezvous client")?;
            Ok(Self { endpoint, connection })
        }

        async fn register(&self, request: RegisterRequest) -> Result<()> {
            let response = self.request(ClientMessage::Register(request)).await?;
            match response {
                ServerMessage::Registered(_) => Ok(()),
                other => anyhow::bail!("unexpected register response: {other:?}"),
            }
        }

        async fn lookup(
            &self,
            requester_pubkey: [u8; 32],
            target_pubkey: [u8; 32],
        ) -> Result<LookupResponse> {
            let response = self
                .request(ClientMessage::Lookup(LookupRequest { requester_pubkey, target_pubkey }))
                .await?;
            match response {
                ServerMessage::LookupResult(response) => Ok(response),
                other => anyhow::bail!("unexpected lookup response: {other:?}"),
            }
        }

        async fn request(&self, message: ClientMessage) -> Result<ServerMessage> {
            let (mut send, mut recv) =
                self.connection.open_bi().await.context("open rendezvous test stream")?;
            write_client_message(&mut send, &message).await?;
            send.finish().context("finish rendezvous test request")?;
            read_server_message(&mut recv).await
        }

        async fn close(self) {
            self.connection.close(0_u32.into(), b"test done");
            self.endpoint.wait_idle().await;
        }
    }

    #[derive(Debug)]
    struct TrustAnyServer;

    impl ServerCertVerifier for TrustAnyServer {
        fn verify_server_cert(
            &self,
            _end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _server_name: &ServerName<'_>,
            _ocsp_response: &[u8],
            _now: UnixTime,
        ) -> std::result::Result<ServerCertVerified, rustls::Error> {
            Ok(ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &DigitallySignedStruct,
        ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn verify_tls13_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &DigitallySignedStruct,
        ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            vec![
                SignatureScheme::ECDSA_NISTP256_SHA256,
                SignatureScheme::ED25519,
                SignatureScheme::RSA_PSS_SHA256,
            ]
        }
    }
}
