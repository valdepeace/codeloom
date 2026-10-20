//! Ajustes Model/server de Mindfork desde `settings-model-dark-en.png`.

use crate::demo::Demo;
use batten::widgets::{settings_panel, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Block;
use ratatui::Frame;

pub struct MindforkModelSettingsDemo;

impl Demo for MindforkModelSettingsDemo {
    fn title(&self) -> &str {
        "Mindfork · settings/model"
    }

    fn hint(&self) -> &str {
        "Model/server · Assistant · Server · Model · Performance"
    }

    fn fullscreen(&self) -> bool {
        true
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        frame.render_widget(
            Block::default().style(Style::default().bg(ratatui::style::Color::Rgb(15, 16, 19))),
            area,
        );
        settings_panel::render_mindfork_model(frame, area);
    }

    fn handle_key(&mut self, _code: KeyCode) -> bool {
        false
    }
}
