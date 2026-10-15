//! Demo de avisos: `t` lanza uno (rota de tono), caduca solo.

use crate::demo::Demo;
use batten::state::{Toasts, Tone};
use batten::widgets::{render_toast, Anim, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct ToastDemo {
    toasts: Toasts,
    next: usize,
}

impl ToastDemo {
    pub fn new() -> Self {
        Self {
            toasts: Toasts::new(),
            next: 0,
        }
    }
}

impl Demo for ToastDemo {
    fn title(&self) -> &str {
        "toast"
    }

    fn hint(&self) -> &str {
        "t: lanzar aviso"
    }

    fn tick(&mut self, _anim: &Anim) {
        self.toasts.tick();
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        if code != KeyCode::Char('t') {
            return false;
        }
        let tone = Tone::ALL[self.next % Tone::ALL.len()];
        self.next += 1;
        self.toasts
            .push(tone, format!("Aviso #{} ({tone:?})", self.next), 40);
        true
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let mut lines = vec![Line::from("pulsa t; cada aviso dura 40 frames (~2 s)")];
        lines.extend(render_toast(&self.toasts, area.width, theme));
        f.render_widget(Paragraph::new(lines), area);
    }
}
