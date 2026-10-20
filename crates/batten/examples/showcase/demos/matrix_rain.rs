//! Fondo Matrix: start/stop y drenaje de las gotas en Batten.

use crate::demo::Demo;
use batten::widgets::{matrix_rain::MatrixRain, Anim, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct MatrixRainDemo {
    rain: MatrixRain,
    emitting: bool,
}

impl MatrixRainDemo {
    pub fn new() -> Self {
        Self {
            rain: MatrixRain::default(),
            emitting: false,
        }
    }
}

impl Demo for MatrixRainDemo {
    fn title(&self) -> &str {
        "Matrix rain background"
    }

    fn hint(&self) -> &str {
        "Space: iniciar/detener emisión · las gotas activas llegan al borde"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        self.rain.render(frame, area);
        let status = if self.emitting {
            "emitiendo lluvia · Space para detener"
        } else if self.rain.animating() {
            "drenando gotas · Space para reanudar"
        } else {
            "lluvia detenida · Space para iniciar"
        };
        let [status_row] = Layout::vertical([Constraint::Length(1)]).areas(area);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    " BATTEN / MATRIX ",
                    Style::default().fg(Color::Rgb(150, 255, 180)),
                ),
                Span::raw(format!(" {status}")),
            ])),
            status_row,
        );
    }

    fn tick(&mut self, _anim: &Anim) {
        self.rain.advance();
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        if matches!(code, KeyCode::Char(' ') | KeyCode::Enter) {
            self.emitting = !self.emitting;
            if self.emitting {
                self.rain.start();
            } else {
                self.rain.stop();
            }
            true
        } else {
            false
        }
    }
}
