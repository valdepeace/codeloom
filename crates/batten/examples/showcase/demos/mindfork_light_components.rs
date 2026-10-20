//! Vistas light derivadas de los screenshots en inglés de Mindfork.

use crate::demo::Demo;
use batten::widgets::{settings_panel, Theme};
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::Frame;

pub struct MindforkLightComponentsDemo {
    views: Vec<Box<dyn Demo>>,
    selected: usize,
}

impl MindforkLightComponentsDemo {
    pub fn new() -> Self {
        Self {
            views: vec![
                Box::new(super::mindfork_reference::MindforkReference::new()),
                Box::new(super::mindfork_chat_list::MindforkChatListDemo::new()),
                Box::new(super::mindfork_self_model::MindforkSelfModelDemo),
                Box::new(super::mindfork_tools_settings::MindforkToolsSettingsDemo),
                Box::new(super::mindfork_model_settings::MindforkModelSettingsDemo),
            ],
            selected: 0,
        }
    }
}

impl Demo for MindforkLightComponentsDemo {
    fn title(&self) -> &str {
        "Mindfork · light components"
    }
    fn hint(&self) -> &str {
        "Tab cambia chat/list/self-model/tools/model · controles internos conservados"
    }
    fn fullscreen(&self) -> bool {
        true
    }
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        self.views[self.selected].render(frame, area, theme);
        settings_panel::apply_mindfork_light(frame, area);
    }
    fn handle_key(&mut self, code: KeyCode) -> bool {
        if code == KeyCode::Tab {
            self.selected = (self.selected + 1) % self.views.len();
            return true;
        }
        self.views[self.selected].handle_key(code)
    }
    fn handle_key_event(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        if code == KeyCode::Tab && modifiers.is_empty() {
            self.selected = (self.selected + 1) % self.views.len();
            return true;
        }
        self.views[self.selected].handle_key_event(code, modifiers)
    }
}
