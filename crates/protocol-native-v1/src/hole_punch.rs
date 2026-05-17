//! Candidate-pair dialer for WAN native QUIC connections.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

use crate::candidates::{CandidateKind, NativeCandidate};
use crate::tls::{ensure_ring_crypto_provider, NativeServerVerifier};
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

/// Expected remote certificate identity for native WAN candidate dialing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeDialTrust {
    expected_certificate_fingerprint: String,
}

impl NativeDialTrust {
    /// Create a trust config from the expected server certificate fingerprint.
    pub fn new(expected_certificate_fingerprint: impl Into<String>) -> Self {
        Self { expected_certificate_fingerprint: expected_certificate_fingerprint.into() }
    }

    /// Return the expected server certificate fingerprint.
    pub fn expected_certificate_fingerprint(&self) -> String {
        self.expected_certificate_fingerprint.clone()
    }
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
    trust: NativeDialTrust,
) -> Result<DialedCandidateConnection> {
    dial_candidates_with_timing(
        local_candidates,
        remote_candidates,
        trust,
        DEFAULT_ATTEMPT_TIMEOUT,
        DEFAULT_STAGGER,
    )
    .await
}

async fn dial_candidates_with_timing(
    local_candidates: Vec<NativeCandidate>,
    remote_candidates: Vec<NativeCandidate>,
    trust: NativeDialTrust,
    attempt_timeout: Duration,
    stagger: Duration,
) -> Result<DialedCandidateConnection> {
    let pairs = candidate_pairs(&local_candidates, &remote_candidates);
    if pairs.is_empty() {
        return Err(no_direct_path_error(Vec::new(), false, None));
    }

    let mut diagnostics = PunchDiagnostics {
        attempted_pairs: pairs.iter().map(describe_pair).collect(),
        last_error: None,
    };
    let (tx, mut rx) = mpsc::channel(pairs.len());

    for (index, pair) in pairs.iter().cloned().enumerate() {
        let tx = tx.clone();
        let trust = trust.clone();
        tokio::spawn(async move {
            tokio::time::sleep(stagger.saturating_mul(index as u32)).await;
            let result = dial_pair(pair.clone(), trust, attempt_timeout).await;
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

    Err(no_direct_path_error(
        diagnostics.attempted_pairs,
        only_server_reflexive_remote_candidates(&pairs),
        diagnostics.last_error,
    ))
}

async fn dial_pair(
    pair: CandidatePair,
    trust: NativeDialTrust,
    attempt_timeout: Duration,
) -> Result<(quinn::Endpoint, quinn::Connection)> {
    let endpoint = native_client_endpoint(pair.local.address, trust)?;
    let connecting = endpoint
        .connect(pair.remote.address, "localhost")
        .map_err(|error| NativeError::Transport(error.to_string()))?;
    let connection = tokio::time::timeout(attempt_timeout, connecting)
        .await
        .map_err(|_| NativeError::Transport("candidate dial timed out".to_string()))?
        .map_err(|error| NativeError::Transport(error.to_string()))?;
    Ok((endpoint, connection))
}

fn native_client_endpoint(
    bind_addr: SocketAddr,
    trust: NativeDialTrust,
) -> Result<quinn::Endpoint> {
    ensure_ring_crypto_provider();
    let mut rustls_config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NativeServerVerifier::new(
            trust.expected_certificate_fingerprint(),
        )))
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

fn no_direct_path_error(
    attempted_pairs: Vec<String>,
    only_server_reflexive_failed: bool,
    last_error: Option<String>,
) -> NativeError {
    let hint = if only_server_reflexive_failed {
        "symmetric NAT or firewall likely. Relay is not available in the open-base v1 build."
            .to_string()
    } else {
        "firewall or routing policy likely. Relay is not available in the open-base v1 build."
            .to_string()
    };
    NativeError::NoDirectPath { attempted_pairs, hint, last_error }
}

fn only_server_reflexive_remote_candidates(pairs: &[CandidatePair]) -> bool {
    !pairs.is_empty() && pairs.iter().all(|pair| pair.remote.kind == CandidateKind::ServerReflexive)
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

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
        let (server, server_fingerprint) = loopback_server().await;
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
        let trust = NativeDialTrust::new(server_fingerprint);

        let dialed = dial_candidates_with_timing(
            vec![local],
            vec![remote],
            trust,
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
            NativeDialTrust::new("unused"),
            Duration::from_millis(10),
            Duration::from_millis(1),
        )
        .await
        .unwrap_err();

        let message = error.to_string();
        assert!(message.contains("attempted_pairs="), "{message}");
        assert!(message.contains("127.0.0.1:9"), "{message}");
        let NativeError::NoDirectPath { last_error, .. } = error else {
            panic!("expected NoDirectPath");
        };
        assert!(last_error.is_some());
    }

    #[tokio::test]
    async fn all_failures_return_no_direct_path() {
        let local = candidate(CandidateKind::Local, 127, 0, 100);
        let remote = candidate(CandidateKind::ServerReflexive, 127, 9, 100);

        let error = dial_candidates_with_timing(
            vec![local],
            vec![remote],
            NativeDialTrust::new("unused"),
            Duration::from_millis(10),
            Duration::from_millis(1),
        )
        .await
        .unwrap_err();

        assert!(matches!(error, NativeError::NoDirectPath { .. }));
    }

    #[tokio::test]
    async fn diagnostics_include_no_direct_wan_path_message() {
        let error = failed_server_reflexive_dial_error().await;

        assert!(error.to_string().contains("no direct WAN path"), "{error}");
    }

    #[tokio::test]
    async fn diagnostics_mention_relay_not_available_in_base_v1() {
        let error = failed_server_reflexive_dial_error().await;

        assert!(error.to_string().contains("Relay is not available"), "{error}");
        assert!(error.to_string().contains("open-base v1"), "{error}");
    }

    #[tokio::test]
    async fn diagnostics_hint_symmetric_nat_for_server_reflexive_failures() {
        let error = failed_server_reflexive_dial_error().await;

        assert!(error.to_string().contains("symmetric NAT"), "{error}");
    }

    async fn loopback_server() -> (quinn::Endpoint, String) {
        let tls_identity = NativeTlsIdentity::generate("hole-punch-test").unwrap();
        let fingerprint = tls_identity.fingerprint_sha256_hex();
        let server_config = NativeTransportConfig::default()
            .apply_to_server_config(tls_identity.quinn_server_config().unwrap())
            .unwrap();
        let endpoint =
            quinn::Endpoint::server(server_config, SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
                .unwrap();
        (endpoint, fingerprint)
    }

    #[test]
    fn dialer_requires_expected_certificate_fingerprint() {
        let expected = "abcd".to_string();
        let config = NativeDialTrust::new(expected.clone());

        assert_eq!(config.expected_certificate_fingerprint(), expected);
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

    async fn failed_server_reflexive_dial_error() -> NativeError {
        let local = candidate(CandidateKind::Local, 127, 0, 100);
        let remote = candidate(CandidateKind::ServerReflexive, 127, 9, 100);
        dial_candidates_with_timing(
            vec![local],
            vec![remote],
            NativeDialTrust::new("unused"),
            Duration::from_millis(10),
            Duration::from_millis(1),
        )
        .await
        .unwrap_err()
    }
}
