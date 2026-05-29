//! Ratatui rendering for the daemon dashboard.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{block::Title, Block, BorderType, Borders, List, ListItem, Paragraph, Tabs, Wrap},
    Frame,
};

use crate::app::{AppState, Tab};
use crate::theme;

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

    render_header(frame, state, chunks[0]);
    render_status(frame, state, chunks[1]);

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

    render_help(frame, state, chunks[3]);
}

fn render_header(frame: &mut Frame, state: &AppState, area: ratatui::layout::Rect) {
    let titles: Vec<_> = Tab::ALL.iter().map(|tab| tab.title()).collect();
    let selected = Tab::ALL.iter().position(|tab| *tab == state.selected_tab).unwrap_or(0);

    let online = state.last_error.is_none();
    let status = if online {
        Span::styled("● online", Style::default().fg(theme::TEAL))
    } else {
        Span::styled("● issue", Style::default().fg(theme::RED))
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(
            Title::from(Line::from(vec![
                Span::styled("▌", Style::default().fg(theme::RED)),
                Span::styled(
                    " NightBridge",
                    Style::default().fg(theme::TEAL).add_modifier(Modifier::BOLD),
                ),
            ]))
            .alignment(Alignment::Left),
        )
        .title(Title::from(status).alignment(Alignment::Right));

    frame.render_widget(
        Tabs::new(titles)
            .select(selected)
            .block(block)
            .style(Style::default().fg(theme::MUTED))
            .highlight_style(Style::default().fg(theme::TEAL).add_modifier(Modifier::BOLD))
            .divider(Span::styled("·", Style::default().fg(theme::BORDER))),
        area,
    );
}

fn render_status(frame: &mut Frame, state: &AppState, area: ratatui::layout::Rect) {
    let mut lines = vec![
        kv("alias", state.alias.as_deref().unwrap_or("—")),
        kv("fingerprint", state.fingerprint.as_deref().unwrap_or("—")),
        kv("version", state.version.as_deref().unwrap_or("—")),
        kv("inbox", state.inbox_dir.as_deref().unwrap_or("—")),
    ];
    let ls = state.localsend_port.map(|p| p.to_string()).unwrap_or_else(|| "—".into());
    if state.advanced {
        let nv = state.native_port.map(|p| p.to_string()).unwrap_or_else(|| "—".into());
        lines.push(Line::from(format!("ports: LocalSend={ls} native={nv}")));
    } else {
        lines.push(kv("LocalSend port", &ls));
    }

    frame.render_widget(
        Paragraph::new(lines)
            .block(theme::panel("Daemon", state.selected_tab == Tab::Dashboard))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_help(frame: &mut Frame, state: &AppState, area: ratatui::layout::Rect) {
    let sep = || Span::styled("  ·  ", Style::default().fg(theme::BORDER));
    let line = if let Some(error) = &state.last_error {
        Line::from(vec![
            Span::styled("⚠ ", Style::default().fg(theme::RED)),
            Span::styled(error.clone(), Style::default().fg(theme::RED)),
        ])
    } else {
        Line::from(vec![
            theme::key("Tab/Shift+Tab"),
            Span::styled(" tabs", Style::default().fg(theme::MUTED)),
            sep(),
            theme::key("j/k"),
            Span::styled(" move", Style::default().fg(theme::MUTED)),
            sep(),
            Span::styled("a", Style::default().fg(theme::GREEN).add_modifier(Modifier::BOLD)),
            Span::styled(" approve", Style::default().fg(theme::MUTED)),
            sep(),
            Span::styled("d", Style::default().fg(theme::RED).add_modifier(Modifier::BOLD)),
            Span::styled(" deny", Style::default().fg(theme::MUTED)),
            sep(),
            theme::key("q"),
            Span::styled(" quit", Style::default().fg(theme::MUTED)),
        ])
    };
    frame.render_widget(
        Paragraph::new(line).block(theme::panel("Help", false)).wrap(Wrap { trim: true }),
        area,
    );
}

fn kv<'a>(label: &str, value: &'a str) -> Line<'a> {
    Line::from(vec![
        Span::styled(
            format!("{:<15}", format!("{label}:")),
            Style::default().fg(theme::TEAL).add_modifier(Modifier::BOLD),
        ),
        Span::styled(value.to_string(), Style::default().fg(theme::MOON)),
    ])
}

fn localsend_peer_list(state: &AppState) -> List<'_> {
    let focused = state.selected_tab == Tab::LocalSend;
    let items = if state.pending_localsend_peers.is_empty() {
        vec![ListItem::new(Span::styled(
            "— no pending LocalSend peers —",
            Style::default().fg(theme::MUTED),
        ))]
    } else {
        state
            .pending_localsend_peers
            .iter()
            .enumerate()
            .map(|(index, peer)| {
                let selected = index == state.selected_localsend_peer;
                let marker = if selected {
                    Span::styled("▌ ", Style::default().fg(theme::TEAL))
                } else {
                    Span::raw("  ")
                };
                let badge_color = match peer.status.as_str() {
                    "pending" => theme::GOLD,
                    "approved" => theme::GREEN,
                    "denied" | "blocked" => theme::RED,
                    _ => theme::MUTED,
                };
                let source = peer.source_ip.as_deref().unwrap_or("—");
                ListItem::new(Line::from(vec![
                    marker,
                    Span::styled(
                        &peer.alias,
                        Style::default().fg(theme::MOON).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        format!("[{}]", peer.status),
                        Style::default().fg(badge_color).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("  ×{}  {}", peer.attempt_count, source),
                        Style::default().fg(theme::MUTED),
                    ),
                ]))
            })
            .collect()
    };
    List::new(items)
        .block(theme::panel("LocalSend approvals", focused))
        .highlight_style(Style::default().fg(theme::TEAL).add_modifier(Modifier::BOLD))
}

fn transfer_list(state: &AppState) -> List<'_> {
    let focused = state.selected_tab == Tab::Transfers;
    let items = if state.active_transfers.is_empty() {
        vec![ListItem::new(Span::styled(
            "— no active transfers —",
            Style::default().fg(theme::MUTED),
        ))]
    } else {
        state
            .active_transfers
            .iter()
            .map(|transfer| {
                let color = match transfer.state.as_str() {
                    "active" => theme::GREEN,
                    "failed" | "interrupted" => theme::RED,
                    "completed" => theme::BLUE,
                    _ => theme::MUTED,
                };
                let pct = theme::percent(transfer.bytes_done, transfer.bytes_total);
                let bar = theme::progress_bar(transfer.bytes_done, transfer.bytes_total, 12);
                ListItem::new(vec![
                    Line::from(vec![
                        Span::styled(
                            &transfer.transfer_id,
                            Style::default().fg(theme::MOON).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("  {}", transfer.direction),
                            Style::default().fg(theme::MUTED),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled(bar, Style::default().fg(color)),
                        Span::styled(format!(" {pct:>3}%  "), Style::default().fg(color)),
                        Span::styled(
                            format!(
                                "{} / {}",
                                theme::human_bytes(transfer.bytes_done),
                                theme::human_bytes(transfer.bytes_total)
                            ),
                            Style::default().fg(theme::MUTED),
                        ),
                    ]),
                ])
            })
            .collect()
    };
    List::new(items).block(theme::panel("Transfers", focused))
}

fn inbox_list(state: &AppState) -> List<'_> {
    let focused = state.selected_tab == Tab::Inbox;
    let items = if state.inbox_entries.is_empty() {
        vec![ListItem::new(Span::styled("— inbox empty —", Style::default().fg(theme::MUTED)))]
    } else {
        state
            .inbox_entries
            .iter()
            .map(|entry| {
                ListItem::new(Line::from(vec![
                    Span::styled("› ", Style::default().fg(theme::TEAL)),
                    Span::styled(&entry.file_name, Style::default().fg(theme::MOON)),
                    Span::styled(
                        format!("  {}", theme::human_bytes(entry.size)),
                        Style::default().fg(theme::MUTED),
                    ),
                ]))
            })
            .collect()
    };
    List::new(items).block(theme::panel("Inbox", focused))
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
                alias: "iOS Test Device".to_string(),
                label: None,
                status: "pending".to_string(),
                attempt_count: 2,
                source_ip: Some("192.0.2.53".to_string()),
            }],
            ..AppState::default()
        };

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &state)).unwrap();

        let rendered = format!("{:?}", terminal.backend().buffer());
        assert!(rendered.contains("LocalSend"), "{rendered}");
        assert!(rendered.contains("iOS Test Device"), "{rendered}");
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
