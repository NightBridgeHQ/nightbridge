//! `night-bridge identity ...` subcommands.

use anyhow::{Context, Result};
use clap::Subcommand;
use lsi_core::identity::{Fingerprint, FsVault, IdentityVault, Keypair};
use lsi_core::paths;

#[derive(Subcommand)]
pub enum Cmd {
    /// Print this node's fingerprint and public key.
    Show,
    /// Generate a new keypair, replacing the existing one.
    Rotate {
        /// Skip the confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
}

pub fn run(command: Cmd) -> Result<()> {
    let vault = FsVault::new(paths::identity_file());
    match command {
        Cmd::Show => show(&vault),
        Cmd::Rotate { yes } => rotate(&vault, yes),
    }
}

fn show(vault: &FsVault) -> Result<()> {
    let keypair = match vault.load()? {
        Some(keypair) => keypair,
        None => {
            let fresh = Keypair::generate();
            vault.save(&fresh).context("saving fresh identity")?;
            fresh
        }
    };
    let fingerprint = Fingerprint::from_pubkey(&keypair.public_bytes());
    println!("fingerprint: {fingerprint}");
    println!("pubkey-hex:  {}", hex_encode(&keypair.public_bytes()));
    println!("path:        {}", vault.path().display());
    Ok(())
}

fn rotate(vault: &FsVault, yes: bool) -> Result<()> {
    if !yes {
        eprintln!(
            "Rotating identity will INVALIDATE all existing trust relationships.\n\
             Re-run with --yes to confirm."
        );
        return Ok(());
    }

    let keypair = Keypair::generate();
    vault.save(&keypair).context("saving new identity")?;
    let fingerprint = Fingerprint::from_pubkey(&keypair.public_bytes());
    println!("new fingerprint: {fingerprint}");
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut hex, byte| {
        write!(&mut hex, "{byte:02x}").expect("writing to a String cannot fail");
        hex
    })
}
