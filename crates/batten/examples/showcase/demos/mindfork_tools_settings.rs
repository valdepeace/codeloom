//! Settings/Tools de Mindfork basada en el screenshot dark/en.

use crate::demo::Demo;
use batten::widgets::{settings_panel, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Block;
use ratatui::Frame;

pub struct MindforkToolsSettingsDemo;

impl Demo for MindforkToolsSettingsDemo {
    fn title(&self) -> &str {
        "Mindfork · settings/tools"
    }

    fn hint(&self) -> &str {
        "Sections · Agentic loop · Web search · Python"
    }

    fn fullscreen(&self) -> bool {
        true
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        frame.render_widget(
            Block::default().style(Style::default().bg(ratatui::style::Color::Rgb(15, 16, 19))),
            area,
        );
        settings_panel::render_mindfork_tools(frame, area);
    }

    fn handle_key(&mut self, _code: KeyCode) -> bool {
        false
    }
}
