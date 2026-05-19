use std::convert::Infallible;
use std::fmt::Write as _;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode, Uri},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Json, Router,
};
use lsi_core::trust::{PeerPolicy as CorePeerPolicy, TrustStore};
use lsi_proto::events::v1::{daemon_event, DaemonEventType};
use lsi_proto::transfers::v1::{send_request, SendRequest};
use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;

use crate::api::transfers;
use crate::state::DaemonState;

pub(crate) struct HttpApiRuntime {
    local_addr: SocketAddr,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    server_task: JoinHandle<Result<()>>,
}

impl HttpApiRuntime {
    pub(crate) fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }
}

#[derive(Clone)]
struct HttpState {
    daemon: Arc<DaemonState>,
    token: String,
}

pub(crate) async fn start_http_runtime(
    state: Arc<DaemonState>,
    port: u16,
) -> Result<HttpApiRuntime> {
    let bind_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .with_context(|| format!("failed to bind HTTP API on {bind_addr}"))?;
    let local_addr = listener.local_addr().context("failed to read HTTP API listener address")?;
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let app =
        router(HttpState { token: state.api_token.expose_secret().to_string(), daemon: state });

    let server_task = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(wait_for_shutdown_signal(shutdown_rx))
            .await
            .map_err(Into::into)
    });

    Ok(HttpApiRuntime { local_addr, shutdown_tx, server_task })
}

pub(crate) async fn stop_http_runtime(runtime: HttpApiRuntime) -> Result<()> {
    let _ = runtime.shutdown_tx.send(true);
    runtime.server_task.await.context("HTTP API server task panicked")??;
    Ok(())
}

fn router(state: HttpState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics))
        .route("/api/v1/status", get(status))
        .route("/api/v1/peers/trusted", get(list_trusted))
        .route("/api/v1/peers/lan", get(list_lan))
        .route("/api/v1/localsend/pending-peers", get(list_pending_localsend))
        .route("/api/v1/localsend/pending-peers/:fingerprint/approve", post(approve_localsend))
        .route("/api/v1/localsend/pending-peers/:fingerprint/deny", post(deny_localsend))
        .route("/api/v1/inbox", get(list_inbox))
        .route("/api/v1/transfers/active", get(list_active_transfers))
        .route("/api/v1/transfers/send", post(send_transfer))
        .route("/api/v1/transfers/:id/resume", post(resume_transfer))
        .route("/api/v1/transfers/:id/cancel", post(cancel_transfer))
        .route("/api/v1/events", get(events))
        .route("/", get(webui_index))
        .route("/assets/*path", get(webui_asset))
        .fallback(webui_fallback)
        .with_state(state)
}

async fn healthz() -> StatusCode {
    StatusCode::OK
}

async fn readyz(State(_state): State<HttpState>) -> StatusCode {
    StatusCode::OK
}

async fn metrics(State(state): State<HttpState>) -> Result<Response, StatusCode> {
    let Some(metrics) = &state.daemon.metrics else {
        return Err(StatusCode::NOT_FOUND);
    };

    Ok(([(header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")], metrics.render())
        .into_response())
}

async fn webui_index() -> Result<Response, StatusCode> {
    webui_response("index.html")
}

async fn webui_asset(Path(path): Path<String>) -> Result<Response, StatusCode> {
    webui_response(&format!("assets/{path}"))
}

async fn webui_fallback(uri: Uri) -> Result<Response, StatusCode> {
    if uri.path().starts_with("/api/") {
        return Err(StatusCode::NOT_FOUND);
    }

    webui_response("index.html")
}

fn webui_response(path: &str) -> Result<Response, StatusCode> {
    let asset = lsi_webui::asset(path).ok_or(StatusCode::NOT_FOUND)?;
    Ok(([(header::CONTENT_TYPE, asset.content_type)], Body::from(asset.data.into_owned()))
        .into_response())
}

fn local_send_peer_dto(peer: lsi_core::trust::LocalSendPeer) -> LocalSendPeerDto {
    LocalSendPeerDto {
        fingerprint: peer.fingerprint,
        alias: peer.alias,
        label: peer.label,
        status: match peer.status {
            lsi_core::trust::LocalSendPeerStatus::Pending => "pending",
            lsi_core::trust::LocalSendPeerStatus::Trusted => "trusted",
            lsi_core::trust::LocalSendPeerStatus::Blocked => "blocked",
        },
        first_seen_unix_seconds: peer.first_seen,
        last_seen_unix_seconds: peer.last_seen,
        attempt_count: peer.attempt_count,
        source_ip: peer.source_ip,
    }
}

async fn status(
    State(state): State<HttpState>,
    headers: HeaderMap,
) -> Result<Json<StatusDto>, ApiError> {
    authorize(&headers, &state.token)?;
    Ok(Json(StatusDto {
        alias: state.daemon.alias.clone(),
        fingerprint: state.daemon.fingerprint.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        inbox_dir: state.daemon.inbox_dir.display().to_string(),
        localsend_port: state.daemon.localsend_port,
        native_port: state.daemon.native_port,
    }))
}

async fn list_trusted(
    State(state): State<HttpState>,
    headers: HeaderMap,
) -> Result<Json<TrustedPeersDto>, ApiError> {
    authorize(&headers, &state.token)?;
    let store = TrustStore::open(&state.daemon.trust_db_path).map_err(ApiError::internal)?;
    let peers = store
        .list()
        .map_err(ApiError::internal)?
        .into_iter()
        .map(|peer| TrustedPeerDto {
            fingerprint: peer.fingerprint.to_string(),
            pubkey_hex: hex_encode(&peer.pubkey),
            label: peer.label,
            trusted_at_unix_seconds: peer.trusted_at,
            last_seen_unix_seconds: peer.last_seen,
            native_certificate_fingerprint: peer.native_certificate_fingerprint,
            policy: match peer.policy {
                CorePeerPolicy::AutoAccept => "auto_accept",
                CorePeerPolicy::Prompt => "prompt",
                CorePeerPolicy::Block => "block",
            },
        })
        .collect();
    Ok(Json(TrustedPeersDto { peers }))
}

async fn list_lan(
    State(state): State<HttpState>,
    headers: HeaderMap,
) -> Result<Json<LanPeersDto>, ApiError> {
    authorize(&headers, &state.token)?;
    let peers = crate::localsend_lan::scan_and_cache(&state.daemon.trust_db_path)
        .await
        .map_err(ApiError::internal)?
        .into_iter()
        .map(localsend_lan_peer)
        .collect();
    Ok(Json(LanPeersDto { peers }))
}

fn localsend_lan_peer(peer: lsi_core::trust::LocalSendLanPeer) -> LanPeerDto {
    LanPeerDto {
        alias: peer.alias,
        address: peer.source_ip,
        port: peer.port,
        protocol: "localsend_v2",
        fingerprint: peer.fingerprint,
        device_model: peer.device_model,
        device_type: peer.device_type,
        download: peer.download,
    }
}

async fn list_pending_localsend(
    State(state): State<HttpState>,
    headers: HeaderMap,
) -> Result<Json<LocalSendPeersDto>, ApiError> {
    authorize(&headers, &state.token)?;
    let store = TrustStore::open(&state.daemon.trust_db_path).map_err(ApiError::internal)?;
    let peers = store
        .list_pending_localsend_peers()
        .map_err(ApiError::internal)?
        .into_iter()
        .map(local_send_peer_dto)
        .collect();
    Ok(Json(LocalSendPeersDto { peers }))
}

async fn approve_localsend(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Path(fingerprint): Path<String>,
    Json(request): Json<ApproveLocalSendPeerDto>,
) -> Result<Json<LocalSendPeerDto>, ApiError> {
    authorize(&headers, &state.token)?;
    let store = TrustStore::open(&state.daemon.trust_db_path).map_err(ApiError::internal)?;
    let peer = store
        .approve_localsend_peer(&fingerprint, request.label.as_deref())
        .map_err(ApiError::internal)?;
    Ok(Json(local_send_peer_dto(peer)))
}

async fn deny_localsend(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Path(fingerprint): Path<String>,
) -> Result<Json<LocalSendPeerDto>, ApiError> {
    authorize(&headers, &state.token)?;
    let store = TrustStore::open(&state.daemon.trust_db_path).map_err(ApiError::internal)?;
    let peer = store.deny_localsend_peer(&fingerprint).map_err(ApiError::internal)?;
    Ok(Json(local_send_peer_dto(peer)))
}

async fn list_inbox(
    State(state): State<HttpState>,
    headers: HeaderMap,
) -> Result<Json<InboxDto>, ApiError> {
    authorize(&headers, &state.token)?;
    let entries = inbox_entries(&state.daemon.inbox_dir).await?;
    Ok(Json(InboxDto { entries }))
}

async fn list_active_transfers(
    State(state): State<HttpState>,
    headers: HeaderMap,
) -> Result<Json<ActiveTransfersDto>, ApiError> {
    authorize(&headers, &state.token)?;
    Ok(Json(ActiveTransfersDto { transfers: Vec::new() }))
}

async fn send_transfer(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Json(request): Json<SendTransferDto>,
) -> Result<Json<SendResponseDto>, ApiError> {
    authorize(&headers, &state.token)?;
    let transfer_id = transfers::send_files(&state.daemon, request.into_proto()?)
        .await
        .map_err(ApiError::from_status)?;
    Ok(Json(SendResponseDto { transfer_id }))
}

async fn resume_transfer(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Path(transfer_id): Path<String>,
) -> Result<Json<TransferStateDto>, ApiError> {
    authorize(&headers, &state.token)?;
    Err(ApiError::new(
        StatusCode::NOT_IMPLEMENTED,
        format!("resume is not wired to daemon API yet for transfer {transfer_id}"),
    ))
}

async fn cancel_transfer(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Path(transfer_id): Path<String>,
) -> Result<Json<CancelResponseDto>, ApiError> {
    authorize(&headers, &state.token)?;
    Ok(Json(CancelResponseDto { transfer_id, state: "unspecified" }))
}

async fn events(
    State(state): State<HttpState>,
    headers: HeaderMap,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    authorize(&headers, &state.token)?;
    let mut rx = state.daemon.events.subscribe();
    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let data = event_json(event.to_proto()).to_string();
                    yield Ok(Event::default().event("daemon").data(data));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

fn event_json(event: lsi_proto::events::v1::DaemonEvent) -> serde_json::Value {
    let event_type =
        DaemonEventType::try_from(event.r#type).unwrap_or(DaemonEventType::Unspecified);
    let mut value = serde_json::json!({
        "event_id": event.event_id,
        "occurred_at_unix_seconds": event.occurred_at_unix_seconds,
        "type": event_type_name(event_type),
    });

    if let Some(payload) = event.payload {
        let (key, payload) = payload_json(payload);
        value[key] = payload;
    }

    value
}

fn event_type_name(event_type: DaemonEventType) -> &'static str {
    match event_type {
        DaemonEventType::TransferStarted => "transfer_started",
        DaemonEventType::TransferProgress => "transfer_progress",
        DaemonEventType::TransferCompleted => "transfer_completed",
        DaemonEventType::TransferFailed => "transfer_failed",
        DaemonEventType::InboxChanged => "inbox_changed",
        DaemonEventType::PeerSeen => "peer_seen",
        DaemonEventType::Unspecified => "unspecified",
    }
}

fn payload_json(payload: daemon_event::Payload) -> (&'static str, serde_json::Value) {
    match payload {
        daemon_event::Payload::TransferStarted(started) => {
            let transfer = started.transfer;
            (
                "transfer_started",
                serde_json::json!({
                    "transfer": transfer.map(|transfer| serde_json::json!({
                        "transfer_id": transfer.transfer_id,
                        "peer_fingerprint": transfer.peer_fingerprint,
                        "direction": transfer.direction,
                        "state": transfer.state,
                        "bytes_total": transfer.bytes_total,
                        "bytes_done": transfer.bytes_done,
                        "created_at_unix_seconds": transfer.created_at_unix_seconds,
                        "updated_at_unix_seconds": transfer.updated_at_unix_seconds,
                    })),
                }),
            )
        }
        daemon_event::Payload::TransferProgress(progress) => (
            "transfer_progress",
            serde_json::json!({
                "transfer_id": progress.transfer_id,
                "bytes_done": progress.bytes_done,
                "bytes_total": progress.bytes_total,
                "files": transfer_files_json(progress.files),
            }),
        ),
        daemon_event::Payload::TransferCompleted(completed) => (
            "transfer_completed",
            serde_json::json!({
                "transfer_id": completed.transfer_id,
                "inbox_entries": inbox_entries_json(completed.inbox_entries),
            }),
        ),
        daemon_event::Payload::TransferFailed(failed) => (
            "transfer_failed",
            serde_json::json!({
                "transfer_id": failed.transfer_id,
                "error": failed.error.map(|error| serde_json::json!({
                    "code": error.code,
                    "message": error.message,
                })),
            }),
        ),
        daemon_event::Payload::InboxChanged(changed) => {
            ("inbox_changed", serde_json::json!({ "entries": inbox_entries_json(changed.entries) }))
        }
        daemon_event::Payload::PeerSeen(seen) => (
            "peer_seen",
            serde_json::json!({
                "peer": seen.peer.map(|peer| serde_json::json!({
                    "alias": peer.alias,
                    "address": peer.address,
                    "port": peer.port,
                    "protocol": peer.protocol,
                    "fingerprint": peer.fingerprint.map(|fingerprint| fingerprint.value),
                    "device_model": peer.device_model,
                    "device_type": peer.device_type,
                    "download": peer.download,
                    "extensions": peer.extensions,
                })),
            }),
        ),
    }
}

fn transfer_files_json(
    files: Vec<lsi_proto::transfers::v1::TransferFile>,
) -> Vec<serde_json::Value> {
    files
        .into_iter()
        .map(|file| {
            serde_json::json!({
                "file_id": file.file_id,
                "file_name": file.file_name,
                "path": file.path,
                "size": file.size,
                "bytes_done": file.bytes_done,
                "blake3": file.blake3,
            })
        })
        .collect()
}

fn inbox_entries_json(entries: Vec<lsi_proto::inbox::v1::InboxEntry>) -> Vec<serde_json::Value> {
    entries
        .into_iter()
        .map(|entry| {
            serde_json::json!({
                "file_name": entry.file_name,
                "path": entry.path,
                "size": entry.size,
                "modified_unix_seconds": entry.modified_unix_seconds,
            })
        })
        .collect()
}

async fn inbox_entries(inbox_dir: &std::path::Path) -> Result<Vec<InboxEntryDto>, ApiError> {
    if tokio::fs::metadata(inbox_dir).await.is_err() {
        return Ok(Vec::new());
    }

    let mut dir = tokio::fs::read_dir(inbox_dir).await.map_err(ApiError::internal)?;
    let mut entries = Vec::new();
    while let Some(entry) = dir.next_entry().await.map_err(ApiError::internal)? {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if file_name == ".incoming" || file_name.ends_with(".incoming") {
            continue;
        }
        let metadata = entry.metadata().await.map_err(ApiError::internal)?;
        if !metadata.is_file() {
            continue;
        }
        let modified_unix_seconds = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or(0);
        entries.push(InboxEntryDto {
            file_name,
            path: entry.path().canonicalize().map_err(ApiError::internal)?.display().to_string(),
            size: metadata.len(),
            modified_unix_seconds,
        });
    }
    entries.sort_by(|left, right| left.file_name.cmp(&right.file_name));
    Ok(entries)
}

fn authorize(headers: &HeaderMap, expected_token: &str) -> Result<(), ApiError> {
    let Some(value) = headers.get(axum::http::header::AUTHORIZATION) else {
        return Err(ApiError::new(StatusCode::UNAUTHORIZED, "missing bearer token"));
    };
    let value = value
        .to_str()
        .map_err(|_| ApiError::new(StatusCode::UNAUTHORIZED, "malformed bearer token"))?;
    let Some(token) = value.strip_prefix("Bearer ") else {
        return Err(ApiError::new(StatusCode::UNAUTHORIZED, "malformed bearer token"));
    };
    if token != expected_token {
        return Err(ApiError::new(StatusCode::UNAUTHORIZED, "invalid bearer token"));
    }
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut output, byte| {
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
        output
    })
}

async fn wait_for_shutdown_signal(mut shutdown_rx: tokio::sync::watch::Receiver<bool>) {
    while !*shutdown_rx.borrow_and_update() {
        if shutdown_rx.changed().await.is_err() {
            break;
        }
    }
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self { status, message: message.into() }
    }

    fn internal(error: impl std::fmt::Display) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
    }

    fn from_status(status: tonic::Status) -> Self {
        let code = match status.code() {
            tonic::Code::InvalidArgument => StatusCode::BAD_REQUEST,
            tonic::Code::Unimplemented => StatusCode::NOT_IMPLEMENTED,
            tonic::Code::Unauthenticated => StatusCode::UNAUTHORIZED,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self::new(code, status.message())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (self.status, Json(ErrorDto { error: self.message })).into_response()
    }
}

#[derive(Serialize)]
struct ErrorDto {
    error: String,
}

#[derive(Serialize)]
struct StatusDto {
    alias: String,
    fingerprint: String,
    version: String,
    inbox_dir: String,
    localsend_port: u16,
    native_port: u16,
}

#[derive(Serialize)]
struct TrustedPeersDto {
    peers: Vec<TrustedPeerDto>,
}

#[derive(Serialize)]
struct TrustedPeerDto {
    fingerprint: String,
    pubkey_hex: String,
    label: String,
    trusted_at_unix_seconds: i64,
    last_seen_unix_seconds: Option<i64>,
    native_certificate_fingerprint: Option<String>,
    policy: &'static str,
}

#[derive(Serialize)]
struct LanPeersDto {
    peers: Vec<LanPeerDto>,
}

#[derive(Serialize)]
struct LanPeerDto {
    alias: String,
    address: String,
    port: u16,
    protocol: &'static str,
    fingerprint: String,
    device_model: Option<String>,
    device_type: Option<String>,
    download: bool,
}

#[derive(Serialize)]
struct LocalSendPeersDto {
    peers: Vec<LocalSendPeerDto>,
}

#[derive(Serialize)]
struct LocalSendPeerDto {
    fingerprint: String,
    alias: String,
    label: Option<String>,
    status: &'static str,
    first_seen_unix_seconds: i64,
    last_seen_unix_seconds: i64,
    attempt_count: u64,
    source_ip: Option<String>,
}

#[derive(Deserialize)]
struct ApproveLocalSendPeerDto {
    #[serde(default)]
    label: Option<String>,
}

#[derive(Serialize)]
struct InboxDto {
    entries: Vec<InboxEntryDto>,
}

#[derive(Serialize)]
struct InboxEntryDto {
    file_name: String,
    path: String,
    size: u64,
    modified_unix_seconds: i64,
}

#[derive(Serialize)]
struct ActiveTransfersDto {
    transfers: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct SendTransferDto {
    paths: Vec<String>,
    #[serde(default)]
    localsend_url: Option<String>,
    #[serde(default)]
    native_url: Option<String>,
    #[serde(default)]
    peer_fingerprint: Option<String>,
    #[serde(default)]
    wan_peer: Option<String>,
    #[serde(default)]
    native: bool,
}

impl SendTransferDto {
    fn into_proto(self) -> Result<SendRequest, ApiError> {
        let target_count = self.localsend_url.is_some() as u8
            + self.native_url.is_some() as u8
            + self.peer_fingerprint.is_some() as u8
            + self.wan_peer.is_some() as u8;
        if target_count != 1 {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "exactly one transfer target is required",
            ));
        }
        if self.native && self.localsend_url.is_some() {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "native transfer target cannot use localsend_url",
            ));
        }

        let target = self
            .localsend_url
            .map(send_request::Target::LocalsendUrl)
            .or_else(|| self.native_url.map(send_request::Target::NativeUrl))
            .or_else(|| self.peer_fingerprint.map(send_request::Target::PeerFingerprint))
            .or_else(|| self.wan_peer.map(send_request::Target::WanPeer));
        Ok(SendRequest { paths: self.paths, target, native: self.native })
    }
}

#[derive(Serialize)]
struct SendResponseDto {
    transfer_id: String,
}

#[derive(Serialize)]
struct TransferStateDto {
    transfer_id: String,
    state: &'static str,
}

#[derive(Serialize)]
struct CancelResponseDto {
    transfer_id: String,
    state: &'static str,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use clap::Parser;
    use futures_util::StreamExt;
    use lsi_core::{
        api_token::ApiToken,
        config::{AppConfig, MetricsConfig},
        identity::{Fingerprint, Keypair},
    };
    use metrics_exporter_prometheus::PrometheusBuilder;
    use reqwest::StatusCode;

    use super::*;
    use crate::{events::DaemonEvent, Args};

    #[tokio::test]
    async fn http_rejects_missing_bearer_token() {
        let fixture = HttpFixture::start().await;
        let response =
            reqwest::get(format!("http://{}/api/v1/status", fixture.runtime.local_addr()))
                .await
                .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        fixture.stop().await;
    }

    #[tokio::test]
    async fn http_rejects_bad_bearer_token() {
        let fixture = HttpFixture::start().await;
        let client = reqwest::Client::new();
        let response = client
            .get(format!("http://{}/api/v1/status", fixture.runtime.local_addr()))
            .bearer_auth("wrong-token")
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        fixture.stop().await;
    }

    #[tokio::test]
    async fn http_status_returns_json_with_valid_bearer_token() {
        let fixture = HttpFixture::start().await;
        let client = reqwest::Client::new();
        let body: serde_json::Value = client
            .get(format!("http://{}/api/v1/status", fixture.runtime.local_addr()))
            .bearer_auth("test-token")
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();

        assert_eq!(body["alias"], "http-test");
        assert_eq!(body["fingerprint"], fixture.fingerprint);
        fixture.stop().await;
    }

    #[tokio::test]
    async fn http_lists_and_approves_pending_localsend_peers() {
        let fixture = HttpFixture::start().await;
        TrustStore::open(&fixture.state.trust_db_path)
            .unwrap()
            .record_pending_localsend_peer("AA:BB", "Diego iPhone", Some("10.16.20.53"))
            .unwrap();
        let client = reqwest::Client::new();

        let pending: serde_json::Value = client
            .get(format!("http://{}/api/v1/localsend/pending-peers", fixture.runtime.local_addr()))
            .bearer_auth("test-token")
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();

        assert_eq!(pending["peers"][0]["fingerprint"], "aabb");
        assert_eq!(pending["peers"][0]["status"], "pending");

        let approved: serde_json::Value = client
            .post(format!(
                "http://{}/api/v1/localsend/pending-peers/aa-bb/approve",
                fixture.runtime.local_addr()
            ))
            .bearer_auth("test-token")
            .json(&serde_json::json!({ "label": "personal phone" }))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();

        assert_eq!(approved["status"], "trusted");
        assert_eq!(approved["label"], "personal phone");
        fixture.stop().await;
    }

    #[tokio::test]
    async fn healthz_returns_ok_without_bearer_token() {
        let fixture = HttpFixture::start().await;
        let response =
            reqwest::get(format!("http://{}/healthz", fixture.runtime.local_addr())).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        fixture.stop().await;
    }

    #[tokio::test]
    async fn readyz_returns_ok_without_bearer_token() {
        let fixture = HttpFixture::start().await;
        let response =
            reqwest::get(format!("http://{}/readyz", fixture.runtime.local_addr())).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        fixture.stop().await;
    }

    #[tokio::test]
    async fn metrics_returns_prometheus_text_without_bearer_token_when_enabled() {
        let fixture = HttpFixture::start_with_metrics().await;
        let response =
            reqwest::get(format!("http://{}/metrics", fixture.runtime.local_addr())).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("text/plain"));
        fixture.stop().await;
    }

    #[tokio::test]
    async fn metrics_returns_not_found_without_bearer_token_when_disabled() {
        let fixture = HttpFixture::start().await;
        let response =
            reqwest::get(format!("http://{}/metrics", fixture.runtime.local_addr())).await.unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        fixture.stop().await;
    }

    #[tokio::test]
    async fn http_serves_webui_index_without_bearer_token() {
        let fixture = HttpFixture::start().await;
        let response =
            reqwest::get(format!("http://{}/", fixture.runtime.local_addr())).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(reqwest::header::CONTENT_TYPE).unwrap(),
            "text/html; charset=utf-8"
        );
        assert!(response.text().await.unwrap().contains("<div id=\"app\">"));
        fixture.stop().await;
    }

    #[tokio::test]
    async fn http_spa_fallback_serves_index_without_bearer_token() {
        let fixture = HttpFixture::start().await;
        let response = reqwest::get(format!("http://{}/dashboard", fixture.runtime.local_addr()))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.text().await.unwrap().contains("<div id=\"app\">"));
        fixture.stop().await;
    }

    #[tokio::test]
    async fn http_send_rejects_conflicting_targets() {
        let fixture = HttpFixture::start().await;
        let client = reqwest::Client::new();
        let response = client
            .post(format!("http://{}/api/v1/transfers/send", fixture.runtime.local_addr()))
            .bearer_auth("test-token")
            .json(&serde_json::json!({
                "paths": [],
                "localsend_url": "http://127.0.0.1:53317",
                "native_url": "quic://127.0.0.1:53400"
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body: serde_json::Value = response.json().await.unwrap();
        assert!(body["error"].as_str().unwrap().contains("exactly one transfer target"));
        fixture.stop().await;
    }

    #[tokio::test]
    async fn http_events_streams_sse_events() {
        let fixture = HttpFixture::start().await;
        let client = reqwest::Client::new();
        let response = client
            .get(format!("http://{}/api/v1/events", fixture.runtime.local_addr()))
            .bearer_auth("test-token")
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        let read_task = tokio::spawn(async move {
            let mut stream = response.bytes_stream();
            let mut observed = Vec::new();
            while let Some(chunk) =
                tokio::time::timeout(Duration::from_secs(2), stream.next()).await.unwrap()
            {
                let chunk = chunk.unwrap();
                observed.extend_from_slice(&chunk);
                let text = String::from_utf8_lossy(&observed);
                if text.contains("event: daemon") || text.contains("event:daemon") {
                    return observed;
                }
            }
            observed
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        fixture.state.events.emit(DaemonEvent::InboxChanged);
        let observed = read_task.await.unwrap();

        assert!(String::from_utf8_lossy(&observed).contains("data:"));
        fixture.stop().await;
    }

    #[tokio::test]
    async fn http_events_include_transfer_payload() {
        let fixture = HttpFixture::start().await;
        let client = reqwest::Client::new();
        let response = client
            .get(format!("http://{}/api/v1/events", fixture.runtime.local_addr()))
            .bearer_auth("test-token")
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        let read_task = tokio::spawn(async move {
            let mut stream = response.bytes_stream();
            let mut observed = Vec::new();
            while let Some(chunk) =
                tokio::time::timeout(Duration::from_secs(2), stream.next()).await.unwrap()
            {
                let chunk = chunk.unwrap();
                observed.extend_from_slice(&chunk);
                let text = String::from_utf8_lossy(&observed);
                if text.contains("transfer-123") {
                    return observed;
                }
            }
            observed
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        fixture
            .state
            .events
            .emit(DaemonEvent::TransferStarted { transfer_id: "transfer-123".to_string() });
        let observed = read_task.await.unwrap();

        let text = String::from_utf8_lossy(&observed);
        assert!(text.contains("transfer_started"), "{text}");
        assert!(text.contains("transfer-123"), "{text}");
        fixture.stop().await;
    }

    struct HttpFixture {
        runtime: HttpApiRuntime,
        state: Arc<DaemonState>,
        fingerprint: String,
        _temp: tempfile::TempDir,
    }

    impl HttpFixture {
        async fn start() -> Self {
            Self::start_with_config(AppConfig::default(), None).await
        }

        async fn start_with_metrics() -> Self {
            let config = AppConfig {
                metrics: MetricsConfig { enabled: true, ..MetricsConfig::default() },
                ..AppConfig::default()
            };
            let handle = PrometheusBuilder::new().build_recorder().handle();

            Self::start_with_config(config, Some(handle)).await
        }

        async fn start_with_config(
            config: AppConfig,
            metrics: Option<metrics_exporter_prometheus::PrometheusHandle>,
        ) -> Self {
            let temp = tempfile::TempDir::new().unwrap();
            let args = Args::parse_from([
                "daemon",
                "--alias",
                "http-test",
                "--inbox",
                temp.path().join("inbox").to_str().unwrap(),
                "--trust-db",
                temp.path().join("trust.db").to_str().unwrap(),
            ]);
            let identity = Keypair::generate();
            let fingerprint = Fingerprint::from_pubkey(&identity.public_bytes()).to_string();
            let state = Arc::new(DaemonState::from_args_with_metrics(
                &args,
                identity,
                fingerprint.clone(),
                ApiToken::new("test-token").unwrap(),
                config,
                metrics,
            ));
            let runtime = start_http_runtime(Arc::clone(&state), 0).await.unwrap();
            Self { runtime, state, fingerprint, _temp: temp }
        }

        async fn stop(self) {
            stop_http_runtime(self.runtime).await.unwrap();
        }
    }
}
