//! Demo de salida ANSI: una compilación de cargo con sus colores.

use crate::demo::Demo;
use batten::widgets::ansi::ansi_lines;
use batten::widgets::Theme;
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const SAMPLE: &[u8] = b"\x1b[1m\x1b[32m   Compiling\x1b[0m batten v0.1.0\n\
\x1b[1m\x1b[33mwarning\x1b[0m: unused variable: `x`\n\
\x1b[1m\x1b[31merror[E0425]\x1b[0m: cannot find value `y` in this scope\n\
\x1b[1m\x1b[32m    Finished\x1b[0m `dev` profile in 2.1s";

pub struct AnsiDemo;

impl AnsiDemo {
    pub fn new() -> Self {
        Self
    }
}

impl Demo for AnsiDemo {
    fn title(&self) -> &str {
        "ansi"
    }

    fn hint(&self) -> &str {
        "salida de cargo con colores"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, _theme: &Theme) {
        f.render_widget(Paragraph::new(ansi_lines(SAMPLE)), area);
    }
}
