//! `night-bridge-tui` terminal dashboard entry point.

use std::io::{self, Stdout};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use lsi_core::{
    api_token::{ApiTokenVault, FsApiTokenVault},
    paths,
};
use lsi_tui::{
    app::AppState,
    client::{DaemonApiClient, DaemonApiConfig},
    ui,
};
use ratatui::{backend::CrosstermBackend, Terminal};

const DEFAULT_DAEMON_GRPC: &str = "http://127.0.0.1:53500";

#[tokio::main]
async fn main() -> Result<()> {
    let config = config_from_args(std::env::args().skip(1))?;
    let client = DaemonApiClient::new(config.clone());
    let mut state = match client.fetch_once().await {
        Ok(state) => state,
        Err(error) => AppState { last_error: Some(error.to_string()), ..AppState::default() },
    };

    let mut terminal = TerminalSession::enter()?;
    run_app(&mut terminal.terminal, client, config.poll_interval(), &mut state).await
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    client: DaemonApiClient,
    poll_interval: Duration,
    state: &mut AppState,
) -> Result<()> {
    let mut last_poll = Instant::now();
    loop {
        terminal.draw(|frame| ui::render(frame, state))?;

        if state.should_quit {
            break;
        }

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                state.handle_key(key);
            }
        }

        if last_poll.elapsed() >= poll_interval {
            match client.fetch_once().await {
                Ok(next) => *state = next,
                Err(error) => state.last_error = Some(error.to_string()),
            }
            last_poll = Instant::now();
        }
    }

    Ok(())
}

fn config_from_args<I>(args: I) -> Result<DaemonApiConfig>
where
    I: IntoIterator<Item = String>,
{
    let mut endpoint = DEFAULT_DAEMON_GRPC.to_string();
    let mut api_token = None;
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--daemon-grpc" => {
                endpoint = args.next().context("--daemon-grpc requires a value")?;
            }
            "--api-token" => {
                api_token = Some(args.next().context("--api-token requires a value")?);
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => bail!("unknown argument: {other}"),
        }
    }

    let api_token = match api_token {
        Some(token) => token,
        None => load_api_token()?,
    };

    Ok(DaemonApiConfig::new(endpoint, api_token))
}

fn load_api_token() -> Result<String> {
    let path = paths::api_token_file();
    let vault = FsApiTokenVault::new(&path);
    let Some(token) =
        vault.load().with_context(|| format!("loading api token from {}", path.display()))?
    else {
        bail!(
            "api token not found at {}; start the daemon once to create it or pass --api-token",
            path.display()
        );
    };
    Ok(token.expose_secret().to_string())
}

fn print_help() {
    println!(
        "Usage: night-bridge-tui [--daemon-grpc <URL>] [--api-token <TOKEN>]\n\n\
         Options:\n  \
         --daemon-grpc <URL>   Daemon gRPC endpoint [default: {DEFAULT_DAEMON_GRPC}]\n  \
         --api-token <TOKEN>   Local daemon API bearer token\n  \
         -h, --help            Show this help"
    );
}

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalSession {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("enabling terminal raw mode")?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen).context("entering alternate screen")?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend).context("creating terminal")?;
        Ok(Self { terminal })
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_parses_endpoint_and_token() {
        let config = config_from_args([
            "--daemon-grpc".to_string(),
            "http://127.0.0.1:1".to_string(),
            "--api-token".to_string(),
            "token".to_string(),
        ])
        .unwrap();

        assert_eq!(config.endpoint, "http://127.0.0.1:1");
        assert_eq!(config.api_token, "token");
    }
}
