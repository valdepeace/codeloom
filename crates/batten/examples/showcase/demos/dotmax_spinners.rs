//! Galería interactiva de los indicadores de actividad de dotmax.

use crate::demo::Demo;
use batten::widgets::{Anim, Theme};
use dotmax::progress::{all_styles, styles_for_theme, themes, BarContext, ProgressStyle};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const DOTMAX_TICK: f32 = 0.05;

pub struct DotmaxSpinnersDemo {
    theme_index: usize,
    style_index: usize,
    elapsed: f32,
    total_styles: usize,
    styles: Vec<Box<dyn ProgressStyle>>,
}

impl DotmaxSpinnersDemo {
    pub fn new() -> Self {
        let styles = styles_for_theme("spinner");
        Self {
            theme_index: themes()
                .iter()
                .position(|theme| *theme == "spinner")
                .unwrap_or(0),
            style_index: 0,
            elapsed: 0.0,
            total_styles: all_styles().len(),
            styles,
        }
    }

    fn select_theme(&mut self, index: usize) {
        self.theme_index = index % themes().len();
        self.styles = styles_for_theme(themes()[self.theme_index]);
        self.style_index = 0;
    }

    fn selected_style(&self) -> &dyn ProgressStyle {
        self.styles[self.style_index % self.styles.len()].as_ref()
    }
}

impl Demo for DotmaxSpinnersDemo {
    fn title(&self) -> &str {
        "dotmax spinner gallery"
    }

    fn hint(&self) -> &str {
        "←→: tema · ↑↓: estilo · catálogo dotmax (MIT OR Apache-2.0)"
    }

    fn tick(&mut self, anim: &Anim) {
        self.elapsed = anim.frame as f32 * DOTMAX_TICK;
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Left => self.select_theme(self.theme_index + themes().len() - 1),
            KeyCode::Right => self.select_theme(self.theme_index + 1),
            KeyCode::Up => {
                self.style_index = self
                    .style_index
                    .checked_add(self.styles.len().saturating_sub(1))
                    .unwrap_or(0)
                    % self.styles.len();
            }
            KeyCode::Down => self.style_index = (self.style_index + 1) % self.styles.len(),
            _ => return false,
        }
        true
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if area.width < 4 || area.height < 3 || self.styles.is_empty() {
            return;
        }

        let rows = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(area);
        let style = self.selected_style();
        let theme_name = themes()[self.theme_index];
        let heading = Line::from(vec![
            Span::styled(theme_name, theme.styles.accent),
            Span::raw(format!(
                "  ·  {}/{}  ·  {}",
                self.style_index + 1,
                self.styles.len(),
                format!("{} · {} estilos total", style.name(), self.total_styles)
            )),
        ]);
        frame.render_widget(Paragraph::new(heading), rows[0]);

        let width = usize::from(rows[1].width).max(1);
        let height = usize::from(rows[1].height).max(1);
        let mut grid = match dotmax::BrailleGrid::new(width, height) {
            Ok(grid) => grid,
            Err(_) => return,
        };
        let context = BarContext::new(0.63, self.elapsed, width, height);
        if style.render(&mut grid, &context).is_err() {
            return;
        }

        let lines = (0..height)
            .map(|y| {
                let spans = (0..width)
                    .map(|x| {
                        let glyph = grid.get_char(x, y);
                        let glyph = if glyph == '\u{2800}' || glyph == ' ' {
                            " ".to_owned()
                        } else {
                            glyph.to_string()
                        };
                        let color = grid
                            .get_color(x, y)
                            .map(|color| Color::Rgb(color.r, color.g, color.b))
                            .unwrap_or(theme.styles.accent.fg.unwrap_or(Color::Reset));
                        Span::styled(glyph, Style::default().fg(color))
                    })
                    .collect::<Vec<_>>();
                Line::from(spans)
            })
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(lines), rows[1]);

        let description = Line::from(Span::styled(style.describe(), theme.styles.muted));
        frame.render_widget(Paragraph::new(vec![description, Line::raw("")]), rows[2]);
    }
}
