//! `localsend-improved transfers ...` subcommands.

use anyhow::{bail, Result};
use clap::Subcommand;

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

pub fn run(command: Cmd) -> Result<()> {
    match command {
        Cmd::ListActive => list_active(),
        Cmd::Resume { transfer_id } => resume(&transfer_id),
    }
}

fn list_active() -> Result<()> {
    println!("no active transfers");
    Ok(())
}

fn resume(transfer_id: &str) -> Result<()> {
    if transfer_id.trim().is_empty() {
        bail!("transfer id is required");
    }

    bail!("native transfer resume is not wired to daemon yet")
}
