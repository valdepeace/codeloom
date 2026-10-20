//! Tira de estado de Pawan con modelo, tokens y uso de contexto.

use crate::demo::Demo;
use batten::widgets::status_bar::PawanStatus;
use batten::widgets::{status_bar, Theme};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct PawanStatusBarDemo;

impl PawanStatusBarDemo {
    pub fn new() -> Self {
        Self
    }
}

impl Demo for PawanStatusBarDemo {
    fn title(&self) -> &str {
        "Pawan · status bar"
    }

    fn hint(&self) -> &str {
        "modo · rama · modelo · tokens · contexto"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        let [body, footer] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);
        frame.render_widget(
            Paragraph::new("Pawan · status strip")
                .style(Style::default().fg(Color::White).bg(Color::Black)),
            body,
        );
        status_bar::render_pawan(
            frame,
            footer,
            &PawanStatus {
                mode: "build",
                thinking: Some("high"),
                branch: Some("main"),
                goal_active: false,
                loop_active: false,
                model: "deepseek-v4-flash",
                total_tokens: 12_400,
                context_pct: 0.72,
                iteration: 3,
                timestamp: "12:41",
            },
        );
    }
}
