//! `localsend-improved send ...` command.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use clap::Args;
use lsi_core::identity::{Fingerprint, FsVault, IdentityVault, Keypair};
use lsi_core::paths;
use lsi_protocol_localsend_v2::client::LocalSendClient;
use lsi_protocol_localsend_v2::dto::{DeviceInfo, Protocol};
use lsi_protocol_native_v1::chunk::{blake3_hex, plan_chunks};
use lsi_protocol_native_v1::dto::{default_extensions, DoneTransfer, Hello, PROTOCOL_VERSION};
use lsi_protocol_native_v1::framing::{read_control, write_control, ControlMessage};
use lsi_protocol_native_v1::tls::ensure_ring_crypto_provider;
use lsi_protocol_native_v1::transfer::{write_chunk_frame, NativeTransferSender, TransferChunk};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

/// Send files to a LocalSend peer.
#[derive(Args)]
pub struct Cmd {
    /// Explicit peer API URL, for example https://192.168.1.20:53317.
    #[arg(long, required_unless_present = "native")]
    url: Option<String>,
    /// Use the native QUIC protocol instead of LocalSend v2.
    #[arg(long)]
    native: bool,
    /// One or more files to send.
    #[arg(required = true)]
    paths: Vec<PathBuf>,
}

pub fn run(command: Cmd) -> Result<()> {
    if command.native {
        return run_native(command);
    }

    let Some(url) = command.url else {
        bail!("send requires --url");
    };

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("creating async runtime")?;
    let sender = sender_info()?;
    let file_count = command.paths.len();

    runtime.block_on(async {
        LocalSendClient::new()
            .context("creating LocalSend client")?
            .send_files_to_url(&url, command.paths, sender)
            .await
            .context("sending files")
    })?;

    println!("sent {file_count} file(s)");
    Ok(())
}

fn run_native(command: Cmd) -> Result<()> {
    let Some(url) = command.url else {
        bail!("native send requires --url until peer selection is available");
    };

    let target = native_socket_addr(&url)?;
    for path in &command.paths {
        if !path.is_file() {
            bail!("native send file does not exist: {}", path.display());
        }
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("creating async runtime")?;
    let keypair = load_or_create_keypair()?;
    let file_count = command.paths.len();
    runtime
        .block_on(send_native_files(target, command.paths, keypair))
        .context("sending native files")?;

    println!("sent {file_count} file(s)");
    Ok(())
}

fn native_socket_addr(url: &str) -> Result<SocketAddr> {
    let Some(address) = url.strip_prefix("quic://") else {
        bail!("native --url must start with quic://");
    };
    address.parse::<SocketAddr>().context("native --url must be a quic://host:port socket address")
}

fn sender_info() -> Result<DeviceInfo> {
    let keypair = load_or_create_keypair()?;
    let fingerprint = Fingerprint::from_pubkey(&keypair.public_bytes()).to_string();

    Ok(DeviceInfo {
        alias: "localsend-improved".to_string(),
        version: "2.0".to_string(),
        device_model: None,
        device_type: Some("desktop".to_string()),
        fingerprint,
        port: 0,
        protocol: Protocol::from("https"),
        download: false,
    })
}

fn load_or_create_keypair() -> Result<Keypair> {
    let vault = FsVault::new(paths::identity_file());
    match vault.load()? {
        Some(keypair) => Ok(keypair),
        None => {
            let fresh = Keypair::generate();
            vault.save(&fresh).context("saving fresh identity")?;
            Ok(fresh)
        }
    }
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
    connection.close(0_u32.into(), b"done");
    endpoint.wait_idle().await;
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
    rustls_config.alpn_protocols = vec![lsi_protocol_native_v1::transport::NATIVE_ALPN.to_vec()];
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
