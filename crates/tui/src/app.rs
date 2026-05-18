//! Application state for the daemon dashboard.

use crossterm::event::{KeyCode, KeyEvent};

/// Dashboard tab selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tab {
    /// High-level daemon status.
    Dashboard,
    /// LocalSend receive approvals.
    LocalSend,
    /// Active transfers.
    Transfers,
    /// Inbox entries.
    Inbox,
}

impl Tab {
    /// All tabs in display order.
    pub const ALL: [Self; 4] = [Self::Dashboard, Self::LocalSend, Self::Transfers, Self::Inbox];

    /// User-facing tab title.
    pub fn title(self) -> &'static str {
        match self {
            Self::Dashboard => "Dashboard",
            Self::LocalSend => "LocalSend",
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
    /// Show native/QUIC/WAN details intended for advanced releases.
    pub advanced: bool,
    /// Trusted peers from the API.
    pub trusted_peers: Vec<TrustedPeer>,
    /// Official LocalSend peers waiting for approval.
    pub pending_localsend_peers: Vec<LocalSendPeer>,
    /// Selected pending LocalSend peer index.
    pub selected_localsend_peer: usize,
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
    pub fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('j') | KeyCode::Down => self.select_next_localsend_peer(),
            KeyCode::Char('k') | KeyCode::Up => self.select_previous_localsend_peer(),
            KeyCode::Char('a') => return self.approve_selected_localsend_peer(),
            KeyCode::Char('d') => return self.deny_selected_localsend_peer(),
            _ => {}
        }
        AppAction::None
    }

    fn select_next_localsend_peer(&mut self) {
        if self.pending_localsend_peers.is_empty() {
            self.selected_localsend_peer = 0;
            return;
        }
        self.selected_localsend_peer =
            (self.selected_localsend_peer + 1).min(self.pending_localsend_peers.len() - 1);
    }

    fn select_previous_localsend_peer(&mut self) {
        self.selected_localsend_peer = self.selected_localsend_peer.saturating_sub(1);
    }

    fn selected_localsend_peer(&self) -> Option<&LocalSendPeer> {
        self.pending_localsend_peers.get(self.selected_localsend_peer)
    }

    fn approve_selected_localsend_peer(&self) -> AppAction {
        self.selected_localsend_peer()
            .map(|peer| AppAction::ApproveLocalSend {
                fingerprint: peer.fingerprint.clone(),
                label: Some(peer.label.clone().unwrap_or_else(|| peer.alias.clone())),
            })
            .unwrap_or(AppAction::None)
    }

    fn deny_selected_localsend_peer(&self) -> AppAction {
        self.selected_localsend_peer()
            .map(|peer| AppAction::DenyLocalSend { fingerprint: peer.fingerprint.clone() })
            .unwrap_or(AppAction::None)
    }
}

/// TUI action that needs a daemon API call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppAction {
    /// No API action.
    None,
    /// Approve a pending official LocalSend peer.
    ApproveLocalSend {
        /// LocalSend fingerprint.
        fingerprint: String,
        /// Operator label.
        label: Option<String>,
    },
    /// Deny a pending official LocalSend peer.
    DenyLocalSend {
        /// LocalSend fingerprint.
        fingerprint: String,
    },
}

/// Official LocalSend peer row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalSendPeer {
    /// Normalized LocalSend fingerprint.
    pub fingerprint: String,
    /// Last advertised LocalSend alias.
    pub alias: String,
    /// Operator label.
    pub label: Option<String>,
    /// Approval status.
    pub status: String,
    /// Number of rejected attempts observed.
    pub attempt_count: u64,
    /// Last source IP, when observed.
    pub source_ip: Option<String>,
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

        assert_eq!(
            state.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
            AppAction::None
        );
        assert!(state.should_quit);

        let mut state = AppState::default();
        assert_eq!(
            state.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            AppAction::None
        );
        assert!(state.should_quit);
    }

    #[test]
    fn local_send_peer_keys_select_approve_and_deny() {
        let mut state = AppState {
            pending_localsend_peers: vec![
                LocalSendPeer {
                    fingerprint: "one".to_string(),
                    alias: "phone one".to_string(),
                    label: None,
                    status: "pending".to_string(),
                    attempt_count: 1,
                    source_ip: None,
                },
                LocalSendPeer {
                    fingerprint: "two".to_string(),
                    alias: "phone two".to_string(),
                    label: None,
                    status: "pending".to_string(),
                    attempt_count: 1,
                    source_ip: None,
                },
            ],
            ..AppState::default()
        };

        assert_eq!(state.selected_localsend_peer, 0);
        state.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(state.selected_localsend_peer, 1);
        assert_eq!(
            state.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
            AppAction::ApproveLocalSend {
                fingerprint: "two".to_string(),
                label: Some("phone two".to_string())
            }
        );
        assert_eq!(
            state.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE)),
            AppAction::DenyLocalSend { fingerprint: "two".to_string() }
        );
    }
}
