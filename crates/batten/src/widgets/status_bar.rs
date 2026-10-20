//! Barra de estado de Mindfork, reconstruida desde `chat-dark-en.png`.

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

const BG: Color = Color::Rgb(23, 25, 30);
const GREEN: Color = Color::Rgb(111, 192, 130);
const KEY_FG: Color = Color::Rgb(133, 139, 147);
const LABEL_FG: Color = Color::Rgb(133, 139, 146);
const KEY_BG: Color = Color::Rgb(33, 36, 42);

/// Renderiza la barra de estado de Mindfork con chips de estado y teclas.
///
/// Los atajos del extremo derecho se descartan si el ancho no permite
/// mostrarlos; el indicador de chat y los atajos iniciales tienen prioridad.
pub fn render_mindfork(frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let green = Style::default().fg(GREEN);
    let state = vec![
        Span::styled("●", green.add_modifier(Modifier::BOLD)),
        Span::styled(" chat  ", green),
        Span::styled("●", green.add_modifier(Modifier::BOLD)),
        Span::styled(" emb", green),
    ];
    let state_width = "● chat  ● emb".width();
    let hints = [
        ("Ctrl+W", "mouse: select"),
        ("F1", "help"),
        ("Esc", "chats"),
        ("Ctrl+N", "new"),
        ("Ctrl+P", "settings"),
        ("Ctrl+Q", "quit"),
    ];
    let mut shown = hints.len();
    let group_width = |(key, label): &(&str, &str)| key.width() + 3 + label.width();
    let width = usize::from(area.width);
    let mut content_width = state_width + hints.iter().map(group_width).sum::<usize>();
    while shown > 0 && content_width + shown > width {
        shown -= 1;
        content_width -= group_width(&hints[shown]);
    }

    let gaps = shown + 1;
    let spare = width.saturating_sub(content_width);
    let base_gap = if gaps == 0 { 0 } else { spare / gaps };
    let extra = if gaps == 0 { 0 } else { spare % gaps };
    let mut spans = vec![Span::styled(
        " ".repeat(base_gap + usize::from(extra > 0)),
        Style::default(),
    )];
    spans.extend(state);

    let key_style = Style::default().fg(KEY_FG).bg(KEY_BG);
    let label_style = Style::default().fg(LABEL_FG);
    for (i, (key, label)) in hints.iter().take(shown).enumerate() {
        let gap = base_gap + usize::from(i < extra.saturating_sub(1));
        spans.push(Span::raw(" ".repeat(gap)));
        spans.push(Span::styled(format!(" {key} "), key_style));
        spans.push(Span::styled(label.to_string(), label_style));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(BG)),
        area,
    );
}

/// Información para la tira de estado de Pawan.
pub struct PawanStatus<'a> {
    pub mode: &'a str,
    pub thinking: Option<&'a str>,
    pub branch: Option<&'a str>,
    pub goal_active: bool,
    pub loop_active: bool,
    pub model: &'a str,
    pub total_tokens: u64,
    pub context_pct: f32,
    pub iteration: u32,
    pub timestamp: &'a str,
}

/// Tira inferior de Pawan: modo y contexto a la izquierda; modelo, tokens y
/// uso de contexto a la derecha, según el renderer del repositorio.
pub fn render_pawan(frame: &mut Frame, area: Rect, status: &PawanStatus<'_>) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let accent = Style::default().fg(Color::Rgb(0, 204, 204));
    let muted = Style::default().fg(Color::Rgb(136, 136, 136));
    let foreground = Style::default().fg(Color::White);
    let warning = Style::default().fg(Color::Rgb(255, 184, 108));

    let mut left = vec![Span::styled(
        format!(" {} ", status.mode),
        accent.add_modifier(Modifier::BOLD),
    )];
    if let Some(thinking) = status.thinking {
        left.push(Span::styled(format!(" {thinking}"), warning));
    }
    if status.goal_active {
        left.push(Span::styled(" GOAL ", accent.add_modifier(Modifier::BOLD)));
    }
    if status.loop_active {
        left.push(Span::styled(" LOOP ", warning.add_modifier(Modifier::BOLD)));
    }
    if let Some(branch) = status.branch.filter(|branch| !branch.trim().is_empty()) {
        left.push(Span::styled(format!(" ⎇{}", branch.trim()), muted));
    }

    let narrow = area.width < 60;
    let model = if narrow && status.model.chars().count() > 20 {
        format!(
            "…{}",
            status
                .model
                .chars()
                .skip(status.model.chars().count() - 20)
                .collect::<String>()
        )
    } else {
        status.model.to_string()
    };
    let pct = (status.context_pct.clamp(0.0, 1.0) * 100.0).round() as u32;
    let bar_width = if area.width >= 80 { 10 } else { 6 };
    let bar = context_bar(status.context_pct, bar_width);
    let sep = Span::styled("  |  ", muted);
    let mut right = vec![Span::styled(model, accent), sep.clone()];
    right.push(Span::styled(format_tokens(status.total_tokens), foreground));
    right.push(sep.clone());
    right.push(Span::styled(format!("ctx {pct}% "), foreground));
    right.push(Span::styled(bar, muted));
    if status.iteration > 0 {
        right.push(sep.clone());
        right.push(Span::styled(format!("iter {}", status.iteration), muted));
    }
    right.push(sep);
    right.push(Span::styled(format!("{} ", status.timestamp), muted));

    let right_width = right.iter().map(|span| span.width() as u16).sum::<u16>();
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(right_width)]).areas(area);
    frame.render_widget(Paragraph::new(Line::from(left)), left_area);
    frame.render_widget(
        Paragraph::new(Line::from(right)).alignment(Alignment::Right),
        right_area,
    );
}

fn format_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M tok", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k tok", tokens as f64 / 1_000.0)
    } else {
        format!("{tokens} tok")
    }
}

fn context_bar(value: f32, width: usize) -> String {
    let pct = value.clamp(0.0, 1.0);
    let filled = ((pct * width as f32).round() as usize).min(width);
    (0..width)
        .map(|index| if index < filled { '█' } else { '░' })
        .collect()
}
