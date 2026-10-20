//! Landing de Crush con contexto de proyecto y recursos.

use crate::demo::Demo;
use batten::widgets::Theme;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct CrushLandingDemo;

impl Demo for CrushLandingDemo {
    fn title(&self) -> &str {
        "Crush · landing"
    }

    fn hint(&self) -> &str {
        "cabecera de marca · proyecto/modelo · columnas de LSP, MCP y Skills"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let [brand, context, resources, prompt] = Layout::vertical([
            Constraint::Length(4),
            Constraint::Length(5),
            Constraint::Min(6),
            Constraint::Length(3),
        ])
        .areas(area);
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "CRUSH",
                    theme.styles.accent.add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled("Your coding companion", theme.styles.muted)),
            ]),
            brand,
        );
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "~/src/loom  •  main",
                    theme.styles.heading.add_modifier(Modifier::BOLD),
                )),
                Line::from("Claude Sonnet 4.5  ·  Anthropic"),
                Line::from(Span::styled(
                    "Thinking high  ·  context 42%",
                    theme.styles.muted,
                )),
            ]),
            context,
        );
        let [lsp, mcp, skills] = Layout::horizontal([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .areas(resources);
        let sections = [
            ("LANGUAGE SERVERS", "● rust-analyzer", "2 diagnostics"),
            ("MCP SERVERS", "● github", "● filesystem"),
            ("SKILLS", "● review-pr", "● summarize"),
        ];
        for (column, (heading, first, second)) in [lsp, mcp, skills].into_iter().zip(sections) {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled(
                        heading,
                        theme.styles.accent.add_modifier(Modifier::BOLD),
                    )),
                    Line::from(Span::styled(first, theme.styles.success)),
                    Line::from(Span::styled(second, theme.styles.muted)),
                ]),
                column,
            );
        }
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled("> ", theme.styles.accent)),
                Line::from(Span::styled("Ask about this project…", theme.styles.muted)),
            ]),
            prompt,
        );
    }
}
