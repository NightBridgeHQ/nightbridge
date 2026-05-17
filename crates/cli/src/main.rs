// CLI entry point.

use anyhow::Result;
use clap::{Parser, Subcommand};

mod cmd;
mod daemon_client;

#[derive(Parser)]
#[command(
    name = "night-bridge",
    version,
    about = "CLI for NightBridge",
    long_about = None
)]
struct Cli {
    /// Daemon gRPC endpoint.
    #[arg(long, global = true, default_value = "http://127.0.0.1:53500")]
    daemon_grpc: String,
    /// Local daemon API bearer token. Defaults to the token stored by the daemon.
    #[arg(long, global = true)]
    api_token: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Inspect the local daemon.
    #[command(subcommand)]
    Daemon(DaemonCommand),
    /// Manage this node's identity.
    #[command(subcommand)]
    Identity(cmd::identity::Cmd),
    /// Manage trusted peers.
    #[command(subcommand)]
    Peers(cmd::peers::Cmd),
    /// Send files to a LocalSend peer.
    Send(cmd::send::Cmd),
    /// Inspect and resume native transfers.
    #[command(subcommand)]
    Transfers(cmd::transfers::Cmd),
}

#[derive(Subcommand)]
enum DaemonCommand {
    /// Show daemon status from the local gRPC API.
    Status,
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
    let daemon_config =
        daemon_client::DaemonClientConfig { endpoint: cli.daemon_grpc, api_token: cli.api_token };

    match cli.command {
        Command::Daemon(DaemonCommand::Status) => print_daemon_status(&daemon_config),
        Command::Identity(command) => cmd::identity::run(command),
        Command::Peers(command) => cmd::peers::run(command, &daemon_config),
        Command::Send(command) => cmd::send::run(command, &daemon_config),
        Command::Transfers(command) => cmd::transfers::run(command, &daemon_config),
    }
}

fn print_daemon_status(config: &daemon_client::DaemonClientConfig) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let status = runtime.block_on(daemon_client::get_status(config))?;

    println!("alias: {}", status.alias);
    println!("fingerprint: {}", status.fingerprint);
    println!("version: {}", status.version);
    println!("inbox: {}", status.inbox_dir);
    println!("localsend port: {}", status.localsend_port);
    println!("native port: {}", status.native_port);

    Ok(())
}
