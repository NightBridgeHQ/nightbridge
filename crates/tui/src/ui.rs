//! Ratatui rendering for the daemon dashboard.

use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs, Wrap},
    Frame,
};

use crate::app::AppState;

/// Render the daemon dashboard into a terminal frame.
pub fn render(frame: &mut Frame, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(7),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(frame.size());

    let titles: Vec<_> = crate::app::Tab::ALL.iter().map(|tab| tab.title()).collect();
    let selected =
        crate::app::Tab::ALL.iter().position(|tab| *tab == state.selected_tab).unwrap_or(0);
    frame.render_widget(
        Tabs::new(titles)
            .select(selected)
            .block(Block::default().borders(Borders::ALL).title("NightBridge"))
            .style(Style::default().fg(Color::Gray))
            .highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        chunks[0],
    );

    let status = vec![
        labeled_line("alias", state.alias.as_deref().unwrap_or("-"), Color::Green),
        labeled_line("fingerprint", state.fingerprint.as_deref().unwrap_or("-"), Color::Yellow),
        labeled_line("version", state.version.as_deref().unwrap_or("-"), Color::Blue),
        labeled_line("inbox", state.inbox_dir.as_deref().unwrap_or("-"), Color::Magenta),
        if state.advanced {
            Line::from(format!(
                "ports: LocalSend={} native={}",
                state.localsend_port.map(|port| port.to_string()).unwrap_or_else(|| "-".into()),
                state.native_port.map(|port| port.to_string()).unwrap_or_else(|| "-".into())
            ))
        } else {
            Line::from(format!(
                "LocalSend port: {}",
                state.localsend_port.map(|port| port.to_string()).unwrap_or_else(|| "-".into())
            ))
        },
    ];
    frame.render_widget(
        Paragraph::new(status)
            .block(Block::default().borders(Borders::ALL).title("Daemon"))
            .wrap(Wrap { trim: true }),
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

    frame.render_widget(localsend_peer_list(state), columns[0]);
    frame.render_widget(transfer_list(state), columns[1]);
    frame.render_widget(inbox_list(state), columns[2]);

    let help = vec![Line::from(vec![
        Span::styled(
            "Tab/Shift+Tab",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" tabs  "),
        Span::styled("j/k", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(" move  "),
        Span::styled("a", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        Span::raw(" approve  "),
        Span::styled("d", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
        Span::raw(" deny  "),
        Span::styled("q", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw(" quit"),
    ])];
    frame.render_widget(
        Paragraph::new(help)
            .block(Block::default().borders(Borders::ALL).title("Help"))
            .wrap(Wrap { trim: true }),
        chunks[3],
    );
}

fn labeled_line<'a>(label: &'a str, value: &'a str, color: Color) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{label}: "), Style::default().fg(color).add_modifier(Modifier::BOLD)),
        Span::raw(value),
    ])
}

fn localsend_peer_list(state: &AppState) -> List<'_> {
    let items = if state.pending_localsend_peers.is_empty() {
        vec![ListItem::new("no pending LocalSend peers")]
    } else {
        state
            .pending_localsend_peers
            .iter()
            .enumerate()
            .map(|(index, peer)| {
                let source = peer.source_ip.as_deref().unwrap_or("-");
                let marker = if index == state.selected_localsend_peer { ">" } else { " " };
                ListItem::new(Line::from(vec![
                    Span::styled(marker, Style::default().fg(Color::Cyan)),
                    Span::raw(" "),
                    Span::styled(&peer.alias, Style::default().fg(Color::White)),
                    Span::raw(" "),
                    Span::styled(&peer.status, Style::default().fg(Color::Yellow)),
                    Span::raw(format!(
                        " attempts={} source={} {}",
                        peer.attempt_count, source, peer.fingerprint
                    )),
                ]))
            })
            .collect()
    };
    List::new(items)
        .block(Block::default().borders(Borders::ALL).title("LocalSend approvals"))
        .highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
}

fn transfer_list(state: &AppState) -> List<'_> {
    let items = if state.active_transfers.is_empty() {
        vec![ListItem::new("no active transfers")]
    } else {
        state
            .active_transfers
            .iter()
            .map(|transfer| {
                let color = match transfer.state.as_str() {
                    "active" => Color::Green,
                    "failed" | "interrupted" => Color::Red,
                    "completed" => Color::Blue,
                    _ => Color::Gray,
                };
                ListItem::new(Line::from(vec![
                    Span::styled(&transfer.transfer_id, Style::default().fg(Color::White)),
                    Span::raw(format!(" {} ", transfer.direction)),
                    Span::styled(&transfer.state, Style::default().fg(color)),
                    Span::raw(format!(" {}/{}", transfer.bytes_done, transfer.bytes_total)),
                ]))
            })
            .collect()
    };
    List::new(items).block(Block::default().borders(Borders::ALL).title("Transfers"))
}

fn inbox_list(state: &AppState) -> List<'_> {
    let items = if state.inbox_entries.is_empty() {
        vec![ListItem::new("inbox empty")]
    } else {
        state
            .inbox_entries
            .iter()
            .map(|entry| {
                ListItem::new(Line::from(vec![
                    Span::styled(&entry.file_name, Style::default().fg(Color::White)),
                    Span::raw(format!(" {} bytes", entry.size)),
                ]))
            })
            .collect()
    };
    List::new(items).block(Block::default().borders(Borders::ALL).title("Inbox"))
}

#[cfg(test)]
mod tests {
    use ratatui::{backend::TestBackend, Terminal};

    use super::*;
    use crate::app::{AppState, InboxEntry, LocalSendPeer, Tab, Transfer};

    #[test]
    fn dashboard_renders_daemon_state() {
        let state = AppState {
            alias: Some("workstation".to_string()),
            fingerprint: Some("abcd-1234".to_string()),
            selected_tab: Tab::Dashboard,
            pending_localsend_peers: vec![LocalSendPeer {
                fingerprint: "peer-1".to_string(),
                alias: "phone".to_string(),
                label: None,
                status: "pending".to_string(),
                attempt_count: 1,
                source_ip: None,
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
        assert!(rendered.contains("Tab/Shift+Tab"), "{rendered}");
        assert!(rendered.contains("approve"), "{rendered}");
    }

    #[test]
    fn default_dashboard_hides_native_details_and_shows_localsend_pending_peers() {
        let state = AppState {
            alias: Some("receiver".to_string()),
            localsend_port: Some(53317),
            native_port: Some(53400),
            pending_localsend_peers: vec![LocalSendPeer {
                fingerprint: "ios-fingerprint".to_string(),
                alias: "Diego iPhone".to_string(),
                label: None,
                status: "pending".to_string(),
                attempt_count: 2,
                source_ip: Some("10.16.20.53".to_string()),
            }],
            ..AppState::default()
        };

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &state)).unwrap();

        let rendered = format!("{:?}", terminal.backend().buffer());
        assert!(rendered.contains("LocalSend"), "{rendered}");
        assert!(rendered.contains("Diego iPhone"), "{rendered}");
        assert!(rendered.contains("pending"), "{rendered}");
        assert!(!rendered.contains("native"), "{rendered}");
        assert!(!rendered.contains("QUIC"), "{rendered}");
        assert!(!rendered.contains("WAN"), "{rendered}");
    }

    #[test]
    fn advanced_dashboard_can_show_native_details() {
        let state = AppState { native_port: Some(53400), advanced: true, ..AppState::default() };

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &state)).unwrap();

        let rendered = format!("{:?}", terminal.backend().buffer());
        assert!(rendered.contains("native"), "{rendered}");
        assert!(rendered.contains("53400"), "{rendered}");
    }
}
