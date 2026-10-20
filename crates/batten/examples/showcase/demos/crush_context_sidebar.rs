//! Sidebar de contexto de Crush, reconstruida a partir de su estructura visible.

use crate::demo::Demo;
use batten::widgets::Theme;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

pub struct CrushContextSidebarDemo;

impl Demo for CrushContextSidebarDemo {
    fn title(&self) -> &str {
        "Crush · context sidebar"
    }

    fn hint(&self) -> &str {
        "panel de sesión, modelo y recursos · ancho estrecho para ver el recorte"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, active_theme: &Theme) {
        render_crush_sidebar(frame, area, active_theme);
    }
}

/// The persistent left rail shown beside Crush's transcript and composer.
pub(super) fn render_crush_sidebar(frame: &mut Frame, area: Rect, theme: &Theme) {
    let logo_height = if area.height < 30 { 1 } else { 7 };
    let logo_area = Rect::new(area.x, area.y, area.width, area.height.min(logo_height));
    if logo_height == 1 {
        super::crush_logo::render_small(frame, logo_area);
    } else {
        super::crush_logo::render_sidebar(frame, logo_area);
    }

    let content = Rect::new(
        area.x,
        area.y.saturating_add(logo_area.height),
        area.width,
        area.height.saturating_sub(logo_area.height),
    );
    let accent = theme.styles.accent;
    let muted = theme.styles.muted;
    let text = theme.styles.default;
    let good = theme.styles.success;
    let heading = |title| Line::from(Span::styled(title, accent.add_modifier(Modifier::BOLD)));
    let item = |label| Line::from(vec![Span::styled("● ", good), Span::raw(label)]);
    let lines = vec![
        Line::from(Span::styled(
            "Refactor auth middleware",
            text.add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled("~/src/loom  •  main", muted)),
        Line::from(""),
        heading("MODEL"),
        Line::from("Claude Sonnet 4.5"),
        Line::from(Span::styled("Anthropic  •  Thinking high", muted)),
        Line::from(Span::styled("Context  42%  ▰▰▰▰▱▱▱▱", muted)),
        Line::from(""),
        heading("FILES"),
        item("auth.rs  +18  -4"),
        item("middleware.rs  +7  -1"),
        Line::from(""),
        heading("LANGUAGE SERVERS"),
        item("rust-analyzer"),
        Line::from(""),
        heading("MCP SERVERS"),
        item("github"),
        Line::from(""),
        heading("SKILLS"),
        item("review-pr"),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::LEFT).border_style(muted))
            .style(text),
        content,
    );
}
