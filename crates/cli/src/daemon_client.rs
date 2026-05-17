//! Authenticated gRPC client helpers for the local daemon API.

use anyhow::{bail, Context, Result};
use lsi_core::{
    api_token::{ApiTokenVault, FsApiTokenVault},
    paths,
};
use lsi_proto::{
    common::v1::Empty,
    daemon::v1::{daemon_service_client::DaemonServiceClient, DaemonStatus},
    peers::v1::{
        peers_service_client::PeersServiceClient, ApproveLocalSendPeerRequest,
        DenyLocalSendPeerRequest, ListPendingLocalSendPeersRequest, ListTrustedPeersRequest,
        LocalSendPeer, TrustedPeer,
    },
    transfers::v1::{
        transfers_service_client::TransfersServiceClient, ActiveTransfer,
        ListActiveTransfersRequest, ResumeRequest, ResumeResponse, SendRequest, SendResponse,
    },
};
use tonic::{
    metadata::MetadataValue,
    service::Interceptor,
    transport::{Channel, Endpoint},
    Request, Status,
};

type AuthenticatedService =
    tonic::service::interceptor::InterceptedService<Channel, BearerTokenInterceptor>;

/// CLI configuration needed to reach the daemon gRPC API.
#[derive(Clone, Debug)]
pub struct DaemonClientConfig {
    /// Daemon gRPC endpoint, for example `http://127.0.0.1:53500`.
    pub endpoint: String,
    /// Explicit bearer token. When missing, the token is loaded from disk.
    pub api_token: Option<String>,
}

impl DaemonClientConfig {
    /// Load the configured bearer token or read it from the standard token file.
    pub fn load_api_token(&self) -> Result<String> {
        if let Some(token) = &self.api_token {
            return Ok(token.clone());
        }

        let path = paths::api_token_file();
        let vault = FsApiTokenVault::new(&path);
        let Some(token) =
            vault.load().with_context(|| format!("loading api token from {}", path.display()))?
        else {
            bail!(
                "api token not found at {}; start the daemon once to create it or pass --api-token",
                path.display()
            );
        };

        Ok(token.expose_secret().to_string())
    }

    async fn channel(&self) -> Result<Channel> {
        let endpoint = Endpoint::from_shared(self.endpoint.clone())
            .with_context(|| format!("invalid daemon gRPC endpoint {}", self.endpoint))?;
        endpoint.connect().await.with_context(|| {
            format!(
                "daemon API unavailable at {}; start night-bridge-daemon or use offline command",
                self.endpoint
            )
        })
    }

    fn interceptor(&self) -> Result<BearerTokenInterceptor> {
        BearerTokenInterceptor::new(self.load_api_token()?)
    }

    /// Connect to the daemon and attach bearer metadata to every request.
    pub async fn connect(&self) -> Result<DaemonServiceClient<AuthenticatedService>> {
        let interceptor = self.interceptor()?;
        let channel = self.channel().await?;
        Ok(DaemonServiceClient::with_interceptor(channel, interceptor))
    }

    async fn peers_client(&self) -> Result<PeersServiceClient<AuthenticatedService>> {
        let interceptor = self.interceptor()?;
        let channel = self.channel().await?;
        Ok(PeersServiceClient::with_interceptor(channel, interceptor))
    }

    async fn transfers_client(&self) -> Result<TransfersServiceClient<AuthenticatedService>> {
        let interceptor = self.interceptor()?;
        let channel = self.channel().await?;
        Ok(TransfersServiceClient::with_interceptor(channel, interceptor))
    }
}

/// Fetch daemon status through the authenticated gRPC client.
pub async fn get_status(config: &DaemonClientConfig) -> Result<DaemonStatus> {
    let mut client = config.connect().await?;
    let response =
        client.get_status(Request::new(Empty {})).await.context("daemon status request failed")?;
    Ok(response.into_inner())
}

/// List trusted peers through the authenticated daemon API.
pub async fn list_trusted_peers(config: &DaemonClientConfig) -> Result<Vec<TrustedPeer>> {
    let mut client = config.peers_client().await?;
    let response = client
        .list_trusted(Request::new(ListTrustedPeersRequest {}))
        .await
        .context("trusted peers request failed")?;
    Ok(response.into_inner().peers)
}

/// List pending official LocalSend peers through the authenticated daemon API.
pub async fn list_pending_localsend_peers(
    config: &DaemonClientConfig,
) -> Result<Vec<LocalSendPeer>> {
    let mut client = config.peers_client().await?;
    let response = client
        .list_pending_local_send(Request::new(ListPendingLocalSendPeersRequest {}))
        .await
        .context("pending LocalSend peers request failed")?;
    Ok(response.into_inner().peers)
}

/// Approve an official LocalSend peer fingerprint through the daemon API.
pub async fn approve_localsend_peer(
    config: &DaemonClientConfig,
    fingerprint: String,
    label: Option<String>,
) -> Result<LocalSendPeer> {
    let mut client = config.peers_client().await?;
    let response = client
        .approve_local_send(Request::new(ApproveLocalSendPeerRequest { fingerprint, label }))
        .await
        .context("approve LocalSend peer request failed")?;
    response.into_inner().peer.context("daemon returned no LocalSend peer")
}

/// Deny an official LocalSend peer fingerprint through the daemon API.
pub async fn deny_localsend_peer(
    config: &DaemonClientConfig,
    fingerprint: String,
) -> Result<LocalSendPeer> {
    let mut client = config.peers_client().await?;
    let response = client
        .deny_local_send(Request::new(DenyLocalSendPeerRequest { fingerprint }))
        .await
        .context("deny LocalSend peer request failed")?;
    response.into_inner().peer.context("daemon returned no LocalSend peer")
}

/// List active transfers through the authenticated daemon API.
pub async fn list_active_transfers(config: &DaemonClientConfig) -> Result<Vec<ActiveTransfer>> {
    let mut client = config.transfers_client().await?;
    let response = client
        .list_active(Request::new(ListActiveTransfersRequest {}))
        .await
        .context("active transfers request failed")?;
    Ok(response.into_inner().transfers)
}

/// Ask the daemon to resume an interrupted transfer.
pub async fn resume_transfer(
    config: &DaemonClientConfig,
    transfer_id: String,
) -> Result<ResumeResponse> {
    let mut client = config.transfers_client().await?;
    let response = client
        .resume(Request::new(ResumeRequest { transfer_id }))
        .await
        .context("resume transfer request failed")?;
    Ok(response.into_inner())
}

/// Ask the daemon to send files.
pub async fn send_files(config: &DaemonClientConfig, request: SendRequest) -> Result<SendResponse> {
    let mut client = config.transfers_client().await?;
    let response = client.send(Request::new(request)).await.context("send request failed")?;
    Ok(response.into_inner())
}

/// Interceptor that adds `authorization: Bearer <token>` to each request.
#[derive(Clone)]
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
