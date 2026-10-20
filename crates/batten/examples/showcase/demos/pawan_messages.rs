//! Roles y mensajes de Pawan basados en el snapshot del transcript.

use crate::demo::Demo;
use batten::widgets::transcript::{self, PawanMessage, PawanRole};
use batten::widgets::{Anim, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Block;
use ratatui::Frame;

pub struct PawanMessagesDemo {
    streaming: bool,
    waiting: bool,
    anim: Anim,
}

impl PawanMessagesDemo {
    pub fn new() -> Self {
        Self {
            streaming: false,
            waiting: false,
            anim: Anim::new(),
        }
    }
}

impl Demo for PawanMessagesDemo {
    fn title(&self) -> &str {
        "Pawan · messages"
    }

    fn hint(&self) -> &str {
        "s: cursor streaming · w: Pawan is thinking · blink animado"
    }

    fn tick(&mut self, anim: &Anim) {
        self.anim = *anim;
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        frame.render_widget(
            Block::default().style(Style::default().bg(Color::Black)),
            area,
        );
        let messages = [
            PawanMessage {
                role: PawanRole::User,
                elapsed: "now",
                content: "Hello pawan",
                streaming: false,
            },
            PawanMessage {
                role: PawanRole::Assistant,
                elapsed: "now",
                content: "Hi there!\nI can help you inspect and change this project.",
                streaming: self.streaming,
            },
            PawanMessage {
                role: PawanRole::Tool,
                elapsed: "1.2s",
                content: "view src/main.rs · 128 lines",
                streaming: false,
            },
            PawanMessage {
                role: PawanRole::Error,
                elapsed: "2.1s",
                content: "Request failed: connection closed",
                streaming: false,
            },
        ];
        let waiting_height = if self.waiting { 1 } else { 0 };
        let transcript_area = Rect::new(
            area.x,
            area.y,
            area.width,
            area.height.saturating_sub(waiting_height),
        );
        transcript::render_pawan(frame, transcript_area, &messages);
        if self.waiting && waiting_height > 0 {
            transcript::render_pawan_waiting(
                frame,
                Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1),
                &self.anim,
            );
        }
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char('s') => self.streaming = !self.streaming,
            KeyCode::Char('w') => self.waiting = !self.waiting,
            _ => return false,
        }
        true
    }
}
