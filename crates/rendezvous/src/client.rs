use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};

use crate::protocol::{
    ClientMessage, ErrorResponse, LookupRequest, LookupResponse, NotifyQueuedResponse,
    NotifyRequest, RegisterRequest, RegisteredResponse, ServerMessage,
};
use crate::server::{
    ensure_ring_crypto_provider, read_server_message, write_client_message, RENDEZVOUS_ALPN,
};

/// QUIC client for a self-hosted rendezvous server.
#[derive(Debug)]
pub struct RendezvousClient {
    endpoint: quinn::Endpoint,
    server_addr: SocketAddr,
    server_name: String,
    connection: quinn::Connection,
}

impl RendezvousClient {
    /// Connects to a rendezvous URL such as `quic://127.0.0.1:53410`.
    pub async fn connect(url: &str) -> Result<Self> {
        let server_addr = parse_rendezvous_url(url)?;
        let server_name = "localhost".to_string();
        let endpoint = rendezvous_client_endpoint()?;
        let connection = endpoint
            .connect(server_addr, &server_name)
            .context("connect rendezvous client")?
            .await
            .context("establish rendezvous client connection")?;

        Ok(Self { endpoint, server_addr, server_name, connection })
    }

    /// Registers or refreshes this daemon's current candidates.
    pub async fn register(&self, request: RegisterRequest) -> Result<RegisteredResponse> {
        match self.request(ClientMessage::Register(request)).await? {
            ServerMessage::Registered(response) => Ok(response),
            ServerMessage::Error(error) => Err(server_error(error)),
            other => anyhow::bail!("unexpected rendezvous register response: {other:?}"),
        }
    }

    /// Looks up a target peer.
    pub async fn lookup(&self, request: LookupRequest) -> Result<LookupResponse> {
        match self.request(ClientMessage::Lookup(request)).await? {
            ServerMessage::LookupResult(response) => Ok(response),
            ServerMessage::Error(error) => Err(server_error(error)),
            other => anyhow::bail!("unexpected rendezvous lookup response: {other:?}"),
        }
    }

    /// Queues a notification for a target peer.
    pub async fn notify(&self, request: NotifyRequest) -> Result<NotifyQueuedResponse> {
        match self.request(ClientMessage::Notify(request)).await? {
            ServerMessage::NotifyQueued(response) => Ok(response),
            ServerMessage::Error(error) => Err(server_error(error)),
            other => anyhow::bail!("unexpected rendezvous notify response: {other:?}"),
        }
    }

    /// Returns the remote rendezvous server socket address.
    pub fn server_addr(&self) -> SocketAddr {
        self.server_addr
    }

    /// Returns the TLS server name used by the client.
    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    /// Closes the client connection and waits for the endpoint to drain.
    pub async fn close(self) {
        self.connection.close(0_u32.into(), b"rendezvous client shutdown");
        self.endpoint.wait_idle().await;
    }

    async fn request(&self, message: ClientMessage) -> Result<ServerMessage> {
        let (mut send, mut recv) =
            self.connection.open_bi().await.context("open rendezvous stream")?;
        write_client_message(&mut send, &message).await?;
        send.finish().context("finish rendezvous request")?;
        read_server_message(&mut recv).await
    }
}

fn parse_rendezvous_url(url: &str) -> Result<SocketAddr> {
    let Some(address) = url.strip_prefix("quic://") else {
        anyhow::bail!("rendezvous URL must start with quic://");
    };
    address.parse::<SocketAddr>().context("rendezvous URL must be quic://host:port")
}

fn rendezvous_client_endpoint() -> Result<quinn::Endpoint> {
    ensure_ring_crypto_provider();
    let mut rustls_config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(TrustAnyServer))
        .with_no_client_auth();
    rustls_config.alpn_protocols = vec![RENDEZVOUS_ALPN.to_vec()];
    let quic_crypto = quinn::crypto::rustls::QuicClientConfig::try_from(rustls_config)
        .context("build rendezvous client TLS config")?;
    let client_config = quinn::ClientConfig::new(Arc::new(quic_crypto));
    let mut endpoint = quinn::Endpoint::client(SocketAddr::from(([0, 0, 0, 0], 0)))
        .context("bind rendezvous client")?;
    endpoint.set_default_client_config(client_config);
    Ok(endpoint)
}

fn server_error(error: ErrorResponse) -> anyhow::Error {
    anyhow::anyhow!("rendezvous server error {}: {}", error.code, error.message)
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

#[cfg(test)]
mod tests {
    use crate::protocol::{
        Candidate, CandidateKind, LookupRequest, NotifyRequest, RegisterRequest,
    };
    use crate::server::RendezvousServer;

    use super::*;

    #[tokio::test]
    async fn client_connects_to_local_test_server() {
        let server = RendezvousServer::bind_test().await.unwrap();

        let client =
            RendezvousClient::connect(&format!("quic://{}", server.local_addr())).await.unwrap();

        client.close().await;
        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn client_register_returns_expiry() {
        let server = RendezvousServer::bind_test().await.unwrap();
        let client =
            RendezvousClient::connect(&format!("quic://{}", server.local_addr())).await.unwrap();

        let response = client.register(register_request("alice", [1; 32], 60)).await.unwrap();

        assert_eq!(response.ttl_seconds, 60);
        assert!(response.expires_at_unix_seconds > 0);

        client.close().await;
        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn client_lookup_missing_peer_returns_empty_lookup() {
        let server = RendezvousServer::bind_test().await.unwrap();
        let client =
            RendezvousClient::connect(&format!("quic://{}", server.local_addr())).await.unwrap();

        let response = client
            .lookup(LookupRequest { requester_pubkey: [1; 32], target_pubkey: [2; 32] })
            .await
            .unwrap();

        assert!(response.peer.is_none());

        client.close().await;
        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn client_notify_returns_queued_response() {
        let server = RendezvousServer::bind_test().await.unwrap();
        let client =
            RendezvousClient::connect(&format!("quic://{}", server.local_addr())).await.unwrap();

        client.register(register_request("bob", [2; 32], 60)).await.unwrap();
        let response = client
            .notify(NotifyRequest { requester_pubkey: [1; 32], target_pubkey: [2; 32] })
            .await
            .unwrap();

        assert_eq!(response.target_pubkey, [2; 32]);

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
}
