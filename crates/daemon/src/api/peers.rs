use std::sync::Arc;

use lsi_core::trust::{
    LocalSendLanPeer as CoreLocalSendLanPeer, LocalSendPeer as CoreLocalSendPeer,
    LocalSendPeerStatus as CoreLocalSendPeerStatus, Peer, PeerPolicy as CorePeerPolicy, TrustStore,
};
use lsi_proto::{
    common::v1::Fingerprint,
    peers::v1::{
        peers_service_server::PeersService, ApproveLocalSendPeerRequest, DenyLocalSendPeerRequest,
        LanPeer, ListLanPeersRequest, ListLanPeersResponse, ListPendingLocalSendPeersRequest,
        ListPendingLocalSendPeersResponse, ListTrustedPeersRequest, ListTrustedPeersResponse,
        LocalSendPeer, LocalSendPeerResponse, LocalSendPeerStatus, PeerPolicy, PeerProtocol,
        TrustNativeLanPeerRequest, TrustedPeer, TrustedPeerResponse,
    },
};
use tonic::{Request, Response, Status};

use crate::native_lan::NativeLanPeer;
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
        _request: Request<ListLanPeersRequest>,
    ) -> Result<Response<ListLanPeersResponse>, Status> {
        let peers = crate::localsend_lan::scan_and_cache(&self.state.trust_db_path)
            .await
            .map_err(internal_status)?
            .into_iter()
            .map(localsend_lan_peer)
            .collect();

        Ok(Response::new(ListLanPeersResponse { peers }))
    }

    async fn list_native_lan(
        &self,
        request: Request<ListLanPeersRequest>,
    ) -> Result<Response<ListLanPeersResponse>, Status> {
        let timeout = discovery_timeout(request.into_inner().timeout_ms);
        let peers = crate::native_lan::discover(timeout, &self.state.fingerprint)
            .await
            .map_err(internal_status)?
            .into_iter()
            .map(native_lan_peer)
            .collect();

        Ok(Response::new(ListLanPeersResponse { peers }))
    }

    async fn trust_native_lan(
        &self,
        request: Request<TrustNativeLanPeerRequest>,
    ) -> Result<Response<TrustedPeerResponse>, Status> {
        let request = request.into_inner();
        let policy = match PeerPolicy::try_from(request.policy).unwrap_or(PeerPolicy::AutoAccept) {
            PeerPolicy::AutoAccept | PeerPolicy::Unspecified => CorePeerPolicy::AutoAccept,
            PeerPolicy::Prompt => CorePeerPolicy::Prompt,
            PeerPolicy::Block => CorePeerPolicy::Block,
        };
        let timeout = discovery_timeout(request.timeout_ms);
        let peers = crate::native_lan::discover(timeout, &self.state.fingerprint)
            .await
            .map_err(internal_status)?;
        let peer =
            crate::native_lan::resolve_peer_ref(peers, &request.peer).map_err(not_found_status)?;
        let trusted = crate::native_lan::trust_discovered_peer(
            &self.state.trust_db_path,
            &peer,
            request.label.as_deref(),
            policy,
        )
        .map_err(internal_status)?;

        Ok(Response::new(TrustedPeerResponse { peer: Some(trusted_peer(trusted)) }))
    }

    async fn list_pending_local_send(
        &self,
        _request: Request<ListPendingLocalSendPeersRequest>,
    ) -> Result<Response<ListPendingLocalSendPeersResponse>, Status> {
        let store = TrustStore::open(&self.state.trust_db_path).map_err(internal_status)?;
        let peers = store
            .list_pending_localsend_peers()
            .map_err(internal_status)?
            .into_iter()
            .map(localsend_peer)
            .collect();
        Ok(Response::new(ListPendingLocalSendPeersResponse { peers }))
    }

    async fn approve_local_send(
        &self,
        request: Request<ApproveLocalSendPeerRequest>,
    ) -> Result<Response<LocalSendPeerResponse>, Status> {
        let request = request.into_inner();
        let store = TrustStore::open(&self.state.trust_db_path).map_err(internal_status)?;
        let peer = store
            .approve_localsend_peer(&request.fingerprint, request.label.as_deref())
            .map_err(internal_status)?;
        Ok(Response::new(LocalSendPeerResponse { peer: Some(localsend_peer(peer)) }))
    }

    async fn deny_local_send(
        &self,
        request: Request<DenyLocalSendPeerRequest>,
    ) -> Result<Response<LocalSendPeerResponse>, Status> {
        let request = request.into_inner();
        let store = TrustStore::open(&self.state.trust_db_path).map_err(internal_status)?;
        let peer = store.deny_localsend_peer(&request.fingerprint).map_err(internal_status)?;
        Ok(Response::new(LocalSendPeerResponse { peer: Some(localsend_peer(peer)) }))
    }
}

fn localsend_lan_peer(peer: CoreLocalSendLanPeer) -> LanPeer {
    LanPeer {
        alias: peer.alias,
        address: peer.source_ip,
        port: u32::from(peer.port),
        protocol: PeerProtocol::LocalsendV2 as i32,
        fingerprint: Some(Fingerprint { value: peer.fingerprint }),
        device_model: peer.device_model,
        device_type: peer.device_type,
        download: peer.download,
        extensions: Vec::new(),
        native_pubkey: Vec::new(),
        native_certificate_fingerprint: None,
    }
}

fn native_lan_peer(peer: NativeLanPeer) -> LanPeer {
    LanPeer {
        alias: peer.alias,
        address: peer.address.to_string(),
        port: u32::from(peer.port),
        protocol: PeerProtocol::NativeV1 as i32,
        fingerprint: Some(Fingerprint { value: peer.fingerprint }),
        device_model: None,
        device_type: Some("server".to_string()),
        download: true,
        extensions: peer.extensions,
        native_pubkey: peer.pubkey.to_vec(),
        native_certificate_fingerprint: peer.native_certificate_fingerprint,
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

fn localsend_peer(peer: CoreLocalSendPeer) -> LocalSendPeer {
    LocalSendPeer {
        fingerprint: peer.fingerprint,
        alias: peer.alias,
        label: peer.label,
        status: localsend_peer_status(peer.status) as i32,
        first_seen_unix_seconds: peer.first_seen,
        last_seen_unix_seconds: peer.last_seen,
        attempt_count: peer.attempt_count,
        source_ip: peer.source_ip,
    }
}

fn localsend_peer_status(status: CoreLocalSendPeerStatus) -> LocalSendPeerStatus {
    match status {
        CoreLocalSendPeerStatus::Pending => LocalSendPeerStatus::LocalsendPeerStatusPending,
        CoreLocalSendPeerStatus::Trusted => LocalSendPeerStatus::LocalsendPeerStatusTrusted,
        CoreLocalSendPeerStatus::Blocked => LocalSendPeerStatus::LocalsendPeerStatusBlocked,
    }
}

fn internal_status(error: impl std::fmt::Display) -> Status {
    Status::internal(error.to_string())
}

fn not_found_status(error: impl std::fmt::Display) -> Status {
    Status::not_found(error.to_string())
}

fn discovery_timeout(timeout_ms: u32) -> std::time::Duration {
    let timeout_ms = if timeout_ms == 0 { 1500 } else { timeout_ms };
    std::time::Duration::from_millis(u64::from(timeout_ms))
}
