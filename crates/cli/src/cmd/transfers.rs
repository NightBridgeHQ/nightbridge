//! `night-bridge transfers ...` subcommands.

use crate::daemon_client::{self, DaemonClientConfig};
use anyhow::{bail, Context, Result};
use clap::Subcommand;
use lsi_proto::transfers::v1::{ActiveTransfer, TransferDirection, TransferState};

#[derive(Subcommand)]
pub enum Cmd {
    /// List active native transfers.
    ListActive,
    /// Resume an interrupted native transfer.
    Resume {
        /// Transfer id to resume.
        transfer_id: String,
    },
}

pub fn run(command: Cmd, daemon_config: &DaemonClientConfig) -> Result<()> {
    match command {
        Cmd::ListActive => list_active(daemon_config),
        Cmd::Resume { transfer_id } => resume(daemon_config, &transfer_id),
    }
}

fn list_active(config: &DaemonClientConfig) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("creating async runtime")?;
    let transfers = runtime.block_on(daemon_client::list_active_transfers(config))?;
    if transfers.is_empty() {
        println!("no active transfers");
        return Ok(());
    }

    println!("{:<36} {:<12} {:<12} {:>13} PEER", "TRANSFER ID", "DIRECTION", "STATE", "BYTES");
    for transfer in transfers {
        print_transfer(transfer);
    }
    Ok(())
}

fn resume(config: &DaemonClientConfig, transfer_id: &str) -> Result<()> {
    if transfer_id.trim().is_empty() {
        bail!("transfer id is required");
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("creating async runtime")?;
    let response = runtime.block_on(daemon_client::resume_transfer(config, transfer_id.into()))?;
    let state = transfer_state(response.state);
    println!("transfer {}: {}", response.transfer_id, state);
    Ok(())
}

fn print_transfer(transfer: ActiveTransfer) {
    println!(
        "{:<36} {:<12} {:<12} {:>6}/{:<6} {}",
        transfer.transfer_id,
        transfer_direction(transfer.direction),
        transfer_state(transfer.state),
        transfer.bytes_done,
        transfer.bytes_total,
        transfer.peer_fingerprint
    );
}

fn transfer_direction(direction: i32) -> &'static str {
    match TransferDirection::try_from(direction).unwrap_or(TransferDirection::Unspecified) {
        TransferDirection::Send => "send",
        TransferDirection::Receive => "receive",
        TransferDirection::Unspecified => "unspecified",
    }
}

fn transfer_state(state: i32) -> &'static str {
    match TransferState::try_from(state).unwrap_or(TransferState::Unspecified) {
        TransferState::Pending => "pending",
        TransferState::Active => "active",
        TransferState::Interrupted => "interrupted",
        TransferState::Completed => "completed",
        TransferState::Cancelled => "cancelled",
        TransferState::Failed => "failed",
        TransferState::Unspecified => "unspecified",
    }
}
