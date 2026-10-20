//! Header compacto y footer de Crush.

use crate::demo::Demo;
use batten::widgets::Theme;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct CrushCompactHeaderDemo {
    details_open: bool,
}

impl CrushCompactHeaderDemo {
    pub fn new() -> Self {
        Self {
            details_open: false,
        }
    }
}

impl Demo for CrushCompactHeaderDemo {
    fn title(&self) -> &str {
        "Crush · compact header"
    }

    fn hint(&self) -> &str {
        "d: alternar detalles · marca, diagonales, ruta, rama y uso"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let brand = theme.styles.accent.add_modifier(Modifier::BOLD);
        let muted = theme.styles.muted;
        let mut spans = vec![
            Span::styled("CRUSH ", brand),
            Span::styled("╱".repeat(area.width.min(6) as usize), muted),
            Span::styled("  ~/src/loom", theme.styles.default),
            Span::styled("  • main", muted),
        ];
        if self.details_open {
            spans.extend([
                Span::styled("  • ", muted),
                Span::styled("42% context", theme.styles.success),
                Span::styled("  • ctrl+d close", muted),
            ]);
        } else {
            spans.extend([
                Span::styled("  • 42%", theme.styles.success),
                Span::styled("  • ctrl+d open", muted),
            ]);
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        if code != KeyCode::Char('d') {
            return false;
        }
        self.details_open = !self.details_open;
        true
    }
}

pub struct CrushFooterDemo {
    plan: bool,
    yolo: bool,
    router: bool,
}

impl CrushFooterDemo {
    pub fn new() -> Self {
        Self {
            plan: false,
            yolo: false,
            router: true,
        }
    }
}

impl Demo for CrushFooterDemo {
    fn title(&self) -> &str {
        "Crush · status/help footer"
    }

    fn hint(&self) -> &str {
        "p: plan · y: YOLO · r: mostrar/ocultar badge del router"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let mut spans = vec![Span::raw(" ")];
        if self.plan {
            spans.push(Span::styled(
                " PLAN ",
                theme.styles.accent.add_modifier(Modifier::BOLD),
            ));
        }
        if self.yolo {
            spans.push(Span::styled(
                " YOLO ",
                theme.styles.warning.add_modifier(Modifier::BOLD),
            ));
        }
        spans.extend([
            Span::styled(" ctrl+c ", theme.styles.heading),
            Span::styled("quit", theme.styles.muted),
            Span::styled("   ↑↓ ", theme.styles.heading),
            Span::styled("history", theme.styles.muted),
            Span::styled("   ctrl+p ", theme.styles.heading),
            Span::styled("commands", theme.styles.muted),
        ]);
        if self.router {
            spans.extend([
                Span::styled("   ", theme.styles.muted),
                Span::styled("router: claude-sonnet · high", theme.styles.accent),
            ]);
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char('p') => self.plan = !self.plan,
            KeyCode::Char('y') => self.yolo = !self.yolo,
            KeyCode::Char('r') => self.router = !self.router,
            _ => return false,
        }
        true
    }
}
