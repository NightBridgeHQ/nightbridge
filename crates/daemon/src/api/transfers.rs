use std::sync::Arc;

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
        send_request::Target::NativeUrl(url) => {
            NativeTransferClient::send_files_to_url(&url, paths, state.identity.clone())
                .await
                .map_err(internal_status)
        }
        send_request::Target::PeerFingerprint(_) => {
            Err(Status::unimplemented("trusted-peer send is not wired yet"))
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
