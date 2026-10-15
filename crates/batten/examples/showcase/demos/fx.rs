//! Demo de efectos: una tecla por preset, aplicado sobre un panel.

use crate::demo::Demo;
use batten::state::Tone;
use batten::widgets::fx::{
    dialog_open, error_flash, fade_out, message_in, toast_in, Effect, FxDuration,
};
use batten::widgets::{render_panel, Panel, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct FxDemo {
    pending: Option<char>,
    effect: Option<Effect>,
    name: &'static str,
}

impl FxDemo {
    pub fn new() -> Self {
        Self {
            pending: None,
            effect: None,
            name: "",
        }
    }
}

impl Demo for FxDemo {
    fn title(&self) -> &str {
        "fx"
    }

    fn hint(&self) -> &str {
        "o t f m e: lanzar efecto"
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char(c @ ('o' | 't' | 'f' | 'm' | 'e')) => {
                self.pending = Some(c);
                true
            }
            _ => false,
        }
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        if let Some(c) = self.pending.take() {
            let (name, e) = match c {
                'o' => ("dialog_open", dialog_open(theme)),
                't' => ("toast_in", toast_in(theme)),
                'f' => ("fade_out", fade_out(400, theme)),
                'm' => ("message_in", message_in(theme)),
                _ => ("error_flash", error_flash(theme)),
            };
            self.name = name;
            self.effect = Some(e);
        }
        let panel = Panel {
            title: "Efectos",
            info: self.name,
            tone: Tone::Neutral,
        };
        let inner = render_panel(f, area, &panel, theme);
        let lines = vec![
            Line::from("o dialog_open · t toast_in · f fade_out"),
            Line::from("m message_in · e error_flash"),
            Line::from("El efecto se aplica a todo este panel."),
        ];
        f.render_widget(Paragraph::new(lines), inner);
        if let Some(e) = self.effect.as_mut() {
            e.process(FxDuration::from_millis(50), f.buffer_mut(), area);
        }
    }
}
