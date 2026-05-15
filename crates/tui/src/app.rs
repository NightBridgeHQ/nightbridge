//! Application state for the daemon dashboard.

use crossterm::event::{KeyCode, KeyEvent};

/// Dashboard tab selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tab {
    /// High-level daemon status.
    Dashboard,
    /// Trusted peers.
    Peers,
    /// Active transfers.
    Transfers,
    /// Inbox entries.
    Inbox,
}

impl Tab {
    /// All tabs in display order.
    pub const ALL: [Self; 4] = [Self::Dashboard, Self::Peers, Self::Transfers, Self::Inbox];

    /// User-facing tab title.
    pub fn title(self) -> &'static str {
        match self {
            Self::Dashboard => "Dashboard",
            Self::Peers => "Peers",
            Self::Transfers => "Transfers",
            Self::Inbox => "Inbox",
        }
    }
}

/// TUI app state populated from the daemon API.
#[derive(Clone, Debug, Default)]
pub struct AppState {
    /// Selected dashboard tab.
    pub selected_tab: Tab,
    /// Daemon alias.
    pub alias: Option<String>,
    /// Daemon fingerprint.
    pub fingerprint: Option<String>,
    /// Daemon version.
    pub version: Option<String>,
    /// Daemon inbox path.
    pub inbox_dir: Option<String>,
    /// Daemon LocalSend port.
    pub localsend_port: Option<u32>,
    /// Daemon native port.
    pub native_port: Option<u32>,
    /// Trusted peers from the API.
    pub trusted_peers: Vec<TrustedPeer>,
    /// Active transfers from the API.
    pub active_transfers: Vec<Transfer>,
    /// Inbox entries from the API.
    pub inbox_entries: Vec<InboxEntry>,
    /// Last polling error, if any.
    pub last_error: Option<String>,
    /// Whether the terminal app should exit.
    pub should_quit: bool,
}

impl AppState {
    /// Update status fields from a daemon status response.
    pub fn set_status(&mut self, status: lsi_proto::daemon::v1::DaemonStatus) {
        self.alias = Some(status.alias);
        self.fingerprint = Some(status.fingerprint);
        self.version = Some(status.version);
        self.inbox_dir = Some(status.inbox_dir);
        self.localsend_port = Some(status.localsend_port);
        self.native_port = Some(status.native_port);
    }

    /// Apply a keyboard event to the app state.
    pub fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            _ => {}
        }
    }
}

impl Default for Tab {
    fn default() -> Self {
        Self::Dashboard
    }
}

/// Trusted peer row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrustedPeer {
    /// Peer fingerprint.
    pub fingerprint: String,
    /// Human label.
    pub label: String,
    /// Trust policy.
    pub policy: String,
    /// Last seen unix timestamp.
    pub last_seen_unix_seconds: Option<i64>,
}

/// Active transfer row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transfer {
    /// Transfer identifier.
    pub transfer_id: String,
    /// Peer fingerprint.
    pub peer_fingerprint: String,
    /// Direction label.
    pub direction: String,
    /// State label.
    pub state: String,
    /// Completed bytes.
    pub bytes_done: u64,
    /// Total bytes.
    pub bytes_total: u64,
}

/// Inbox entry row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InboxEntry {
    /// File name.
    pub file_name: String,
    /// Absolute path.
    pub path: String,
    /// Size in bytes.
    pub size: u64,
    /// Modified unix timestamp.
    pub modified_unix_seconds: i64,
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;

    #[test]
    fn q_and_escape_quit_the_app() {
        let mut state = AppState::default();

        state.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(state.should_quit);

        let mut state = AppState::default();
        state.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(state.should_quit);
    }
}
