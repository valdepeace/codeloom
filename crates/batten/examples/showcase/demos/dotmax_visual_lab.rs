//! Muestras Ratatui de las primitivas Braille, densidad y color de dotmax.

use crate::demo::Demo;
use batten::widgets::{Anim, Theme};
use dotmax::color::schemes::{get_scheme, list_schemes};
use dotmax::density::DensitySet;
use dotmax::primitives::{draw_circle, draw_line, draw_polygon, draw_rectangle};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

const VIEWS: [&str; 4] = [
    "braille primitives",
    "density ramps",
    "color schemes",
    "capability map",
];
const DENSITIES: [(&str, fn() -> DensitySet); 4] = [
    ("ASCII", DensitySet::ascii),
    ("Simple", DensitySet::simple),
    ("Blocks", DensitySet::blocks),
    ("Braille", DensitySet::braille),
];

pub struct DotmaxVisualLabDemo {
    view: usize,
    elapsed: f32,
}

impl DotmaxVisualLabDemo {
    pub fn new() -> Self {
        Self {
            view: 0,
            elapsed: 0.0,
        }
    }

    fn render_shapes(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let width = usize::from(area.width).max(1);
        let height = usize::from(area.height).max(1);
        let Ok(mut grid) = dotmax::BrailleGrid::new(width, height) else {
            return;
        };
        let dot_width = (width * 2) as i32;
        let dot_height = (height * 4) as i32;
        let center_x = dot_width / 2;
        let center_y = dot_height / 2;
        let radius = ((dot_height / 3).max(1) as f32 * (0.85 + 0.15 * (self.elapsed * 2.0).sin()))
            .round() as u32;
        let _ = draw_circle(&mut grid, center_x, center_y, radius);
        let _ = draw_line(&mut grid, 0, 0, dot_width - 1, dot_height - 1);
        let _ = draw_rectangle(
            &mut grid,
            dot_width / 8,
            dot_height / 8,
            (dot_width * 3 / 4).max(1) as u32,
            (dot_height * 3 / 4).max(1) as u32,
        );
        if dot_width > 20 && dot_height > 20 {
            let points = [
                (dot_width / 2, dot_height / 5),
                (dot_width * 3 / 5, dot_height * 2 / 3),
                (dot_width * 2 / 5, dot_height * 2 / 3),
            ];
            let _ = draw_polygon(&mut grid, &points);
        }
        render_grid(frame, area, &grid, theme);
    }

    fn render_density(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let rows = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(area);
        let width = usize::from(rows[3].width).max(1);
        let header = Line::from(Span::styled(
            "Intensity → sparse-to-dense character mapping",
            theme.styles.muted,
        ));
        frame.render_widget(Paragraph::new(header), rows[0]);

        for (index, (name, make_set)) in DENSITIES.iter().enumerate() {
            let set = make_set();
            let line = (0..width)
                .map(|x| {
                    let intensity = if width <= 1 {
                        0.0
                    } else {
                        x as f32 / (width - 1) as f32
                    };
                    set.map(intensity)
                })
                .collect::<String>();
            let styled = Line::from(vec![
                Span::styled(format!("{name:<8} "), theme.styles.accent),
                Span::raw(line),
            ]);
            frame.render_widget(Paragraph::new(styled), rows[index + 1]);
        }
    }

    fn render_schemes(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let names = list_schemes();
        let rows = Layout::vertical(vec![Constraint::Length(1); names.len() + 1]).split(area);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "Seven built-in gradients · low intensity → high intensity",
                theme.styles.muted,
            ))),
            rows[0],
        );

        for (index, name) in names.iter().enumerate() {
            let Some(scheme) = get_scheme(name) else {
                continue;
            };
            let mut spans = vec![Span::styled(
                format!("{name:<14}"),
                theme.styles.accent.add_modifier(Modifier::BOLD),
            )];
            let bar_width = usize::from(rows[index + 1].width).saturating_sub(15);
            for x in 0..bar_width {
                let intensity = if bar_width <= 1 {
                    0.0
                } else {
                    x as f32 / (bar_width - 1) as f32
                };
                let color = scheme.sample(intensity);
                spans.push(Span::styled(
                    "█",
                    Style::default().fg(Color::Rgb(color.r, color.g, color.b)),
                ));
            }
            frame.render_widget(Paragraph::new(Line::from(spans)), rows[index + 1]);
        }
    }

    fn render_capabilities(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let entries = [
            ("Core", "BrailleGrid, custom glyphs and drawing primitives — demo interactiva."),
            ("Progress", "692 loading/progress styles in 60 themes — catálogo navegable."),
            ("Density", "ASCII, simple, block and Braille density mapping — demo interactiva."),
            ("Color", "Seven built-in color schemes and custom palettes — demo interactiva."),
            ("Animation", "FrameBuffer, differential rendering, timers and prerendered playback — Ratatui drives this showcase's frame loop."),
            ("Export", "Capture and serialize Braille frames to JSON frame packs — API, not yet demoed."),
            ("Images / SVG", "Feature-gated image processing, GIF/APNG and SVG rendering; features image/svg are disabled here."),
            ("Video / webcam", "Feature video requires FFmpeg; webcam additionally needs a camera and terminal ownership."),
            ("Other features", "Raytracer and chess are optional modules; not terminal UI components for this gallery."),
        ];
        let lines = entries
            .iter()
            .map(|(name, description)| {
                Line::from(vec![
                    Span::styled(
                        format!("{name:<14}"),
                        theme.styles.accent.add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(*description),
                ])
            })
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
    }
}

impl Demo for DotmaxVisualLabDemo {
    fn title(&self) -> &str {
        "dotmax visual lab"
    }

    fn hint(&self) -> &str {
        "←→: primitivas · densidad · paletas · capacidades"
    }

    fn tick(&mut self, anim: &Anim) {
        self.elapsed = anim.frame as f32 * 0.05;
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Left => self.view = (self.view + VIEWS.len() - 1) % VIEWS.len(),
            KeyCode::Right => self.view = (self.view + 1) % VIEWS.len(),
            _ => return false,
        }
        true
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if area.width < 10 || area.height < 3 {
            return;
        }
        let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(area);
        let heading = Line::from(vec![
            Span::styled("dotmax", theme.styles.accent.add_modifier(Modifier::BOLD)),
            Span::raw(format!(
                "  ·  {}/{}  ·  {}",
                self.view + 1,
                VIEWS.len(),
                VIEWS[self.view]
            )),
        ]);
        frame.render_widget(Paragraph::new(heading), rows[0]);
        match self.view {
            0 => self.render_shapes(frame, rows[1], theme),
            1 => self.render_density(frame, rows[1], theme),
            2 => self.render_schemes(frame, rows[1], theme),
            _ => self.render_capabilities(frame, rows[1], theme),
        }
    }
}

fn render_grid(frame: &mut Frame, area: Rect, grid: &dotmax::BrailleGrid, theme: &Theme) {
    let (width, height) = grid.dimensions();
    let lines = (0..height)
        .map(|y| {
            let spans = (0..width)
                .map(|x| {
                    let glyph = grid.get_char(x, y);
                    let glyph = if grid.is_empty(x, y) { ' ' } else { glyph };
                    let color = grid
                        .get_color(x, y)
                        .map(|color| Color::Rgb(color.r, color.g, color.b))
                        .unwrap_or(theme.styles.accent.fg.unwrap_or(Color::Reset));
                    Span::styled(glyph.to_string(), Style::default().fg(color))
                })
                .collect::<Vec<_>>();
            Line::from(spans)
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), area);
}
