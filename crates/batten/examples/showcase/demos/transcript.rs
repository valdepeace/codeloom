//! Demo del transcript con una línea de cada tipo.

use crate::demo::Demo;
use batten::state::{Line, LineKind, Selection, Transcript, Viewport};
use batten::widgets::{transcript, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct TranscriptDemo {
    transcript: Transcript,
    viewport: Viewport,
    selection: Option<Selection>,
    copied: Option<String>,
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
            selection: None,
            copied: None,
        }
    }
}

impl Demo for TranscriptDemo {
    fn title(&self) -> &str {
        "transcript"
    }

    fn hint(&self) -> &str {
        "↓/↑: scroll · g/G · v: seleccionar · j/k: extender · y: copiar"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let [body, status] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);
        transcript::render_with_selection(
            f,
            body,
            &self.transcript,
            &mut self.viewport,
            self.selection.as_ref(),
            theme,
        );
        let msg = match (&self.copied, &self.selection) {
            (Some(text), _) => format!("copiado: {}", text.replace('\n', " ⏎ ")),
            (None, Some(s)) => format!("seleccionadas {} líneas · y: copiar", s.range().count()),
            (None, None) => "v: seleccionar".into(),
        };
        f.render_widget(Paragraph::new(msg).style(theme.styles.muted), status);
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        let len = self.transcript.len();
        if let Some(mut s) = self.selection {
            match code {
                KeyCode::Up | KeyCode::Char('k') => s.extend(-1, len),
                KeyCode::Down | KeyCode::Char('j') => s.extend(1, len),
                KeyCode::Char('y') => {
                    self.copied = Some(s.text(&self.transcript));
                    self.selection = None;
                    return true;
                }
                KeyCode::Char('v') => {
                    self.selection = None;
                    return true;
                }
                _ => return false,
            }
            self.selection = Some(s);
            return true;
        }
        match code {
            KeyCode::Char('v') if len > 0 => {
                self.copied = None;
                self.selection = Some(Selection::at(len - 1));
            }
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

    #[test]
    fn select_extend_and_copy() {
        // 10 líneas de tipos variados + 30 de relleno (0..29): v selecciona la
        // última y k extiende a la penúltima.
        let mut d = TranscriptDemo::new();
        d.handle_key(KeyCode::Char('v'));
        d.handle_key(KeyCode::Char('k'));
        d.handle_key(KeyCode::Char('y'));
        assert!(d.selection.is_none());
        assert_eq!(
            d.copied.as_deref(),
            Some("línea de relleno 28\nlínea de relleno 29")
        );
    }
}
