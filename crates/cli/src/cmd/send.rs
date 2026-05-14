//! `localsend-improved send ...` command.

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Args;
use lsi_core::identity::{Fingerprint, FsVault, IdentityVault, Keypair};
use lsi_core::paths;
use lsi_protocol_localsend_v2::client::LocalSendClient;
use lsi_protocol_localsend_v2::dto::{DeviceInfo, Protocol};

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

    validate_native_url(&url)?;
    for path in &command.paths {
        if !path.is_file() {
            bail!("native send file does not exist: {}", path.display());
        }
    }

    bail!("native send is not wired to daemon yet")
}

fn validate_native_url(url: &str) -> Result<()> {
    let Some(address) = url.strip_prefix("quic://") else {
        bail!("native --url must start with quic://");
    };
    if address.parse::<std::net::SocketAddr>().is_err() {
        bail!("native --url must be a quic://host:port socket address");
    }
    Ok(())
}

fn sender_info() -> Result<DeviceInfo> {
    let vault = FsVault::new(paths::identity_file());
    let keypair = match vault.load()? {
        Some(keypair) => keypair,
        None => {
            let fresh = Keypair::generate();
            vault.save(&fresh).context("saving fresh identity")?;
            fresh
        }
    };
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
