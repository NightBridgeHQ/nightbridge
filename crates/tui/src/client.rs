//! Thin gRPC polling client for the daemon dashboard.

use std::time::Duration;

use anyhow::{Context, Result};
use lsi_proto::{
    common::v1::Empty,
    daemon::v1::daemon_service_client::DaemonServiceClient,
    inbox::v1::{inbox_service_client::InboxServiceClient, ListInboxRequest},
    peers::v1::{peers_service_client::PeersServiceClient, ListTrustedPeersRequest, PeerPolicy},
    transfers::v1::{
        transfers_service_client::TransfersServiceClient, ListActiveTransfersRequest,
        TransferDirection, TransferState,
    },
};
use tonic::{
    metadata::MetadataValue,
    service::Interceptor,
    transport::{Channel, Endpoint},
    Request, Status,
};

use crate::app::{AppState, InboxEntry, Transfer, TrustedPeer};

/// gRPC configuration for polling daemon state.
#[derive(Clone, Debug)]
pub struct DaemonApiConfig {
    /// Daemon gRPC endpoint, for example `http://127.0.0.1:53500`.
    pub endpoint: String,
    /// Local daemon API bearer token.
    pub api_token: String,
    /// Poll interval for live dashboards.
    pub poll_interval: Duration,
}

impl DaemonApiConfig {
    /// Create a config with the default one-second poll interval.
    pub fn new(endpoint: impl Into<String>, api_token: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            api_token: api_token.into(),
            poll_interval: Duration::from_secs(1),
        }
    }

    /// Poll interval for interactive dashboards.
    pub fn poll_interval(&self) -> Duration {
        self.poll_interval
    }
}

/// Polling gRPC client used by the TUI.
#[derive(Clone, Debug)]
pub struct DaemonApiClient {
    config: DaemonApiConfig,
}

impl DaemonApiClient {
    /// Create a new polling client.
    pub fn new(config: DaemonApiConfig) -> Self {
        Self { config }
    }

    /// Fetch a full dashboard snapshot once.
    pub async fn fetch_once(&self) -> Result<AppState> {
        let channel = self.channel().await?;
        let interceptor = self.interceptor()?;
        let mut daemon =
            DaemonServiceClient::with_interceptor(channel.clone(), interceptor.clone());
        let mut peers = PeersServiceClient::with_interceptor(channel.clone(), interceptor.clone());
        let mut transfers =
            TransfersServiceClient::with_interceptor(channel.clone(), interceptor.clone());
        let mut inbox = InboxServiceClient::with_interceptor(channel, interceptor);

        let status = daemon
            .get_status(Request::new(Empty {}))
            .await
            .context("daemon status request failed")?
            .into_inner();
        let trusted_peers = peers
            .list_trusted(Request::new(ListTrustedPeersRequest {}))
            .await
            .context("trusted peers request failed")?
            .into_inner()
            .peers
            .into_iter()
            .map(|peer| TrustedPeer {
                fingerprint: peer
                    .fingerprint
                    .map(|fingerprint| fingerprint.value)
                    .unwrap_or_default(),
                label: peer.label,
                policy: peer_policy(peer.policy).to_string(),
                last_seen_unix_seconds: peer.last_seen_unix_seconds,
            })
            .collect();
        let active_transfers = transfers
            .list_active(Request::new(ListActiveTransfersRequest {}))
            .await
            .context("active transfers request failed")?
            .into_inner()
            .transfers
            .into_iter()
            .map(|transfer| Transfer {
                transfer_id: transfer.transfer_id,
                peer_fingerprint: transfer.peer_fingerprint,
                direction: transfer_direction(transfer.direction).to_string(),
                state: transfer_state(transfer.state).to_string(),
                bytes_done: transfer.bytes_done,
                bytes_total: transfer.bytes_total,
            })
            .collect();
        let inbox_entries = inbox
            .list_inbox(Request::new(ListInboxRequest {}))
            .await
            .context("inbox request failed")?
            .into_inner()
            .entries
            .into_iter()
            .map(|entry| InboxEntry {
                file_name: entry.file_name,
                path: entry.path,
                size: entry.size,
                modified_unix_seconds: entry.modified_unix_seconds,
            })
            .collect();

        let mut state = AppState::default();
        state.set_status(status);
        state.trusted_peers = trusted_peers;
        state.active_transfers = active_transfers;
        state.inbox_entries = inbox_entries;
        Ok(state)
    }

    async fn channel(&self) -> Result<Channel> {
        let endpoint = Endpoint::from_shared(self.config.endpoint.clone())
            .with_context(|| format!("invalid daemon gRPC endpoint {}", self.config.endpoint))?;
        endpoint
            .connect()
            .await
            .with_context(|| format!("daemon API unavailable at {}", self.config.endpoint))
    }

    fn interceptor(&self) -> Result<BearerTokenInterceptor> {
        BearerTokenInterceptor::new(self.config.api_token.clone())
    }
}

fn peer_policy(policy: i32) -> &'static str {
    match PeerPolicy::try_from(policy).unwrap_or(PeerPolicy::Unspecified) {
        PeerPolicy::AutoAccept => "auto_accept",
        PeerPolicy::Prompt => "prompt",
        PeerPolicy::Block => "block",
        PeerPolicy::Unspecified => "unspecified",
    }
}

fn transfer_direction(direction: i32) -> &'static str {
    match TransferDirection::try_from(direction).unwrap_or(TransferDirection::Unspecified) {
        TransferDirection::Send => "send",
        TransferDirection::Receive => "receive",
        TransferDirection::Unspecified => "unspecified",
    }
}

fn transfer_state(state: i32) -> &'static str {
    match TransferState::try_from(state).unwrap_or(TransferState::Unspecified) {
        TransferState::Pending => "pending",
        TransferState::Active => "active",
        TransferState::Interrupted => "interrupted",
        TransferState::Completed => "completed",
        TransferState::Cancelled => "cancelled",
        TransferState::Failed => "failed",
        TransferState::Unspecified => "unspecified",
    }
}

/// Interceptor that adds `authorization: Bearer <token>` to each request.
#[derive(Clone, Debug)]
pub struct BearerTokenInterceptor {
    metadata: MetadataValue<tonic::metadata::Ascii>,
}

impl BearerTokenInterceptor {
    fn new(token: String) -> Result<Self> {
        let value = format!("Bearer {token}");
        let metadata =
            MetadataValue::try_from(value).context("api token cannot be used as metadata")?;
        Ok(Self { metadata })
    }
}

impl Interceptor for BearerTokenInterceptor {
    fn call(&mut self, mut request: Request<()>) -> std::result::Result<Request<()>, Status> {
        request.metadata_mut().insert("authorization", self.metadata.clone());
        Ok(request)
    }
}
