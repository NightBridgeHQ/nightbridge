//! HTTP upload API server for LocalSend v2.

use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::TryStreamExt;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as ConnectionBuilder;
use hyper_util::service::TowerToHyperService;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use serde::Deserialize;
use serde_json::json;
use tokio::io::AsyncRead;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_util::io::StreamReader;
use tracing::{debug, warn};

use crate::dto::{
    DeviceInfo, PrepareUploadRequest, PrepareUploadResponse, RegisterRequest, RegisterResponse,
};
use crate::inbox::InboxWriter;
use crate::session::SessionManager;
use crate::tls::TlsIdentity;
use crate::{LocalSendError, Result};

const API_PREFIX: &str = "/api/localsend/v2";
const LEGACY_API_PREFIX: &str = "/api/localsend/v1";

/// Configuration for the LocalSend v2 upload API server.
#[derive(Clone, Debug)]
pub struct LocalSendServerConfig {
    /// Socket address to bind. Use port `0` to request an ephemeral port.
    pub bind_addr: SocketAddr,
    /// Device information returned by `/info` and `/register`.
    pub info: DeviceInfo,
    /// Local inbox directory where completed uploads are published.
    pub inbox_dir: PathBuf,
    /// Upload session lifetime.
    pub session_ttl: Duration,
    /// Optional TLS identity. When set, the listener serves HTTPS with rustls.
    pub tls_identity: Option<TlsIdentity>,
}

/// Bound LocalSend v2 upload API server.
#[derive(Debug)]
pub struct LocalSendServer {
    listener: TcpListener,
    state: ServerState,
    tls_identity: Option<TlsIdentity>,
}

#[derive(Clone, Debug)]
struct ServerState {
    info: DeviceInfo,
    sessions: SessionManager,
    inbox: InboxWriter,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UploadQuery {
    session_id: String,
    file_id: String,
    token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacySendQuery {
    file_id: String,
    token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CancelQuery {
    session_id: String,
}

impl LocalSendServer {
    /// Binds a LocalSend v2 server to the configured address.
    pub async fn bind(config: LocalSendServerConfig) -> Result<Self> {
        let listener = TcpListener::bind(config.bind_addr).await?;
        let state = ServerState {
            info: config.info,
            sessions: SessionManager::new(config.session_ttl),
            inbox: InboxWriter::new(config.inbox_dir),
        };

        Ok(Self { listener, state, tls_identity: config.tls_identity })
    }

    /// Returns the local socket address selected by the OS.
    pub fn local_addr(&self) -> SocketAddr {
        self.listener.local_addr().expect("bound TCP listener should always expose a local address")
    }

    /// Serves requests until the supplied shutdown future resolves.
    pub async fn serve_until_shutdown<S>(self, shutdown: S) -> Result<()>
    where
        S: Future<Output = ()> + Send + 'static,
    {
        if let Some(identity) = self.tls_identity {
            serve_tls(self.listener, self.state, identity, shutdown).await
        } else {
            let app = router(self.state);
            axum::serve(self.listener, app)
                .with_graceful_shutdown(shutdown)
                .await
                .map_err(LocalSendError::Io)
        }
    }
}

fn router(state: ServerState) -> Router {
    Router::new()
        .route(&format!("{API_PREFIX}/info"), get(info))
        .route(&format!("{API_PREFIX}/register"), post(register))
        .route(&format!("{API_PREFIX}/prepare-upload"), post(prepare_upload))
        .route(&format!("{API_PREFIX}/upload"), post(upload))
        .route(&format!("{API_PREFIX}/cancel"), post(cancel).delete(cancel))
        .route(&format!("{LEGACY_API_PREFIX}/info"), get(legacy_info))
        .route(&format!("{LEGACY_API_PREFIX}/register"), post(legacy_register))
        .route(&format!("{LEGACY_API_PREFIX}/send-request"), post(legacy_send_request))
        .route(&format!("{LEGACY_API_PREFIX}/send"), post(legacy_send))
        .route(&format!("{LEGACY_API_PREFIX}/cancel"), post(legacy_cancel))
        .with_state(Arc::new(state))
}

async fn info(State(state): State<Arc<ServerState>>) -> Json<DeviceInfo> {
    Json(state.info.clone())
}

async fn legacy_info(State(state): State<Arc<ServerState>>) -> Json<serde_json::Value> {
    Json(legacy_device_info(&state.info))
}

async fn register(
    State(state): State<Arc<ServerState>>,
    Json(_request): Json<RegisterRequest>,
) -> Json<RegisterResponse> {
    Json(state.info.clone())
}

async fn legacy_register(
    State(state): State<Arc<ServerState>>,
    Json(_request): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    Json(legacy_device_info(&state.info))
}

async fn legacy_send_request(
    State(state): State<Arc<ServerState>>,
    Json(request): Json<PrepareUploadRequest>,
) -> Response {
    match state.sessions.prepare(normalize_legacy_prepare_request(request)) {
        Ok(prepared) => Json(prepared.files).into_response(),
        Err(error) => api_error(error).into_response(),
    }
}

async fn legacy_send(
    State(state): State<Arc<ServerState>>,
    Query(query): Query<LegacySendQuery>,
    body: Body,
) -> Response {
    let session_id = match state.sessions.session_id_for_file(&query.file_id) {
        Ok(Some(session_id)) => session_id,
        Ok(None) => return StatusCode::FORBIDDEN.into_response(),
        Err(error) => return api_error(error).into_response(),
    };

    write_upload(
        state,
        UploadQuery { session_id, file_id: query.file_id, token: query.token },
        body,
    )
    .await
}

async fn legacy_cancel(State(state): State<Arc<ServerState>>) -> Response {
    match state.sessions.cancel_all() {
        Ok(_) => StatusCode::OK.into_response(),
        Err(error) => api_error(error).into_response(),
    }
}

fn legacy_device_info(info: &DeviceInfo) -> serde_json::Value {
    json!({
        "alias": info.alias,
        "deviceModel": info.device_model,
        "deviceType": info.device_type,
        "fingerprint": info.fingerprint,
    })
}

async fn prepare_upload(
    State(state): State<Arc<ServerState>>,
    Json(request): Json<PrepareUploadRequest>,
) -> Response {
    match state.sessions.prepare(request) {
        Ok(prepared) => {
            Json(PrepareUploadResponse { session_id: prepared.session_id, files: prepared.files })
                .into_response()
        }
        Err(error) => api_error(error).into_response(),
    }
}

async fn upload(
    State(state): State<Arc<ServerState>>,
    Query(query): Query<UploadQuery>,
    body: Body,
) -> Response {
    write_upload(state, query, body).await
}

async fn write_upload(state: Arc<ServerState>, query: UploadQuery, body: Body) -> Response {
    let authorized =
        match state.sessions.authorize_upload(&query.session_id, &query.file_id, &query.token) {
            Ok(upload) => upload,
            Err(error) => return api_error(error).into_response(),
        };

    let reader = body_reader(body);

    match state.inbox.write_upload(&authorized, reader).await {
        Ok(_) => match state.sessions.mark_uploaded(&query.session_id, &query.file_id) {
            Ok(()) => {
                debug!(session_id = %query.session_id, file_id = %query.file_id, "upload completed");
                StatusCode::OK.into_response()
            }
            Err(error) => api_error(error).into_response(),
        },
        Err(error) => {
            warn!(session_id = %query.session_id, file_id = %query.file_id, error = %error, "upload failed");
            api_error(error).into_response()
        }
    }
}

async fn cancel(
    State(state): State<Arc<ServerState>>,
    Query(query): Query<CancelQuery>,
) -> Response {
    match state.sessions.cancel(&query.session_id) {
        Ok(true) => StatusCode::OK.into_response(),
        Ok(false) => StatusCode::CONFLICT.into_response(),
        Err(error) => api_error(error).into_response(),
    }
}

fn api_error(error: LocalSendError) -> StatusCode {
    match error {
        LocalSendError::Json(_) => StatusCode::BAD_REQUEST,
        LocalSendError::Session(_) => StatusCode::FORBIDDEN,
        LocalSendError::Crypto(_) => StatusCode::BAD_REQUEST,
        LocalSendError::Http(_) => StatusCode::BAD_REQUEST,
        LocalSendError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn serve_tls<S>(
    listener: TcpListener,
    state: ServerState,
    identity: TlsIdentity,
    shutdown: S,
) -> Result<()>
where
    S: Future<Output = ()> + Send + 'static,
{
    let acceptor = TlsAcceptor::from(Arc::new(rustls_server_config(identity)?));
    let state = Arc::new(state);
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => return Ok(()),
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let acceptor = acceptor.clone();
                let state = Arc::clone(&state);

                tokio::spawn(async move {
                    let Ok(tls_stream) = acceptor.accept(stream).await else {
                        return;
                    };
                    let service = TowerToHyperService::new(router((*state).clone()));
                    let io = TokioIo::new(tls_stream);
                    let _ = ConnectionBuilder::new(TokioExecutor::new())
                        .serve_connection_with_upgrades(io, service)
                        .await;
                });
            }
        }
    }
}

fn rustls_server_config(identity: TlsIdentity) -> Result<rustls::ServerConfig> {
    let cert_chain = vec![CertificateDer::from(identity.cert_der)];
    let private_key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identity.key_der));

    rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, private_key)
        .map_err(|error| LocalSendError::Crypto(error.to_string()))
}

fn body_reader(body: Body) -> impl AsyncRead + Unpin {
    let stream =
        body.into_data_stream().map_err(|error| io::Error::new(io::ErrorKind::Other, error));
    StreamReader::new(stream)
}

fn normalize_legacy_prepare_request(mut request: PrepareUploadRequest) -> PrepareUploadRequest {
    for (file_id, meta) in &mut request.files {
        if meta.id != *file_id {
            meta.id.clone_from(file_id);
        }
        meta.file_type =
            meta.file_type.as_ref().map(|file_type| normalize_legacy_file_type(file_type));
    }
    request
}

fn normalize_legacy_file_type(file_type: &str) -> String {
    match file_type {
        "image" => "image/*".to_string(),
        "video" => "video/*".to_string(),
        "pdf" => "application/pdf".to_string(),
        "text" => "text/plain".to_string(),
        "other" => "application/octet-stream".to_string(),
        value => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::net::SocketAddr;
    use std::time::Duration;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use crate::dto::{
        DeviceInfo, FileMeta, PrepareUploadRequest, PrepareUploadResponse, Protocol,
        RegisterResponse,
    };
    use crate::server::{LocalSendServer, LocalSendServerConfig};
    use crate::tls::TlsIdentity;

    fn device_info(alias: &str, port: u16) -> DeviceInfo {
        DeviceInfo {
            alias: alias.to_string(),
            version: "2.0".to_string(),
            device_model: Some("test-rig".to_string()),
            device_type: Some("desktop".to_string()),
            fingerprint: "test-fingerprint".to_string(),
            port,
            protocol: Protocol::from("http"),
            download: true,
        }
    }

    fn file_meta(id: &str, file_name: &str, size: u64) -> FileMeta {
        FileMeta {
            id: id.to_string(),
            file_name: file_name.to_string(),
            size,
            file_type: Some("text/plain".to_string()),
            sha256: None,
            preview: None,
            metadata: None,
        }
    }

    async fn spawn_test_server(
        inbox_dir: impl Into<std::path::PathBuf>,
    ) -> (SocketAddr, tokio::sync::oneshot::Sender<()>, tokio::task::JoinHandle<crate::Result<()>>)
    {
        let config = LocalSendServerConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            info: device_info("Receiver", 0),
            inbox_dir: inbox_dir.into(),
            session_ttl: Duration::from_secs(60),
            tls_identity: None,
        };
        let server = LocalSendServer::bind(config).await.unwrap();
        let addr = server.local_addr();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(server.serve_until_shutdown(async {
            let _ = shutdown_rx.await;
        }));

        (addr, shutdown_tx, task)
    }

    async fn spawn_tls_test_server(
        inbox_dir: impl Into<std::path::PathBuf>,
    ) -> (SocketAddr, tokio::sync::oneshot::Sender<()>, tokio::task::JoinHandle<crate::Result<()>>)
    {
        let config = LocalSendServerConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            info: device_info("Receiver", 0),
            inbox_dir: inbox_dir.into(),
            session_ttl: Duration::from_secs(60),
            tls_identity: Some(TlsIdentity::generate("Receiver").unwrap()),
        };
        let server = LocalSendServer::bind(config).await.unwrap();
        let addr = server.local_addr();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(server.serve_until_shutdown(async {
            let _ = shutdown_rx.await;
        }));

        (addr, shutdown_tx, task)
    }

    async fn request(
        addr: SocketAddr,
        method: &str,
        path: &str,
        content_type: Option<&str>,
        body: &[u8],
    ) -> (u16, Vec<u8>) {
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let content_type_header =
            content_type.map(|value| format!("Content-Type: {value}\r\n")).unwrap_or_default();
        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: {addr}\r\n{content_type_header}Content-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        stream.write_all(body).await.unwrap();

        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        parse_response(&response)
    }

    fn parse_response(response: &[u8]) -> (u16, Vec<u8>) {
        let header_end = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .expect("response contains headers")
            + 4;
        let headers = String::from_utf8_lossy(&response[..header_end]);
        let status = headers
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|status| status.parse::<u16>().ok())
            .expect("response contains status code");

        (status, response[header_end..].to_vec())
    }

    async fn prepare_upload(
        addr: SocketAddr,
        file_id: &str,
        file_name: &str,
        body: &[u8],
    ) -> PrepareUploadResponse {
        let prepare_request = PrepareUploadRequest {
            info: device_info("Sender", 53317),
            files: BTreeMap::from([(
                file_id.to_string(),
                file_meta(file_id, file_name, body.len() as u64),
            )]),
        };

        let (status, body) = request(
            addr,
            "POST",
            "/api/localsend/v2/prepare-upload",
            Some("application/json"),
            &serde_json::to_vec(&prepare_request).unwrap(),
        )
        .await;

        assert_eq!(status, 200);
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn info_returns_device_info() {
        let temp = tempfile::tempdir().unwrap();
        let (addr, shutdown, task) = spawn_test_server(temp.path()).await;

        let (status, body) = request(addr, "GET", "/api/localsend/v2/info", None, &[]).await;

        assert_eq!(status, 200);
        let info: DeviceInfo = serde_json::from_slice(&body).unwrap();
        assert_eq!(info.alias, "Receiver");
        assert_eq!(info.protocol.as_str(), "http");

        let _ = shutdown.send(());
        task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn register_returns_device_info() {
        let temp = tempfile::tempdir().unwrap();
        let (addr, shutdown, task) = spawn_test_server(temp.path()).await;
        let body = serde_json::to_vec(&device_info("Sender", 53317)).unwrap();

        let (status, body) =
            request(addr, "POST", "/api/localsend/v2/register", Some("application/json"), &body)
                .await;

        assert_eq!(status, 200);
        let response: RegisterResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(response.alias, "Receiver");

        let _ = shutdown.send(());
        task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn legacy_info_returns_v1_device_info() {
        let temp = tempfile::tempdir().unwrap();
        let (addr, shutdown, task) = spawn_test_server(temp.path()).await;

        let (status, body) =
            request(addr, "GET", "/api/localsend/v1/info?fingerprint=phone", None, &[]).await;

        assert_eq!(status, 200);
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(response["alias"], "Receiver");
        assert_eq!(response["deviceModel"], "test-rig");
        assert_eq!(response["deviceType"], "desktop");

        let _ = shutdown.send(());
        task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn prepare_upload_returns_session_and_tokens() {
        let temp = tempfile::tempdir().unwrap();
        let (addr, shutdown, task) = spawn_test_server(temp.path()).await;

        let response = prepare_upload(addr, "file-1", "note.txt", b"hello").await;

        assert!(!response.session_id.is_empty());
        assert!(!response.files["file-1"].is_empty());

        let _ = shutdown.send(());
        task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn upload_with_valid_token_writes_file() {
        let temp = tempfile::tempdir().unwrap();
        let (addr, shutdown, task) = spawn_test_server(temp.path()).await;
        let body = b"hello";
        let prepared = prepare_upload(addr, "file-1", "note.txt", body).await;
        let token = &prepared.files["file-1"];
        let path = format!(
            "/api/localsend/v2/upload?sessionId={}&fileId=file-1&token={}",
            prepared.session_id, token
        );

        let (status, _) = request(addr, "POST", &path, Some("text/plain"), body).await;

        assert_eq!(status, 200);
        let written = tokio::fs::read_to_string(temp.path().join("note.txt")).await.unwrap();
        assert_eq!(written, "hello");

        let _ = shutdown.send(());
        task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn upload_with_invalid_token_returns_403() {
        let temp = tempfile::tempdir().unwrap();
        let (addr, shutdown, task) = spawn_test_server(temp.path()).await;
        let body = b"hello";
        let prepared = prepare_upload(addr, "file-1", "note.txt", body).await;
        let path = format!(
            "/api/localsend/v2/upload?sessionId={}&fileId=file-1&token=wrong",
            prepared.session_id
        );

        let (status, _) = request(addr, "POST", &path, Some("text/plain"), body).await;

        assert_eq!(status, 403);
        assert!(!temp.path().join("note.txt").exists());

        let _ = shutdown.send(());
        task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn cancel_removes_session() {
        let temp = tempfile::tempdir().unwrap();
        let (addr, shutdown, task) = spawn_test_server(temp.path()).await;
        let body = b"hello";
        let prepared = prepare_upload(addr, "file-1", "note.txt", body).await;
        let cancel_path = format!("/api/localsend/v2/cancel?sessionId={}", prepared.session_id);
        let upload_path = format!(
            "/api/localsend/v2/upload?sessionId={}&fileId=file-1&token={}",
            prepared.session_id, prepared.files["file-1"]
        );

        let (cancel_status, _) = request(addr, "POST", &cancel_path, None, &[]).await;
        let (upload_status, _) =
            request(addr, "POST", &upload_path, Some("text/plain"), body).await;

        assert_eq!(cancel_status, 200);
        assert_eq!(upload_status, 403);
        assert!(!temp.path().join("note.txt").exists());

        let _ = shutdown.send(());
        task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn legacy_upload_over_tls_writes_file_body() {
        let temp = tempfile::tempdir().unwrap();
        let (addr, shutdown, task) = spawn_tls_test_server(temp.path()).await;
        let client = reqwest::Client::builder().danger_accept_invalid_certs(true).build().unwrap();
        let body = b"%PDF-1.7\nhello\n";
        let prepare_request = PrepareUploadRequest {
            info: device_info("Sender", 53317),
            files: BTreeMap::from([(
                "legacy-file".to_string(),
                file_meta("legacy-file", "note.pdf", body.len() as u64),
            )]),
        };

        let tokens: BTreeMap<String, String> = client
            .post(format!("https://{addr}/api/localsend/v1/send-request"))
            .json(&prepare_request)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        let response = client
            .post(format!(
                "https://{addr}/api/localsend/v1/send?fileId=legacy-file&token={}",
                tokens["legacy-file"]
            ))
            .body(body.to_vec())
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let written = tokio::fs::read(temp.path().join("note.pdf")).await.unwrap();
        assert_eq!(written, body);

        let _ = shutdown.send(());
        task.await.unwrap().unwrap();
    }
}
