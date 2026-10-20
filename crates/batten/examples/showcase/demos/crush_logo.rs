//! Referencias visuales del logo de Crush; no copia su implementación Go.

use crate::demo::Demo;
use batten::widgets::Theme;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const DOLLY: Color = Color::Rgb(255, 96, 255);
const CHARPLE: Color = Color::Rgb(107, 80, 255);
const SIDEBAR_ART: [&str; 3] = [
    "▄▀▀▀▀ █▀▀▀▄ █   █ ▄▀▀▀▀ █   █",
    "█     █▀▀▀▄ █   █ ▀▀▀▀█ █▀▀▀█",
    " ▀▀▀▀ ▀   ▀  ▀▀▀  ▀▀▀▀  ▀   ▀",
];
// Rendered sample from Crush's wide logo. The extended S shows its stretched-letter treatment.
const WIDE_ART: [&str; 3] = [
    "▄▀▀▀▀ █▀▀▀▄ █   █ ▄▀▀▀▀▀▀▀▀▀ █   █",
    "█     █▀▀▀▄ █   █ ▀▀▀▀▀▀▀▀▀█ █▀▀▀█",
    " ▀▀▀▀ ▀   ▀  ▀▀▀  ▀▀▀▀▀▀▀▀▀  ▀   ▀",
];
const VERSION: &str = "v1.0.0";

#[derive(Clone, Copy)]
pub enum CrushLogoVariant {
    WideTitle,
    Sidebar,
    SidebarFallback,
}

pub struct CrushLogoDemo(pub CrushLogoVariant);

impl Demo for CrushLogoDemo {
    fn title(&self) -> &str {
        match self.0 {
            CrushLogoVariant::WideTitle => "Crush · logo ancho",
            CrushLogoVariant::Sidebar => "Crush · logo del sidebar",
            CrushLogoVariant::SidebarFallback => "Crush · fallback del sidebar",
        }
    }

    fn hint(&self) -> &str {
        match self.0 {
            CrushLogoVariant::WideTitle => {
                "Render ancho · arte CRUSH · gradiente · muestra del estiramiento aleatorio de una letra"
            }
            CrushLogoVariant::Sidebar => {
                "Render(compact=true) · arte CRUSH + Charm™/versión + campo de diagonales"
            }
            CrushLogoVariant::SidebarFallback => {
                "SmallRender · se usa cuando el sidebar no tiene alto suficiente"
            }
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        match self.0 {
            CrushLogoVariant::WideTitle => render_wide(frame, area),
            CrushLogoVariant::Sidebar => render_sidebar(frame, area),
            CrushLogoVariant::SidebarFallback => render_small(frame, area),
        }
    }
}

pub(super) fn render_sidebar(frame: &mut Frame, area: Rect) {
    let width = SIDEBAR_ART[0].chars().count();
    let field = "╱".repeat(width);
    let mut lines = vec![
        Line::styled(field.clone(), Style::default().fg(CHARPLE)),
        Line::styled(field.clone(), Style::default().fg(CHARPLE)),
    ];
    lines.push(metadata(width));
    lines.extend(gradient_art(&SIDEBAR_ART));
    lines.push(Line::styled(field, Style::default().fg(CHARPLE)));
    frame.render_widget(Paragraph::new(lines), area);
}

fn render_wide(frame: &mut Frame, area: Rect) {
    let art_width = WIDE_ART[0].chars().count();
    let left_width = 6;
    let right_width = usize::from(area.width)
        .saturating_sub(art_width + left_width + 2)
        .max(15);
    let mut lines = vec![metadata(art_width)];
    lines.extend(gradient_art(&WIDE_ART));
    let lines = lines
        .into_iter()
        .enumerate()
        .map(|(row, mut line)| {
            let spans = std::iter::once(Span::styled(
                "╱".repeat(left_width),
                Style::default().fg(CHARPLE),
            ))
            .chain(std::iter::once(Span::raw(" ")))
            .chain(line.spans.drain(..))
            .chain(std::iter::once(Span::raw(" ")))
            .chain(std::iter::once(Span::styled(
                "╱".repeat(right_width.saturating_sub(row)),
                Style::default().fg(CHARPLE),
            )))
            .collect::<Vec<_>>();
            Line::from(spans)
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), area);
}

pub(super) fn render_small(frame: &mut Frame, area: Rect) {
    let text_width = "Charm™ Crush".chars().count();
    let diagonal_count = usize::from(area.width).saturating_sub(text_width + 1);
    let mut spans = vec![Span::styled("Charm™ ", Style::default().fg(DOLLY))];
    spans.extend("Crush".chars().enumerate().map(|(index, character)| {
        let t = index as f32 / 4.0;
        Span::styled(
            character.to_string(),
            Style::default()
                .fg(lerp(DOLLY, CHARPLE, t))
                .add_modifier(Modifier::BOLD),
        )
    }));
    if diagonal_count > 0 {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            "╱".repeat(diagonal_count),
            Style::default().fg(CHARPLE),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn metadata(width: usize) -> Line<'static> {
    let charm = "Charm™";
    let gap = width.saturating_sub(charm.chars().count() + VERSION.len());
    Line::from(vec![
        Span::styled(charm, Style::default().fg(DOLLY)),
        Span::raw(" ".repeat(gap)),
        Span::styled(VERSION, Style::default().fg(CHARPLE)),
    ])
}

fn gradient_art(rows: &[&str]) -> Vec<Line<'static>> {
    let width = rows.first().map_or(1, |row| row.chars().count());
    rows.iter()
        .map(|row| {
            Line::from(
                row.chars()
                    .enumerate()
                    .map(|(column, character)| {
                        if character == ' ' {
                            Span::raw(" ")
                        } else {
                            let t = column as f32 / width.saturating_sub(1).max(1) as f32;
                            Span::styled(
                                character.to_string(),
                                Style::default()
                                    .fg(lerp(DOLLY, CHARPLE, t))
                                    .add_modifier(Modifier::BOLD),
                            )
                        }
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect()
}

fn lerp(from: Color, to: Color, t: f32) -> Color {
    let (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) = (from, to) else {
        return from;
    };
    let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
    Color::Rgb(mix(r1, r2), mix(g1, g2), mix(b1, b2))
}
