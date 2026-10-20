//! Vista Self-model de Mindfork: about, goals, traits y observations.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use ratatui::Frame;

const BG: Color = Color::Rgb(15, 16, 19);
const FG: Color = Color::Rgb(201, 204, 210);
const MUTED: Color = Color::Rgb(133, 139, 146);
const GOLD: Color = Color::Rgb(230, 201, 154);
const GREEN: Color = Color::Rgb(111, 192, 130);
const BORDER: Color = Color::Rgb(54, 58, 66);

/// Entrada de goal guardado por Mindfork.
pub struct ProfileGoal<'a> {
    pub text: &'a str,
    pub date: &'a str,
    pub done: bool,
}

/// Observación fechada del perfil.
pub struct ProfileObservation<'a> {
    pub date: &'a str,
    pub text: &'a str,
}

/// Renderiza los paneles principales de Self-model de Mindfork.
// La vista mantiene explícitos los campos independientes del perfil.
#[allow(clippy::too_many_arguments)]
pub fn render_mindfork(
    frame: &mut Frame,
    area: Rect,
    about: &str,
    goals: &[ProfileGoal<'_>],
    traits: &str,
    interests: &str,
    relationship: &str,
    observations: &[ProfileObservation<'_>],
) {
    if area.width < 20 || area.height < 8 {
        return;
    }
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .title(" ✦ Self-model ")
        .style(Style::default().bg(BG));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let mut lines = vec![Line::from(Span::styled(
        " Assistant",
        Style::default().fg(GOLD).add_modifier(Modifier::BOLD),
    ))];
    lines.push(Line::from(vec![
        Span::styled("▏ ", Style::default().fg(Color::Rgb(201, 204, 210))),
        Span::styled("About me: ", Style::default().fg(GOLD)),
        Span::styled(about.to_string(), Style::default().fg(FG)),
    ]));
    lines.push(Line::raw(""));
    for goal in goals {
        lines.push(Line::from(vec![
            Span::styled(
                if goal.done { "  ✓ " } else { "  ● " },
                Style::default().fg(if goal.done { MUTED } else { GREEN }),
            ),
            Span::styled(goal.text.to_string(), Style::default().fg(FG)),
            Span::styled(format!("  [{}]", goal.date), Style::default().fg(MUTED)),
        ]));
    }
    lines.push(Line::from(Span::styled(
        "  + add goal",
        Style::default().fg(MUTED),
    )));
    lines.extend([
        Line::raw(""),
        Line::from(Span::styled(
            " User",
            Style::default().fg(GOLD).add_modifier(Modifier::BOLD),
        )),
        labeled("Traits:", traits),
        labeled("Interests:", interests),
        labeled("Relationship:", relationship),
        Line::raw(""),
        Line::from(Span::styled(
            format!(" Observations ({})", observations.len()),
            Style::default().fg(GOLD).add_modifier(Modifier::BOLD),
        )),
    ]);
    lines.extend(observations.iter().map(|observation| {
        Line::from(vec![
            Span::styled(
                format!(" [{}] ", observation.date),
                Style::default().fg(MUTED),
            ),
            Span::styled(observation.text.to_string(), Style::default().fg(FG)),
        ])
    }));
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().bg(BG))
            .wrap(Wrap { trim: false }),
        inner,
    );
    let footer = Line::from(vec![
        Span::styled(
            " ↑↓ ",
            Style::default().fg(MUTED).bg(Color::Rgb(33, 36, 42)),
        ),
        Span::styled(" select   ", Style::default().fg(MUTED)),
        Span::styled(
            " Enter ",
            Style::default().fg(MUTED).bg(Color::Rgb(33, 36, 42)),
        ),
        Span::styled(" edit   ", Style::default().fg(MUTED)),
        Span::styled(
            " Ctrl+K ",
            Style::default().fg(MUTED).bg(Color::Rgb(33, 36, 42)),
        ),
        Span::styled(" clear", Style::default().fg(MUTED)),
    ]);
    frame.render_widget(
        Paragraph::new(footer),
        Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
    );
}

fn labeled(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!(" {label} "), Style::default().fg(GOLD)),
        Span::styled(value.to_string(), Style::default().fg(FG)),
    ])
}
