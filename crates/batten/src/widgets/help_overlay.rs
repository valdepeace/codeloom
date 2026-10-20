//! Overlay de atajos de Pawan, tomado del snapshot del renderer Ratatui.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

/// Muestra la ayuda de teclado centrada como en Pawan.
pub fn render_pawan(frame: &mut Frame, screen: Rect) -> Rect {
    if screen.width < 52 || screen.height < 20 {
        return Rect::default();
    }
    let width = 48u16.min(screen.width.saturating_sub(4));
    let height = 16u16.min(screen.height.saturating_sub(4));
    let area = Rect::new(
        screen.x + (screen.width - width) / 2,
        screen.y + (screen.height - height) / 2,
        width,
        height,
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" Keyboard Shortcuts (F1) ");
    frame.render_widget(Clear, area);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let section = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let key = Style::default().fg(Color::Yellow);
    let shortcuts = vec![
        Line::raw(""),
        Line::from(Span::styled("  Navigation", section)),
        key_line("Tab     ", "Switch focus (input/messages)", key),
        key_line("j/k     ", "Scroll up/down", key),
        key_line("g/G     ", "Jump to top/bottom", key),
        key_line("/       ", "Search in messages", key),
        Line::raw(""),
        Line::from(Span::styled("  Commands", section)),
        key_line("Ctrl+P  ", "Fuzzy search (slash commands)", key),
        key_line("Ctrl+L  ", "Clear chat", key),
        key_line("Ctrl+Q  ", "Quit", key),
        Line::raw(""),
        Line::from(Span::styled("  Slash Commands", section)),
        key_line("/model  ", "Switch model at runtime", key),
    ];
    frame.render_widget(Paragraph::new(shortcuts), inner);
    area
}

/// Mindfork's six-tab help strip, including its active tab and narrow-width
/// ellipses. `active` is the tab index in About/Shortcuts/Commands/License/Legal/Components order.
pub fn render_mindfork_tabs(frame: &mut Frame, area: Rect, active: usize) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    const LABELS: [&str; 6] = [
        "About",
        "Shortcuts",
        "Commands",
        "License",
        "Legal",
        "Components",
    ];
    let active = active.min(LABELS.len() - 1);
    let widths: Vec<usize> = LABELS.iter().map(|s| s.len() + 2).collect();
    let (lo, hi) = help_tab_window(&widths, active, area.width as usize);
    let separator = Span::styled(" │", Style::default().fg(Color::Rgb(88, 94, 105)));
    let overflow = || Span::styled(" … ", Style::default().fg(Color::Rgb(139, 144, 153)));
    let mut spans = vec![Span::raw(" ")];
    if lo > 0 {
        spans.extend([overflow(), separator.clone()]);
    }
    for (index, label) in LABELS.iter().enumerate().take(hi + 1).skip(lo) {
        if index > lo {
            spans.push(separator.clone());
        }
        let label = format!(" {label} ");
        let style = if index == active {
            Style::default()
                .fg(Color::White)
                .bg(Color::Rgb(33, 36, 42))
                .bold()
        } else {
            Style::default().fg(Color::Rgb(139, 144, 153))
        };
        spans.push(Span::styled(label, style));
    }
    if hi + 1 < LABELS.len() {
        spans.extend([separator, overflow()]);
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn help_tab_window(widths: &[usize], active: usize, width: usize) -> (usize, usize) {
    let last = widths.len() - 1;
    let strip_width = |lo: usize, hi: usize| {
        1 + widths[lo..=hi].iter().sum::<usize>()
            + 2 * (hi - lo)
            + usize::from(lo > 0) * 5
            + usize::from(hi < last) * 5
    };
    let lo = (0..=active)
        .find(|&lo| strip_width(lo, active) <= width)
        .unwrap_or(active);
    let hi = (active..=last)
        .rev()
        .find(|&hi| strip_width(lo, hi) <= width)
        .unwrap_or(active);
    (lo, hi)
}

fn key_line(key_text: &str, description: &str, style: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {key_text}"), style),
        Span::raw(description.to_string()),
    ])
}
