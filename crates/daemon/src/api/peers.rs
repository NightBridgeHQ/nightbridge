use std::sync::Arc;
use std::time::Duration;

use lsi_core::trust::{Peer, PeerPolicy as CorePeerPolicy, TrustStore};
use lsi_proto::{
    common::v1::Fingerprint,
    peers::v1::{
        peers_service_server::PeersService, LanPeer, ListLanPeersRequest, ListLanPeersResponse,
        ListTrustedPeersRequest, ListTrustedPeersResponse, PeerPolicy, PeerProtocol, TrustedPeer,
    },
};
use lsi_protocol_localsend_v2::discovery::DiscoveryBrowser;
use tonic::{Request, Response, Status};

use crate::state::DaemonState;

#[derive(Clone)]
pub(crate) struct PeersApi {
    state: Arc<DaemonState>,
}

impl PeersApi {
    pub(crate) fn new(state: Arc<DaemonState>) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl PeersService for PeersApi {
    async fn list_trusted(
        &self,
        _request: Request<ListTrustedPeersRequest>,
    ) -> Result<Response<ListTrustedPeersResponse>, Status> {
        let store = TrustStore::open(&self.state.trust_db_path).map_err(internal_status)?;
        let peers = store.list().map_err(internal_status)?.into_iter().map(trusted_peer).collect();
        Ok(Response::new(ListTrustedPeersResponse { peers }))
    }

    async fn list_lan(
        &self,
        request: Request<ListLanPeersRequest>,
    ) -> Result<Response<ListLanPeersResponse>, Status> {
        let timeout_ms = request.into_inner().timeout_ms;
        let timeout = if timeout_ms == 0 { 1500 } else { timeout_ms };
        let browser = DiscoveryBrowser::new(&self.state.fingerprint)
            .with_timeout(Duration::from_millis(u64::from(timeout)));
        let peers = match browser.listen_once().await.map_err(internal_status)? {
            Some(peer) => vec![LanPeer {
                alias: peer.info.alias,
                address: peer.address.ip().to_string(),
                port: u32::from(peer.address.port()),
                protocol: PeerProtocol::LocalsendV2 as i32,
                fingerprint: Some(Fingerprint { value: peer.info.fingerprint }),
                device_model: peer.info.device_model,
                device_type: peer.info.device_type,
                download: peer.info.download,
                extensions: Vec::new(),
            }],
            None => Vec::new(),
        };

        Ok(Response::new(ListLanPeersResponse { peers }))
    }
}

fn trusted_peer(peer: Peer) -> TrustedPeer {
    TrustedPeer {
        fingerprint: Some(Fingerprint { value: peer.fingerprint.to_string() }),
        pubkey: peer.pubkey.to_vec(),
        label: peer.label,
        trusted_at_unix_seconds: peer.trusted_at,
        last_seen_unix_seconds: peer.last_seen,
        policy: peer_policy(peer.policy) as i32,
        native_certificate_fingerprint: peer.native_certificate_fingerprint,
    }
}

fn peer_policy(policy: CorePeerPolicy) -> PeerPolicy {
    match policy {
        CorePeerPolicy::AutoAccept => PeerPolicy::AutoAccept,
        CorePeerPolicy::Prompt => PeerPolicy::Prompt,
        CorePeerPolicy::Block => PeerPolicy::Block,
    }
}

fn internal_status(error: impl std::fmt::Display) -> Status {
    Status::internal(error.to_string())
}
