//! CLI entry point.

use anyhow::Result;
use clap::{Parser, Subcommand};

mod cmd;

#[derive(Parser)]
#[command(
    name = "localsend-improved",
    version,
    about = "CLI for LocalSend Improved",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Manage this node's identity.
    #[command(subcommand)]
    Identity(cmd::identity::Cmd),
    /// Manage trusted peers.
    #[command(subcommand)]
    Peers(cmd::peers::Cmd),
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Identity(command) => cmd::identity::run(command),
        Command::Peers(command) => cmd::peers::run(command),
    }
}
