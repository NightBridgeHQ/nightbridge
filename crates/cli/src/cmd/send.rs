//! `localsend-improved send ...` command.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;
use lsi_core::identity::{Fingerprint, FsVault, IdentityVault, Keypair};
use lsi_core::paths;
use lsi_protocol_localsend_v2::client::LocalSendClient;
use lsi_protocol_localsend_v2::dto::{DeviceInfo, Protocol};

/// Send files to a LocalSend peer.
#[derive(Args)]
pub struct Cmd {
    /// Explicit peer API URL, for example https://192.168.1.20:53317.
    #[arg(long)]
    url: String,
    /// One or more files to send.
    #[arg(required = true)]
    paths: Vec<PathBuf>,
}

pub fn run(command: Cmd) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("creating async runtime")?;
    let sender = sender_info()?;
    let file_count = command.paths.len();

    runtime.block_on(async {
        LocalSendClient::new()
            .context("creating LocalSend client")?
            .send_files_to_url(&command.url, command.paths, sender)
            .await
            .context("sending files")
    })?;

    println!("sent {file_count} file(s)");
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
