//! Ratatui rendering for the daemon dashboard.

use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs},
    Frame,
};

use crate::app::AppState;

/// Render the daemon dashboard into a terminal frame.
pub fn render(frame: &mut Frame, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Length(7), Constraint::Min(10)])
        .split(frame.size());

    let titles: Vec<_> = crate::app::Tab::ALL.iter().map(|tab| tab.title()).collect();
    let selected =
        crate::app::Tab::ALL.iter().position(|tab| *tab == state.selected_tab).unwrap_or(0);
    frame.render_widget(
        Tabs::new(titles)
            .select(selected)
            .block(Block::default().borders(Borders::ALL).title("NightBridge"))
            .style(Style::default())
            .highlight_style(Style::default().add_modifier(Modifier::BOLD)),
        chunks[0],
    );

    let status = vec![
        Line::from(format!("alias: {}", state.alias.as_deref().unwrap_or("-"))),
        Line::from(format!("fingerprint: {}", state.fingerprint.as_deref().unwrap_or("-"))),
        Line::from(format!("version: {}", state.version.as_deref().unwrap_or("-"))),
        Line::from(format!("inbox: {}", state.inbox_dir.as_deref().unwrap_or("-"))),
        Line::from(format!(
            "ports: localsend={} native={}",
            state.localsend_port.map(|port| port.to_string()).unwrap_or_else(|| "-".into()),
            state.native_port.map(|port| port.to_string()).unwrap_or_else(|| "-".into())
        )),
    ];
    frame.render_widget(
        Paragraph::new(status).block(Block::default().borders(Borders::ALL).title("Daemon")),
        chunks[1],
    );

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(chunks[2]);

    frame.render_widget(peer_list(state), columns[0]);
    frame.render_widget(transfer_list(state), columns[1]);
    frame.render_widget(inbox_list(state), columns[2]);
}

fn peer_list(state: &AppState) -> List {
    let items = if state.trusted_peers.is_empty() {
        vec![ListItem::new("no trusted peers")]
    } else {
        state
            .trusted_peers
            .iter()
            .map(|peer| {
                ListItem::new(format!("{} {} {}", peer.label, peer.policy, peer.fingerprint))
            })
            .collect()
    };
    List::new(items).block(Block::default().borders(Borders::ALL).title("Peers"))
}

fn transfer_list(state: &AppState) -> List {
    let items = if state.active_transfers.is_empty() {
        vec![ListItem::new("no active transfers")]
    } else {
        state
            .active_transfers
            .iter()
            .map(|transfer| {
                ListItem::new(format!(
                    "{} {} {} {}/{}",
                    transfer.transfer_id,
                    transfer.direction,
                    transfer.state,
                    transfer.bytes_done,
                    transfer.bytes_total
                ))
            })
            .collect()
    };
    List::new(items).block(Block::default().borders(Borders::ALL).title("Transfers"))
}

fn inbox_list(state: &AppState) -> List {
    let items = if state.inbox_entries.is_empty() {
        vec![ListItem::new("inbox empty")]
    } else {
        state
            .inbox_entries
            .iter()
            .map(|entry| ListItem::new(format!("{} {} bytes", entry.file_name, entry.size)))
            .collect()
    };
    List::new(items).block(Block::default().borders(Borders::ALL).title("Inbox"))
}

#[cfg(test)]
mod tests {
    use ratatui::{backend::TestBackend, Terminal};

    use super::*;
    use crate::app::{AppState, InboxEntry, Tab, Transfer, TrustedPeer};

    #[test]
    fn dashboard_renders_daemon_state() {
        let state = AppState {
            alias: Some("workstation".to_string()),
            fingerprint: Some("abcd-1234".to_string()),
            selected_tab: Tab::Dashboard,
            trusted_peers: vec![TrustedPeer {
                fingerprint: "peer-1".to_string(),
                label: "phone".to_string(),
                policy: "auto_accept".to_string(),
                last_seen_unix_seconds: Some(42),
            }],
            active_transfers: vec![Transfer {
                transfer_id: "transfer-1".to_string(),
                peer_fingerprint: "peer-1".to_string(),
                direction: "send".to_string(),
                state: "active".to_string(),
                bytes_done: 4,
                bytes_total: 8,
            }],
            inbox_entries: vec![InboxEntry {
                file_name: "note.txt".to_string(),
                path: "/tmp/note.txt".to_string(),
                size: 12,
                modified_unix_seconds: 99,
            }],
            ..AppState::default()
        };

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &state)).unwrap();

        let buffer = terminal.backend().buffer();
        let rendered = format!("{buffer:?}");
        assert!(rendered.contains("workstation"), "{rendered}");
        assert!(rendered.contains("phone"), "{rendered}");
        assert!(rendered.contains("transfer-1"), "{rendered}");
        assert!(rendered.contains("note.txt"), "{rendered}");
    }
}
