use std::sync::Arc;

use lsi_core::identity::Fingerprint;
use lsi_core::trust::{LocalSendLanPeer, Peer, PeerPolicy, TrustStore};
use lsi_proto::transfers::v1::{
    send_request, transfers_service_server::TransfersService, CancelRequest, CancelResponse,
    ListActiveTransfersRequest, ListActiveTransfersResponse, ResumeRequest, ResumeResponse,
    SendRequest, SendResponse, TransferState,
};
use lsi_protocol_localsend_v2::{
    client::LocalSendClient,
    dto::{DeviceInfo, Protocol},
};
use lsi_protocol_native_v1::client::NativeTransferClient;
use tonic::{Request, Response, Status};
use uuid::Uuid;

use crate::events::DaemonEvent;
use crate::state::DaemonState;
use crate::wan;

#[derive(Clone)]
pub(crate) struct TransfersApi {
    state: Arc<DaemonState>,
}

impl TransfersApi {
    pub(crate) fn new(state: Arc<DaemonState>) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl TransfersService for TransfersApi {
    async fn list_active(
        &self,
        _request: Request<ListActiveTransfersRequest>,
    ) -> Result<Response<ListActiveTransfersResponse>, Status> {
        Ok(Response::new(ListActiveTransfersResponse { transfers: Vec::new() }))
    }

    async fn send(&self, request: Request<SendRequest>) -> Result<Response<SendResponse>, Status> {
        let transfer_id = send_files(&self.state, request.into_inner()).await?;
        Ok(Response::new(SendResponse { transfer_id }))
    }

    async fn resume(
        &self,
        request: Request<ResumeRequest>,
    ) -> Result<Response<ResumeResponse>, Status> {
        let transfer_id = request.into_inner().transfer_id;
        Err(Status::unimplemented(format!(
            "resume is not wired to daemon API yet for transfer {transfer_id}"
        )))
    }

    async fn cancel(
        &self,
        request: Request<CancelRequest>,
    ) -> Result<Response<CancelResponse>, Status> {
        let transfer_id = request.into_inner().transfer_id;
        Ok(Response::new(CancelResponse { transfer_id, state: TransferState::Unspecified as i32 }))
    }
}

pub(crate) async fn send_files(
    state: &DaemonState,
    request: SendRequest,
) -> Result<String, Status> {
    let paths = validated_paths(request.paths).await?;
    let target =
        request.target.ok_or_else(|| Status::invalid_argument("send target is required"))?;
    let transfer_id = Uuid::new_v4().to_string();

    state.events.emit(DaemonEvent::TransferStarted { transfer_id: transfer_id.clone() });
    let result = match target {
        send_request::Target::LocalsendUrl(url) => LocalSendClient::new()
            .map_err(internal_status)?
            .send_files_to_url(&url, paths, sender_info(state))
            .await
            .map_err(internal_status),
        send_request::Target::NativeUrl(_url) => Err(Status::failed_precondition(
            "native URL sends require pinned certificate metadata; use CLI --direct --native --native-cert-fingerprint or trusted WAN peer metadata",
        )),
        send_request::Target::PeerFingerprint(peer) => {
            if request.native {
                send_files_to_native_lan_peer(state, peer, paths).await
            } else {
                send_files_to_localsend_lan_peer(state, peer, paths).await
            }
        }
        send_request::Target::WanPeer(fingerprint) => {
            send_files_to_wan_peer(state, fingerprint, paths).await
        }
    };

    match result {
        Ok(()) => {
            state.events.emit(DaemonEvent::TransferCompleted { transfer_id: transfer_id.clone() });
            Ok(transfer_id)
        }
        Err(status) => {
            state.events.emit(DaemonEvent::TransferFailed {
                transfer_id,
                code: format!("{:?}", status.code()),
                message: status.message().to_string(),
            });
            Err(status)
        }
    }
}

async fn send_files_to_native_lan_peer(
    state: &DaemonState,
    peer_ref: String,
    paths: Vec<std::path::PathBuf>,
) -> Result<(), Status> {
    let peers =
        crate::native_lan::discover(std::time::Duration::from_millis(1500), &state.fingerprint)
            .await
            .map_err(internal_status)?;
    let peer = crate::native_lan::resolve_peer_ref(peers, &peer_ref).map_err(not_found_status)?;
    let fingerprint = peer.fingerprint.parse::<Fingerprint>().map_err(|error| {
        Status::invalid_argument(format!("invalid native peer fingerprint: {error}"))
    })?;
    let trust_store = TrustStore::open(&state.trust_db_path)
        .map_err(|error| Status::internal(format!("failed to open trust store: {error}")))?;
    let trusted = trust_store
        .get(&fingerprint)
        .map_err(|error| Status::internal(format!("failed to read trusted peer: {error}")))?
        .ok_or_else(|| {
            Status::permission_denied(format!("native peer is not trusted: {peer_ref}"))
        })?;
    let expected_certificate_fingerprint = native_wan_certificate_fingerprint(&trusted)?;
    let advertised_certificate_fingerprint =
        peer.native_certificate_fingerprint.as_deref().ok_or_else(|| {
            Status::failed_precondition(format!(
                "native peer {peer_ref:?} did not advertise certificate metadata"
            ))
        })?;
    if advertised_certificate_fingerprint != expected_certificate_fingerprint {
        return Err(Status::permission_denied(format!(
            "native peer certificate fingerprint mismatch for {peer_ref:?}"
        )));
    }

    let url = format!("quic://{}:{}", peer.address, peer.port);
    NativeTransferClient::send_files_to_url(
        &url,
        paths,
        state.identity.clone(),
        expected_certificate_fingerprint,
    )
    .await
    .map_err(internal_status)
}

async fn send_files_to_localsend_lan_peer(
    state: &DaemonState,
    peer_ref: String,
    paths: Vec<std::path::PathBuf>,
) -> Result<(), Status> {
    let peer = resolve_localsend_lan_peer(state, &peer_ref).await?;
    let url = format!("{}://{}:{}", peer.protocol, peer.source_ip, peer.port);
    LocalSendClient::new()
        .map_err(internal_status)?
        .send_files_to_url(&url, paths, sender_info(state))
        .await
        .map_err(internal_status)
}

async fn resolve_localsend_lan_peer(
    state: &DaemonState,
    peer_ref: &str,
) -> Result<LocalSendLanPeer, Status> {
    let needle = normalize_localsend_peer_ref(peer_ref);
    if needle.is_empty() {
        return Err(Status::invalid_argument("LocalSend peer is required"));
    }

    let peers = crate::localsend_lan::scan_and_cache(&state.trust_db_path)
        .await
        .map_err(internal_status)?;
    let mut matches = peers
        .into_iter()
        .filter(|peer| {
            normalize_localsend_peer_ref(&peer.fingerprint) == needle
                || peer.alias.eq_ignore_ascii_case(peer_ref)
        })
        .collect::<Vec<_>>();

    match matches.len() {
        0 => Err(Status::not_found(format!(
            "LocalSend LAN peer {peer_ref:?} was not discovered; keep the app open and run `night-bridge peers list-lan`"
        ))),
        1 => Ok(matches.remove(0)),
        _ => Err(Status::failed_precondition(format!(
            "LocalSend LAN peer {peer_ref:?} is ambiguous; use its fingerprint"
        ))),
    }
}

fn normalize_localsend_peer_ref(value: &str) -> String {
    value.chars().filter(|ch| *ch != ':' && *ch != '-').flat_map(char::to_lowercase).collect()
}

async fn send_files_to_wan_peer(
    state: &DaemonState,
    fingerprint: String,
    paths: Vec<std::path::PathBuf>,
) -> Result<(), Status> {
    if !state.config.wan.enabled || state.config.wan.rendezvous_url.is_none() {
        return Err(Status::failed_precondition("WAN rendezvous is not configured"));
    }

    let fingerprint = fingerprint.parse::<Fingerprint>().map_err(|error| {
        Status::invalid_argument(format!("invalid WAN peer fingerprint: {error}"))
    })?;
    let trust_store = TrustStore::open(&state.trust_db_path)
        .map_err(|error| Status::internal(format!("failed to open trust store: {error}")))?;
    let peer = trust_store
        .get(&fingerprint)
        .map_err(|error| Status::internal(format!("failed to read trusted peer: {error}")))?
        .ok_or_else(|| Status::not_found(format!("trusted peer not found: {fingerprint}")))?;

    let expected_certificate_fingerprint = native_wan_certificate_fingerprint(&peer)?;
    let candidates = wan::lookup_peer_candidates(&state.config.wan, &state.identity, peer.pubkey)
        .await
        .map_err(internal_status)?;
    NativeTransferClient::send_files_to_candidates(
        candidates,
        paths,
        state.identity.clone(),
        expected_certificate_fingerprint,
    )
    .await
    .map_err(internal_status)
}

fn native_wan_certificate_fingerprint(peer: &Peer) -> Result<String, Status> {
    match peer.policy {
        PeerPolicy::Block => {
            return Err(Status::permission_denied(format!(
                "trusted peer is blocked: {}",
                peer.fingerprint
            )));
        }
        PeerPolicy::Prompt => {
            return Err(Status::permission_denied(format!(
                "trusted peer requires prompt before native WAN send: {}",
                peer.fingerprint
            )));
        }
        PeerPolicy::AutoAccept => {}
    }

    peer.native_certificate_fingerprint.clone().ok_or_else(|| {
        Status::failed_precondition(format!(
            "trusted peer is missing native certificate metadata: {}; re-pair or refresh trust",
            peer.fingerprint
        ))
    })
}

async fn validated_paths(paths: Vec<String>) -> Result<Vec<std::path::PathBuf>, Status> {
    if paths.is_empty() {
        return Err(Status::invalid_argument("at least one file path is required"));
    }

    let mut validated = Vec::with_capacity(paths.len());
    for path in paths {
        let path = std::path::PathBuf::from(path);
        let metadata = tokio::fs::metadata(&path).await.map_err(|error| {
            Status::invalid_argument(format!("file does not exist: {} ({error})", path.display()))
        })?;
        if !metadata.is_file() {
            return Err(Status::invalid_argument(format!("not a file: {}", path.display())));
        }
        validated.push(path);
    }
    Ok(validated)
}

fn sender_info(state: &DaemonState) -> DeviceInfo {
    DeviceInfo {
        alias: state.alias.clone(),
        version: "2.0".to_string(),
        device_model: None,
        device_type: Some("desktop".to_string()),
        fingerprint: state.fingerprint.clone(),
        port: 0,
        protocol: Protocol::from("https"),
        download: false,
    }
}

fn internal_status(error: impl std::fmt::Display) -> Status {
    Status::internal(error.to_string())
}

fn not_found_status(error: impl std::fmt::Display) -> Status {
    Status::not_found(error.to_string())
}

#[cfg(test)]
mod tests {
    use lsi_core::identity::Fingerprint;

    use super::*;

    fn peer(policy: PeerPolicy, native_certificate_fingerprint: Option<String>) -> Peer {
        let pubkey = [7; 32];
        Peer {
            fingerprint: Fingerprint::from_pubkey(&pubkey),
            pubkey,
            label: "peer".to_string(),
            trusted_at: 1,
            last_seen: None,
            native_certificate_fingerprint,
            policy,
        }
    }

    #[test]
    fn native_wan_auto_accept_requires_certificate_metadata() {
        let status =
            native_wan_certificate_fingerprint(&peer(PeerPolicy::AutoAccept, None)).unwrap_err();

        assert_eq!(status.code(), tonic::Code::FailedPrecondition);
        assert!(status.message().contains("missing native certificate metadata"));
    }

    #[test]
    fn native_wan_rejects_blocked_peer() {
        let status =
            native_wan_certificate_fingerprint(&peer(PeerPolicy::Block, Some("a".repeat(64))))
                .unwrap_err();

        assert_eq!(status.code(), tonic::Code::PermissionDenied);
        assert!(status.message().contains("trusted peer is blocked"));
    }

    #[test]
    fn native_wan_rejects_prompt_peer() {
        let status =
            native_wan_certificate_fingerprint(&peer(PeerPolicy::Prompt, Some("a".repeat(64))))
                .unwrap_err();

        assert_eq!(status.code(), tonic::Code::PermissionDenied);
        assert!(status.message().contains("requires prompt"));
    }

    #[test]
    fn native_wan_auto_accept_returns_certificate_metadata() {
        let cert = "a".repeat(64);
        let result =
            native_wan_certificate_fingerprint(&peer(PeerPolicy::AutoAccept, Some(cert.clone())))
                .unwrap();

        assert_eq!(result, cert);
    }
}
