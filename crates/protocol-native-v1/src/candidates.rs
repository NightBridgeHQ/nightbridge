//! WAN connection candidate model for native QUIC peers.

use std::cmp::Reverse;
use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::Duration;
use tracing::warn;

use crate::{NativeError, Result};

/// Candidate endpoint type.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum CandidateKind {
    /// Candidate reachable on the peer's local network.
    Local,
    /// Public endpoint discovered through STUN.
    ServerReflexive,
}

/// A possible UDP endpoint for native QUIC connectivity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeCandidate {
    /// Candidate source.
    pub kind: CandidateKind,
    /// UDP socket address to try.
    pub address: SocketAddr,
    /// Higher values are attempted first.
    pub priority: u32,
}

/// Builds local native QUIC candidates for a listener port.
pub fn local_candidates(native_port: u16) -> Result<Vec<NativeCandidate>> {
    if native_port == 0 {
        return Err(NativeError::Transport(
            "native candidate port must be greater than zero".to_string(),
        ));
    }

    let mut candidates = vec![NativeCandidate {
        kind: CandidateKind::Local,
        address: SocketAddr::from((Ipv4Addr::LOCALHOST, native_port)),
        priority: 100,
    }];

    if let Some(address) = primary_ipv4_address() {
        candidates.push(NativeCandidate {
            kind: CandidateKind::Local,
            address: SocketAddr::new(IpAddr::V4(address), native_port),
            priority: 90,
        });
    }

    let mut candidates = dedupe_candidates(candidates);
    sort_candidates(&mut candidates);
    Ok(candidates)
}

/// Gathers local and STUN-reflexive native QUIC candidates.
pub async fn gather_candidates(
    native_port: u16,
    stun_servers: &[SocketAddr],
) -> Result<Vec<NativeCandidate>> {
    gather_candidates_with_timeout(native_port, stun_servers, Duration::from_secs(3)).await
}

async fn gather_candidates_with_timeout(
    native_port: u16,
    stun_servers: &[SocketAddr],
    timeout: Duration,
) -> Result<Vec<NativeCandidate>> {
    let mut candidates = local_candidates(native_port)?;

    for stun_server in stun_servers {
        match crate::stun::discover_server_reflexive_candidate(*stun_server, native_port, timeout)
            .await
        {
            Ok(address) => candidates.push(NativeCandidate {
                kind: CandidateKind::ServerReflexive,
                address,
                priority: 80,
            }),
            Err(error) => {
                warn!(%error, server = %stun_server, "failed to discover STUN candidate");
            }
        }
    }

    let mut candidates = dedupe_candidates(candidates);
    sort_candidates(&mut candidates);
    Ok(candidates)
}

/// Removes duplicate candidates by kind and address, preserving first occurrence.
pub fn dedupe_candidates(candidates: Vec<NativeCandidate>) -> Vec<NativeCandidate> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::with_capacity(candidates.len());

    for candidate in candidates {
        if seen.insert((candidate.kind.clone(), candidate.address)) {
            deduped.push(candidate);
        }
    }

    deduped
}

/// Sorts candidates by descending priority with deterministic ties.
pub fn sort_candidates(candidates: &mut [NativeCandidate]) {
    candidates.sort_by_key(|candidate| {
        (Reverse(candidate.priority), candidate_kind_rank(&candidate.kind), candidate.address)
    });
}

fn candidate_kind_rank(kind: &CandidateKind) -> u8 {
    match kind {
        CandidateKind::Local => 0,
        CandidateKind::ServerReflexive => 1,
    }
}

fn primary_ipv4_address() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(198, 51, 100, 1), 9)).ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(address) if !address.is_loopback() && !address.is_unspecified() => Some(address),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use super::*;

    #[test]
    fn local_candidates_include_loopback_with_native_port() {
        let candidates = local_candidates(53400).unwrap();

        assert!(candidates.iter().any(|candidate| {
            candidate.kind == CandidateKind::Local
                && candidate.address == SocketAddr::from((Ipv4Addr::LOCALHOST, 53400))
        }));
    }

    #[test]
    fn local_candidates_reject_zero_port() {
        let error = local_candidates(0).unwrap_err();

        assert!(error.to_string().contains("native candidate port must be greater than zero"));
    }

    #[test]
    fn dedupe_candidates_keeps_first_matching_kind_and_address() {
        let duplicate = NativeCandidate {
            kind: CandidateKind::Local,
            address: SocketAddr::from((Ipv4Addr::LOCALHOST, 53400)),
            priority: 100,
        };
        let candidates = dedupe_candidates(vec![
            duplicate.clone(),
            NativeCandidate { priority: 10, ..duplicate.clone() },
        ]);

        assert_eq!(candidates, vec![duplicate]);
    }

    #[test]
    fn sort_candidates_orders_by_priority_descending() {
        let mut candidates = vec![
            NativeCandidate {
                kind: CandidateKind::ServerReflexive,
                address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10)), 53400),
                priority: 50,
            },
            NativeCandidate {
                kind: CandidateKind::Local,
                address: SocketAddr::from((Ipv4Addr::LOCALHOST, 53400)),
                priority: 100,
            },
        ];

        sort_candidates(&mut candidates);

        assert_eq!(candidates[0].kind, CandidateKind::Local);
        assert_eq!(candidates[0].priority, 100);
    }

    #[test]
    fn local_candidates_are_deduped_and_sorted() {
        let candidates = local_candidates(53400).unwrap();
        let mut sorted = candidates.clone();
        sort_candidates(&mut sorted);

        assert_eq!(candidates, sorted);
        assert_eq!(candidates.len(), dedupe_candidates(candidates.clone()).len());
    }

    #[tokio::test]
    async fn gather_candidates_returns_local_candidates_when_stun_is_empty() {
        let candidates = gather_candidates(53400, &[]).await.unwrap();

        assert!(candidates.iter().any(|candidate| {
            candidate.kind == CandidateKind::Local
                && candidate.address == SocketAddr::from((Ipv4Addr::LOCALHOST, 53400))
        }));
    }

    #[tokio::test]
    async fn gather_candidates_tolerates_stun_failure() {
        let stun_server = SocketAddr::from((Ipv4Addr::LOCALHOST, 9));
        let candidates =
            gather_candidates_with_timeout(53400, &[stun_server], Duration::from_millis(10))
                .await
                .unwrap();

        assert!(candidates.iter().any(|candidate| candidate.kind == CandidateKind::Local));
    }
}
