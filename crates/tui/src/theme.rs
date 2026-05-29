//! NightBridge brand theme for the TUI (gothic-tech palette).
//!
//! Colors mirror `docs/brand/palette.md`: teal as the brand/structure anchor,
//! blood-red as the signature accent, moonlight foreground, gold for "pending"
//! status only. Values are truecolor; ratatui degrades gracefully on 16/256
//! color terminals.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, BorderType, Borders};

/// Brand teal — primary structure and focus.
pub const TEAL: Color = Color::Rgb(0x5e, 0xc5, 0xb5);
/// Dim teal for unfocused borders.
pub const BORDER: Color = Color::Rgb(0x33, 0x4b, 0x45);
/// Moonlight foreground for values and headlines.
pub const MOON: Color = Color::Rgb(0xe7, 0xef, 0xe9);
/// Muted secondary text.
pub const MUTED: Color = Color::Rgb(0x8f, 0xa6, 0x9c);
/// Blood-red signature accent (deny / failed / errors).
pub const RED: Color = Color::Rgb(0xe0, 0x56, 0x4f);
/// Gold — "pending" status only.
pub const GOLD: Color = Color::Rgb(0xf4, 0xc7, 0x66);
/// Blue — informational / completed.
pub const BLUE: Color = Color::Rgb(0x86, 0xbf, 0xf4);
/// Green — success / active.
pub const GREEN: Color = Color::Rgb(0x6f, 0xe0, 0xa8);

/// A rounded, brand-styled panel. Focused panels get a teal border + title.
pub fn panel(title: &str, focused: bool) -> Block<'_> {
    let edge = if focused { TEAL } else { BORDER };
    let title_color = if focused { TEAL } else { MUTED };
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(edge))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(title_color).add_modifier(Modifier::BOLD),
        ))
}

/// A key style (teal, bold) for help hints and labels.
pub fn key(text: &str) -> Span<'_> {
    Span::styled(text, Style::default().fg(TEAL).add_modifier(Modifier::BOLD))
}

/// Humanize a byte count (B/KB/MB/GB/TB, 1 decimal above KB).
pub fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if n < 1024 {
        return format!("{n} B");
    }
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// Render a fixed-width unicode progress bar like `███████░░░`.
pub fn progress_bar(done: u64, total: u64, width: usize) -> String {
    let filled = if total == 0 {
        0
    } else {
        ((done as u128 * width as u128) / total as u128).min(width as u128) as usize
    };
    let mut bar = String::with_capacity(width * 3);
    bar.push_str(&"█".repeat(filled));
    bar.push_str(&"░".repeat(width.saturating_sub(filled)));
    bar
}

/// Percentage (0–100) of done/total, saturating.
pub fn percent(done: u64, total: u64) -> u64 {
    if total == 0 {
        0
    } else {
        ((done as u128 * 100) / total as u128).min(100) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_bytes_scales() {
        assert_eq!(human_bytes(4), "4 B");
        assert_eq!(human_bytes(2048), "2.0 KB");
        assert_eq!(human_bytes(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn progress_bar_fills_proportionally() {
        assert_eq!(progress_bar(0, 10, 10), "░░░░░░░░░░");
        assert_eq!(progress_bar(5, 10, 10), "█████░░░░░");
        assert_eq!(progress_bar(10, 10, 10), "██████████");
        assert_eq!(progress_bar(1, 0, 4), "░░░░"); // no divide-by-zero
    }
}
