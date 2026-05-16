//! Native QUIC transfer client.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use lsi_core::identity::Keypair;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::candidates::{CandidateKind, NativeCandidate};
use crate::chunk::{blake3_hex, plan_chunks};
use crate::dto::{default_extensions, DoneTransfer, Hello, PROTOCOL_VERSION};
use crate::framing::{read_control, write_control, ControlMessage};
use crate::hole_punch::dial_candidates;
use crate::tls::ensure_ring_crypto_provider;
use crate::transfer::{write_chunk_frame, NativeTransferSender, TransferChunk};

/// Protocol-native sender for direct QUIC file transfers.
pub struct NativeTransferClient;

impl NativeTransferClient {
    /// Sends files to a peer URL such as `quic://192.168.1.20:53317`.
    pub async fn send_files_to_url(url: &str, paths: Vec<PathBuf>, keypair: Keypair) -> Result<()> {
        let target = native_socket_addr(url)?;
        for path in &paths {
            if !path.is_file() {
                bail!("native send file does not exist: {}", path.display());
            }
        }

        send_native_files(target, paths, keypair).await.context("sending native files")
    }

    /// Sends files to a peer using a resolved WAN candidate list.
    pub async fn send_files_to_candidates(
        remote_candidates: Vec<NativeCandidate>,
        paths: Vec<PathBuf>,
        keypair: Keypair,
    ) -> Result<()> {
        for path in &paths {
            if !path.is_file() {
                bail!("native send file does not exist: {}", path.display());
            }
        }

        let local_candidates = vec![NativeCandidate {
            kind: CandidateKind::Local,
            address: SocketAddr::from(([0, 0, 0, 0], 0)),
            priority: 100,
        }];
        let dialed = dial_candidates(local_candidates, remote_candidates)
            .await
            .map_err(anyhow::Error::new)
            .context("dialing native WAN candidates")?;
        send_native_files_over_connection(&dialed.connection, paths, keypair).await?;
        dialed.connection.close(0_u32.into(), b"done");
        dialed.endpoint.wait_idle().await;
        Ok(())
    }
}

fn native_socket_addr(url: &str) -> Result<SocketAddr> {
    let Some(address) = url.strip_prefix("quic://") else {
        bail!("native --url must start with quic://");
    };
    address.parse::<SocketAddr>().context("native --url must be a quic://host:port socket address")
}

async fn send_native_files(
    target: SocketAddr,
    paths: Vec<PathBuf>,
    keypair: Keypair,
) -> Result<()> {
    let sender = NativeTransferSender::default();
    let request = sender.prepare_files(&paths).await.context("preparing native transfer")?;
    let endpoint = native_client_endpoint()?;
    let connection = endpoint.connect(target, "localhost")?.await?;
    send_prepared_native_files(&connection, request, paths, keypair).await?;
    connection.close(0_u32.into(), b"done");
    endpoint.wait_idle().await;
    Ok(())
}

async fn send_native_files_over_connection(
    connection: &quinn::Connection,
    paths: Vec<PathBuf>,
    keypair: Keypair,
) -> Result<()> {
    let sender = NativeTransferSender::default();
    let request = sender.prepare_files(&paths).await.context("preparing native transfer")?;
    send_prepared_native_files(connection, request, paths, keypair).await
}

async fn send_prepared_native_files(
    connection: &quinn::Connection,
    request: crate::dto::RequestTransfer,
    paths: Vec<PathBuf>,
    keypair: Keypair,
) -> Result<()> {
    let (mut send, mut recv) = connection.open_bi().await?;

    let hello = Hello {
        protocol_version: PROTOCOL_VERSION,
        alias: "localsend-improved".to_string(),
        pubkey: keypair.public_bytes(),
        nonce: request.transfer_id.as_bytes().to_vec(),
        extensions: default_extensions(),
    };
    write_control(&mut send, &ControlMessage::Hello(hello)).await?;
    match read_control(&mut recv).await? {
        ControlMessage::HelloAck(_) => {}
        other => bail!("native peer returned unexpected hello response: {other:?}"),
    }

    write_control(&mut send, &ControlMessage::RequestTransfer(request.clone())).await?;
    let accepted = match read_control(&mut recv).await? {
        ControlMessage::Accept(accepted) if accepted.transfer_id == request.transfer_id => accepted,
        other => bail!("native peer returned unexpected transfer response: {other:?}"),
    };

    for (file, path) in request.files.iter().zip(paths.iter()) {
        send_file_chunks(&mut send, path, &file.file_id, file.size, accepted.chunk_size).await?;
    }
    let done = DoneTransfer { transfer_id: request.transfer_id };
    write_control(&mut send, &ControlMessage::Done(done.clone())).await?;
    match read_control(&mut recv).await? {
        ControlMessage::Done(ack) if ack.transfer_id == done.transfer_id => {}
        other => bail!("native peer returned unexpected completion response: {other:?}"),
    }
    send.finish()?;
    Ok(())
}

async fn send_file_chunks(
    send: &mut quinn::SendStream,
    path: &Path,
    file_id: &str,
    file_size: u64,
    chunk_size: u64,
) -> Result<()> {
    let mut file = tokio::fs::File::open(path).await?;
    for range in plan_chunks(file_size, chunk_size)? {
        file.seek(std::io::SeekFrom::Start(range.offset)).await?;
        let mut bytes = vec![0_u8; range.length as usize];
        file.read_exact(&mut bytes).await?;
        write_chunk_frame(
            send,
            &TransferChunk {
                file_id: file_id.to_string(),
                offset: range.offset,
                blake3: blake3_hex(&bytes),
                bytes,
            },
        )
        .await?;
    }
    Ok(())
}

fn native_client_endpoint() -> Result<quinn::Endpoint> {
    ensure_ring_crypto_provider();
    let mut rustls_config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(TrustAnyServer))
        .with_no_client_auth();
    rustls_config.alpn_protocols = vec![crate::transport::NATIVE_ALPN.to_vec()];
    let quic_crypto = quinn::crypto::rustls::QuicClientConfig::try_from(rustls_config)?;
    let client_config = quinn::ClientConfig::new(Arc::new(quic_crypto));
    let mut endpoint = quinn::Endpoint::client(SocketAddr::from(([0, 0, 0, 0], 0)))?;
    endpoint.set_default_client_config(client_config);
    Ok(endpoint)
}

#[derive(Debug)]
struct TrustAnyServer;

impl ServerCertVerifier for TrustAnyServer {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ED25519,
            SignatureScheme::RSA_PSS_SHA256,
        ]
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use lsi_core::identity::Keypair;

    use super::NativeTransferClient;

    #[tokio::test]
    async fn send_files_to_url_rejects_non_quic_urls() {
        let err = NativeTransferClient::send_files_to_url(
            "https://127.0.0.1:53317",
            Vec::new(),
            Keypair::generate(),
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("native --url must start with quic://"));
    }

    #[tokio::test]
    async fn send_files_to_url_rejects_missing_files_before_connecting() {
        let err = NativeTransferClient::send_files_to_url(
            "quic://127.0.0.1:53317",
            vec![PathBuf::from("missing-native-payload.bin")],
            Keypair::generate(),
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("native send file does not exist"));
    }
}
