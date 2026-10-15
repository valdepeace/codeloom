//! Demo del transcript con una línea de cada tipo.

use crate::demo::Demo;
use batten::state::{Line, LineKind, Transcript, Viewport};
use batten::widgets::{transcript, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::Frame;

pub struct TranscriptDemo {
    transcript: Transcript,
    viewport: Viewport,
}

impl TranscriptDemo {
    pub fn new() -> Self {
        let mut transcript = Transcript::new();
        for (kind, text) in [
            (LineKind::User, "lee hello.txt y dime qué contiene"),
            (LineKind::System, "modelo mistral-nemo"),
            (LineKind::Assistant, "hola mundo"),
            (LineKind::Thinking, "el archivo parece un saludo"),
            (LineKind::ToolCall, "view hello.txt"),
            (LineKind::ToolResult, "6 líneas, 12 bytes"),
            (LineKind::Todo, "[x] leer hello.txt"),
            (LineKind::Plan, "1. leer 2. responder"),
            (LineKind::Banner, "sesión iniciada"),
            (LineKind::Error, "no se pudo escribir el archivo"),
        ] {
            transcript.push(Line::new(kind, text));
        }
        // Relleno para que haya más líneas que filas y se vea el scrollbar.
        for i in 0..30 {
            transcript.push(Line::new(
                LineKind::Assistant,
                format!("línea de relleno {i}"),
            ));
        }
        Self {
            transcript,
            viewport: Viewport::new(),
        }
    }
}

impl Demo for TranscriptDemo {
    fn title(&self) -> &str {
        "transcript"
    }

    fn hint(&self) -> &str {
        "↓/↑: scroll · g/G: inicio/fin"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        transcript::render(f, area, &self.transcript, &mut self.viewport, theme);
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Down => self.viewport.scroll_down(1),
            KeyCode::Up => self.viewport.scroll_up(1),
            KeyCode::Char('g') => self.viewport.scroll_up(usize::MAX),
            KeyCode::Char('G') => self.viewport.follow_bottom(),
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyCode;

    #[test]
    fn scroll_changes_the_window() {
        let mut d = TranscriptDemo::new();
        d.viewport.set_geometry(d.transcript.len(), 5);
        d.handle_key(KeyCode::Char('G')); // al final
        let bottom = d.viewport.window();
        d.handle_key(KeyCode::Char('g')); // al principio
        let top = d.viewport.window();
        assert_ne!(bottom, top);
        assert_eq!(top.start, 0);
    }
}
