//! `localsend-improved send ...` command.

use std::path::PathBuf;

use crate::daemon_client::{self, DaemonClientConfig};
use anyhow::{bail, Context, Result};
use clap::Args;
use lsi_core::identity::{Fingerprint, FsVault, IdentityVault, Keypair};
use lsi_core::paths;
use lsi_proto::transfers::v1::{send_request, SendRequest};
use lsi_protocol_localsend_v2::client::LocalSendClient;
use lsi_protocol_localsend_v2::dto::{DeviceInfo, Protocol};
use lsi_protocol_native_v1::client::NativeTransferClient;

/// Send files to a LocalSend peer.
#[derive(Args)]
pub struct Cmd {
    /// Explicit peer API URL, for example https://192.168.1.20:53317.
    #[arg(long, required_unless_present = "native")]
    url: Option<String>,
    /// Use the native QUIC protocol instead of LocalSend v2.
    #[arg(long)]
    native: bool,
    /// Send directly from this CLI process instead of through the daemon API.
    #[arg(long)]
    direct: bool,
    /// One or more files to send.
    #[arg(required = true)]
    paths: Vec<PathBuf>,
}

pub fn run(command: Cmd, daemon_config: &DaemonClientConfig) -> Result<()> {
    if command.direct {
        return run_direct(command);
    }

    run_daemon(command, daemon_config)
}

fn run_direct(command: Cmd) -> Result<()> {
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

fn run_daemon(command: Cmd, config: &DaemonClientConfig) -> Result<()> {
    let Some(url) = command.url else {
        if command.native {
            bail!("native send requires --url until peer selection is available");
        }
        bail!("send requires --url");
    };

    let paths = command.paths.iter().map(|path| path.display().to_string()).collect();
    let target = if command.native {
        send_request::Target::NativeUrl(url)
    } else {
        send_request::Target::LocalsendUrl(url)
    };
    let request = SendRequest { paths, target: Some(target), native: command.native };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("creating async runtime")?;
    let response = runtime.block_on(daemon_client::send_files(config, request))?;

    println!("transfer: {}", response.transfer_id);
    Ok(())
}

fn run_native(command: Cmd) -> Result<()> {
    let Some(url) = command.url else {
        bail!("native send requires --url until peer selection is available");
    };

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("creating async runtime")?;
    let keypair = load_or_create_keypair()?;
    let file_count = command.paths.len();
    runtime.block_on(NativeTransferClient::send_files_to_url(&url, command.paths, keypair))?;

    println!("sent {file_count} file(s)");
    Ok(())
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
