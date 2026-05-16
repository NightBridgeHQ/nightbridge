//! Candidate-pair dialer for WAN native QUIC connections.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use tokio::sync::mpsc;

use crate::candidates::NativeCandidate;
use crate::tls::ensure_ring_crypto_provider;
use crate::transport::{NativeTransportConfig, NATIVE_ALPN};
use crate::{NativeError, Result};

const DEFAULT_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(2);
const DEFAULT_STAGGER: Duration = Duration::from_millis(25);

/// One local/remote candidate pair to try for a direct WAN connection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidatePair {
    /// Local candidate selected for this attempt.
    pub local: NativeCandidate,
    /// Remote candidate selected for this attempt.
    pub remote: NativeCandidate,
    /// Higher values are attempted first.
    pub priority: u64,
}

/// Diagnostics collected while trying candidate pairs.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PunchDiagnostics {
    /// Candidate-pair descriptions attempted in priority order.
    pub attempted_pairs: Vec<String>,
    /// Last connection error observed, if any.
    pub last_error: Option<String>,
}

/// Successful direct candidate connection.
#[derive(Debug)]
pub struct DialedCandidateConnection {
    /// Client endpoint that owns the connection.
    pub endpoint: quinn::Endpoint,
    /// Established QUIC connection.
    pub connection: quinn::Connection,
    /// Diagnostics captured before the winning attempt completed.
    pub diagnostics: PunchDiagnostics,
}

/// Builds candidate pairs ordered by descending combined priority.
pub fn candidate_pairs(
    local_candidates: &[NativeCandidate],
    remote_candidates: &[NativeCandidate],
) -> Vec<CandidatePair> {
    let mut pairs = local_candidates
        .iter()
        .flat_map(|local| {
            remote_candidates.iter().map(move |remote| CandidatePair {
                local: local.clone(),
                remote: remote.clone(),
                priority: combined_priority(local.priority, remote.priority),
            })
        })
        .collect::<Vec<_>>();
    pairs.sort_by(|a, b| {
        b.priority.cmp(&a.priority).then_with(|| a.remote.address.cmp(&b.remote.address))
    });
    pairs
}

/// Dials remote candidates using a small stagger between concurrent attempts.
pub async fn dial_candidates(
    local_candidates: Vec<NativeCandidate>,
    remote_candidates: Vec<NativeCandidate>,
) -> Result<DialedCandidateConnection> {
    dial_candidates_with_timing(
        local_candidates,
        remote_candidates,
        DEFAULT_ATTEMPT_TIMEOUT,
        DEFAULT_STAGGER,
    )
    .await
}

async fn dial_candidates_with_timing(
    local_candidates: Vec<NativeCandidate>,
    remote_candidates: Vec<NativeCandidate>,
    attempt_timeout: Duration,
    stagger: Duration,
) -> Result<DialedCandidateConnection> {
    let pairs = candidate_pairs(&local_candidates, &remote_candidates);
    if pairs.is_empty() {
        return Err(NativeError::NoDirectPath("no WAN candidate pairs available".to_string()));
    }

    let mut diagnostics = PunchDiagnostics {
        attempted_pairs: pairs.iter().map(describe_pair).collect(),
        last_error: None,
    };
    let (tx, mut rx) = mpsc::channel(pairs.len());

    for (index, pair) in pairs.iter().cloned().enumerate() {
        let tx = tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(stagger.saturating_mul(index as u32)).await;
            let result = dial_pair(pair.clone(), attempt_timeout).await;
            let _ = tx.send((describe_pair(&pair), result)).await;
        });
    }
    drop(tx);

    while let Some((_description, result)) = rx.recv().await {
        match result {
            Ok((endpoint, connection)) => {
                return Ok(DialedCandidateConnection { endpoint, connection, diagnostics });
            }
            Err(error) => {
                diagnostics.last_error = Some(error.to_string());
            }
        }
    }

    Err(NativeError::NoDirectPath(format!(
        "all WAN candidate pairs failed; attempted={:?}; last_error={}",
        diagnostics.attempted_pairs,
        diagnostics.last_error.as_deref().unwrap_or("none")
    )))
}

async fn dial_pair(
    pair: CandidatePair,
    attempt_timeout: Duration,
) -> Result<(quinn::Endpoint, quinn::Connection)> {
    let endpoint = native_client_endpoint(pair.local.address)?;
    let connecting = endpoint
        .connect(pair.remote.address, "localhost")
        .map_err(|error| NativeError::Transport(error.to_string()))?;
    let connection = tokio::time::timeout(attempt_timeout, connecting)
        .await
        .map_err(|_| NativeError::Transport("candidate dial timed out".to_string()))?
        .map_err(|error| NativeError::Transport(error.to_string()))?;
    Ok((endpoint, connection))
}

fn native_client_endpoint(bind_addr: SocketAddr) -> Result<quinn::Endpoint> {
    ensure_ring_crypto_provider();
    let mut rustls_config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(TrustAnyServer))
        .with_no_client_auth();
    rustls_config.alpn_protocols = vec![NATIVE_ALPN.to_vec()];
    let quic_crypto = quinn::crypto::rustls::QuicClientConfig::try_from(rustls_config)
        .map_err(|error| NativeError::Transport(error.to_string()))?;
    let client_config = NativeTransportConfig::default()
        .apply_to_client_config(quinn::ClientConfig::new(Arc::new(quic_crypto)))?;
    let mut endpoint = quinn::Endpoint::client(bind_addr)?;
    endpoint.set_default_client_config(client_config);
    Ok(endpoint)
}

fn combined_priority(local: u32, remote: u32) -> u64 {
    (u64::from(local) << 32) | u64::from(remote)
}

fn describe_pair(pair: &CandidatePair) -> String {
    format!(
        "{:?} {} -> {:?} {}",
        pair.local.kind, pair.local.address, pair.remote.kind, pair.remote.address
    )
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
    use std::net::Ipv4Addr;

    use crate::candidates::{CandidateKind, NativeCandidate};
    use crate::tls::NativeTlsIdentity;

    use super::*;

    #[test]
    fn candidate_pairs_are_ordered_by_combined_priority() {
        let local = vec![
            candidate(CandidateKind::Local, 127, 54001, 10),
            candidate(CandidateKind::Local, 127, 54002, 30),
        ];
        let remote = vec![
            candidate(CandidateKind::ServerReflexive, 203, 54003, 20),
            candidate(CandidateKind::ServerReflexive, 203, 54004, 40),
        ];

        let pairs = candidate_pairs(&local, &remote);

        assert_eq!(pairs[0].local.priority, 30);
        assert_eq!(pairs[0].remote.priority, 40);
        assert!(pairs[0].priority > pairs[1].priority);
    }

    #[tokio::test]
    async fn loopback_candidate_pair_can_connect_to_local_native_listener() {
        let server = loopback_server().await;
        let accept_endpoint = server.clone();
        let accept_task = tokio::spawn(async move {
            if let Some(incoming) = accept_endpoint.accept().await {
                if let Ok(connection) = incoming.await {
                    connection.closed().await;
                }
            }
        });
        let remote = candidate_at(CandidateKind::Local, server.local_addr().unwrap(), 100);
        let local = candidate(CandidateKind::Local, 127, 0, 100);

        let dialed = dial_candidates_with_timing(
            vec![local],
            vec![remote],
            Duration::from_secs(1),
            Duration::from_millis(1),
        )
        .await
        .unwrap();

        assert_eq!(dialed.connection.remote_address(), server.local_addr().unwrap());
        assert_eq!(dialed.diagnostics.attempted_pairs.len(), 1);

        dialed.connection.close(0_u32.into(), b"test done");
        dialed.endpoint.close(0_u32.into(), b"test done");
        server.close(0_u32.into(), b"test done");
        accept_task.await.unwrap();
    }

    #[tokio::test]
    async fn failed_candidates_are_reported_in_diagnostics() {
        let local = candidate(CandidateKind::Local, 127, 0, 100);
        let remote = candidate(CandidateKind::ServerReflexive, 127, 9, 100);

        let error = dial_candidates_with_timing(
            vec![local],
            vec![remote],
            Duration::from_millis(10),
            Duration::from_millis(1),
        )
        .await
        .unwrap_err();

        let message = error.to_string();
        assert!(message.contains("attempted="), "{message}");
        assert!(message.contains("127.0.0.1:9"), "{message}");
        assert!(message.contains("last_error="), "{message}");
    }

    #[tokio::test]
    async fn all_failures_return_no_direct_path() {
        let local = candidate(CandidateKind::Local, 127, 0, 100);
        let remote = candidate(CandidateKind::ServerReflexive, 127, 9, 100);

        let error = dial_candidates_with_timing(
            vec![local],
            vec![remote],
            Duration::from_millis(10),
            Duration::from_millis(1),
        )
        .await
        .unwrap_err();

        assert!(matches!(error, NativeError::NoDirectPath(_)));
    }

    async fn loopback_server() -> quinn::Endpoint {
        let tls_identity = NativeTlsIdentity::generate("hole-punch-test").unwrap();
        let server_config = NativeTransportConfig::default()
            .apply_to_server_config(tls_identity.quinn_server_config().unwrap())
            .unwrap();
        quinn::Endpoint::server(server_config, SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).unwrap()
    }

    fn candidate(
        kind: CandidateKind,
        first_octet: u8,
        port: u16,
        priority: u32,
    ) -> NativeCandidate {
        candidate_at(kind, SocketAddr::from((Ipv4Addr::new(first_octet, 0, 0, 1), port)), priority)
    }

    fn candidate_at(kind: CandidateKind, address: SocketAddr, priority: u32) -> NativeCandidate {
        NativeCandidate { kind, address, priority }
    }
}
