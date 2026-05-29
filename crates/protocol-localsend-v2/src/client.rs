//! LocalSend v2 outbound upload client.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use reqwest::Url;
use sha2::{Digest, Sha256};
use tokio::fs;

use crate::discovery::LanPeer;
use crate::dto::{DeviceInfo, FileMeta, PrepareUploadRequest, PrepareUploadResponse};
use crate::{LocalSendError, Result};

#[derive(Clone, Debug)]
struct PendingFile {
    path: PathBuf,
    meta: FileMeta,
}

/// LocalSend v2 upload client.
#[derive(Clone, Debug)]
pub struct LocalSendClient {
    client: reqwest::Client,
}

impl LocalSendClient {
    /// Creates a client without certificate pinning. Suitable only for plaintext
    /// HTTP; HTTPS peers must be reached via [`LocalSendClient::pinned`] so the
    /// self-signed certificate is bound to the discovered fingerprint (H-3).
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .build()
            .map_err(|error| LocalSendError::Http(error.to_string()))?;
        Ok(Self { client })
    }

    /// Creates a client that pins the peer's self-signed certificate to
    /// `expected_fingerprint` (the SHA-256 advertised during discovery).
    pub fn pinned(expected_fingerprint: &str) -> Result<Self> {
        Ok(Self { client: crate::verifier::pinned_client(expected_fingerprint)? })
    }

    /// Sends files to a discovered LAN peer using the LocalSend v2 Upload API,
    /// pinning the peer's advertised certificate fingerprint (H-3).
    pub async fn send_files(peer: LanPeer, paths: Vec<PathBuf>, sender: DeviceInfo) -> Result<()> {
        let base_url = format!("{}://{}", peer.info.protocol.as_str(), peer.address);
        Self::pinned(&peer.info.fingerprint)?.send_files_to_url(&base_url, paths, sender).await
    }

    /// Sends files to an explicit LocalSend v2 API base URL.
    pub async fn send_files_to_url(
        &self,
        base_url: &str,
        paths: Vec<PathBuf>,
        sender: DeviceInfo,
    ) -> Result<()> {
        if paths.is_empty() {
            return Err(LocalSendError::Http("at least one file is required".to_string()));
        }

        let pending_files = pending_files(paths).await?;
        let files = pending_files
            .iter()
            .map(|(file_id, pending)| (file_id.clone(), pending.meta.clone()))
            .collect();
        let request = PrepareUploadRequest { info: sender, files };
        let prepared: PrepareUploadResponse = self
            .client
            .post(api_url(base_url, "/api/localsend/v2/prepare-upload")?)
            .json(&request)
            .send()
            .await
            .map_err(|error| LocalSendError::Http(error.to_string()))?
            .error_for_status()
            .map_err(|error| LocalSendError::Http(error.to_string()))?
            .json()
            .await
            .map_err(|error| LocalSendError::Http(error.to_string()))?;

        for (file_id, token) in prepared.files {
            let pending = pending_files.get(&file_id).ok_or_else(|| {
                LocalSendError::Http(format!("server returned unknown file id {file_id}"))
            })?;
            let bytes = fs::read(&pending.path).await?;
            let mut url = api_url(base_url, "/api/localsend/v2/upload")?;
            url.query_pairs_mut()
                .append_pair("sessionId", &prepared.session_id)
                .append_pair("fileId", &file_id)
                .append_pair("token", &token);

            self.client
                .post(url)
                .header(
                    reqwest::header::CONTENT_TYPE,
                    pending.meta.file_type.as_deref().unwrap_or("application/octet-stream"),
                )
                .body(bytes)
                .send()
                .await
                .map_err(|error| LocalSendError::Http(error.to_string()))?
                .error_for_status()
                .map_err(|error| LocalSendError::Http(error.to_string()))?;
        }

        Ok(())
    }
}

async fn pending_files(paths: Vec<PathBuf>) -> Result<BTreeMap<String, PendingFile>> {
    let mut files = BTreeMap::new();

    for (index, path) in paths.into_iter().enumerate() {
        let metadata = fs::metadata(&path).await?;
        if !metadata.is_file() {
            return Err(LocalSendError::Http(format!("{} is not a file", path.display())));
        }

        let bytes = fs::read(&path).await?;
        let file_name = file_name(&path)?;
        let file_id = format!("file-{index}");
        files.insert(
            file_id.clone(),
            PendingFile {
                path: path.clone(),
                meta: FileMeta {
                    id: file_id,
                    file_name: file_name.clone(),
                    size: metadata.len(),
                    file_type: mime_guess::from_path(&path).first_raw().map(str::to_string),
                    sha256: Some(sha256_hex(&bytes)),
                    preview: None,
                    metadata: None,
                },
            },
        );
    }

    Ok(files)
}

fn file_name(path: &Path) -> Result<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .ok_or_else(|| LocalSendError::Http(format!("invalid file path {}", path.display())))
}

fn api_url(base_url: &str, path: &str) -> Result<Url> {
    let mut url = Url::parse(base_url).map_err(|error| LocalSendError::Http(error.to_string()))?;
    url.set_path(path);
    url.set_query(None);
    Ok(url)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().fold(String::with_capacity(64), |mut hex, byte| {
        use std::fmt::Write as _;
        write!(&mut hex, "{byte:02x}").expect("writing to string cannot fail");
        hex
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::net::SocketAddr;
    use std::path::Path;
    use std::time::Duration;

    use tempfile::TempDir;

    use crate::client::LocalSendClient;
    use crate::dto::{DeviceInfo, Protocol};
    use crate::server::{LocalSendReceivePolicy, LocalSendServer, LocalSendServerConfig};

    fn device_info(alias: &str, port: u16) -> DeviceInfo {
        DeviceInfo {
            alias: alias.to_string(),
            version: "2.0".to_string(),
            device_model: Some("test-rig".to_string()),
            device_type: Some("desktop".to_string()),
            fingerprint: format!("{alias}-fingerprint"),
            port,
            protocol: Protocol::from("http"),
            download: true,
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
            receive_policy: LocalSendReceivePolicy::Auto,
            trusted_fingerprints: Default::default(),
            trusted_fingerprints_file: None,
            trust_db_path: None,
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

    #[tokio::test]
    async fn client_posts_prepare_upload_and_uploads_file() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("note.txt");
        tokio::fs::write(&source, b"hello").await.unwrap();
        let (addr, shutdown, task) = spawn_test_server(temp.path().join("inbox")).await;

        LocalSendClient::new()
            .unwrap()
            .send_files_to_url(
                &format!("http://{addr}"),
                vec![source],
                device_info("Sender", 53317),
            )
            .await
            .unwrap();

        let written = tokio::fs::read_to_string(temp.path().join("inbox/note.txt")).await.unwrap();
        assert_eq!(written, "hello");

        let _ = shutdown.send(());
        task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn client_rejects_missing_file() {
        let client = LocalSendClient::new().unwrap();
        let result = client
            .send_files_to_url(
                "http://127.0.0.1:9",
                vec![Path::new("missing.txt").to_path_buf()],
                device_info("Sender", 53317),
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn client_reports_server_rejection() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("note.txt");
        tokio::fs::write(&source, b"hello").await.unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer).await.unwrap();
            stream.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n").await.unwrap();
        });

        let result = LocalSendClient::new()
            .unwrap()
            .send_files_to_url(
                &format!("http://{addr}"),
                vec![source],
                device_info("Sender", 53317),
            )
            .await;

        assert!(result.is_err());
        server.await.unwrap();
    }

    #[tokio::test]
    async fn file_metas_use_file_names_and_hashes() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("note.txt");
        tokio::fs::write(&source, b"hello").await.unwrap();

        let metas: BTreeMap<_, _> = super::pending_files(vec![source]).await.unwrap();

        assert_eq!(metas["file-0"].meta.file_name, "note.txt");
        assert_eq!(metas["file-0"].meta.size, 5);
        assert_eq!(
            metas["file-0"].meta.sha256.as_deref(),
            Some("2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824")
        );
    }

    async fn spawn_https_test_server(
        inbox_dir: impl Into<std::path::PathBuf>,
        identity: crate::tls::TlsIdentity,
    ) -> (SocketAddr, tokio::sync::oneshot::Sender<()>, tokio::task::JoinHandle<crate::Result<()>>)
    {
        let config = LocalSendServerConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            info: device_info("Receiver", 0),
            inbox_dir: inbox_dir.into(),
            session_ttl: Duration::from_secs(60),
            receive_policy: LocalSendReceivePolicy::Auto,
            trusted_fingerprints: Default::default(),
            trusted_fingerprints_file: None,
            trust_db_path: None,
            tls_identity: Some(identity),
        };
        let server = LocalSendServer::bind(config).await.unwrap();
        let addr = server.local_addr();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(server.serve_until_shutdown(async {
            let _ = shutdown_rx.await;
        }));
        (addr, shutdown_tx, task)
    }

    #[tokio::test]
    async fn pinned_client_uploads_over_https_when_fingerprint_matches() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("note.txt");
        tokio::fs::write(&source, b"hello").await.unwrap();
        let identity = crate::tls::TlsIdentity::generate("Receiver").unwrap();
        let fingerprint = identity.fingerprint_sha256_hex();
        let (addr, shutdown, task) =
            spawn_https_test_server(temp.path().join("inbox"), identity).await;

        LocalSendClient::pinned(&fingerprint)
            .unwrap()
            .send_files_to_url(
                &format!("https://{addr}"),
                vec![source],
                device_info("Sender", 53317),
            )
            .await
            .unwrap();

        let written = tokio::fs::read_to_string(temp.path().join("inbox/note.txt")).await.unwrap();
        assert_eq!(written, "hello");

        let _ = shutdown.send(());
        task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn pinned_client_rejects_https_on_fingerprint_mismatch() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("note.txt");
        tokio::fs::write(&source, b"hello").await.unwrap();
        let identity = crate::tls::TlsIdentity::generate("Receiver").unwrap();
        let (addr, shutdown, task) =
            spawn_https_test_server(temp.path().join("inbox"), identity).await;

        // Pin a fingerprint that does not match the server's cert (MITM analogue).
        let wrong = crate::tls::TlsIdentity::generate("attacker").unwrap().fingerprint_sha256_hex();
        let result = LocalSendClient::pinned(&wrong)
            .unwrap()
            .send_files_to_url(
                &format!("https://{addr}"),
                vec![source],
                device_info("Sender", 53317),
            )
            .await;

        assert!(result.is_err(), "a mismatched certificate fingerprint must reject the upload");
        assert!(
            !temp.path().join("inbox/note.txt").exists(),
            "no file should be written when pinning fails"
        );

        let _ = shutdown.send(());
        let _ = task.await;
    }
}
