//! Panel con borde redondeado y título `╭ Título ╱╱╱╱ info ╮`.

use super::gradient::{color_of, fg, lerp_color};
use super::text::truncate;
use super::theme::Theme;
use super::tone::tone_style;
use crate::state::Tone;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

/// Contenido de la cabecera de un panel.
#[derive(Debug, Clone, Copy, Default)]
pub struct Panel<'a> {
    pub title: &'a str,
    pub info: &'a str,
    pub tone: Tone,
}

/// Pinta el marco y la cabecera; devuelve el área interior. Con menos de
/// 2×2 no pinta nada y devuelve `Rect::default()`.
pub fn render(frame: &mut Frame, area: Rect, panel: &Panel, theme: &Theme) -> Rect {
    if area.width < 2 || area.height < 2 {
        return Rect::default();
    }
    let border = if panel.tone == Tone::Neutral {
        theme.styles.border
    } else {
        tone_style(panel.tone, theme)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let top = Rect {
        x: area.x + 1,
        y: area.y,
        width: area.width - 2,
        height: 1,
    };
    frame.render_widget(
        Paragraph::new(header(panel, top.width as usize, theme)),
        top,
    );
    inner
}

/// Cabecera entre esquinas: se descarta primero `info`, luego el relleno, y
/// por último se trunca el título.
fn header(panel: &Panel, w: usize, theme: &Theme) -> Line<'static> {
    if w == 0 {
        return Line::default();
    }
    let s = &theme.styles;
    let title = format!(" {} ", panel.title);
    let tw = title.width();
    if tw >= w {
        return Line::from(Span::styled(truncate(&title, w), s.heading));
    }
    let info = if panel.info.is_empty() {
        String::new()
    } else {
        format!(" {} ", panel.info)
    };
    let iw = if tw + info.width() < w {
        info.width()
    } else {
        0
    };
    let fill = w - tw - iw;
    let from = color_of(s.accent_from);
    let to = color_of(s.accent_to);
    let mut spans = vec![Span::styled(title, s.heading)];
    for i in 0..fill {
        let t = if fill > 1 {
            i as f32 / (fill - 1) as f32
        } else {
            0.0
        };
        spans.push(Span::styled(
            theme.glyphs.field.to_string(),
            fg(theme, lerp_color(from, to, t)),
        ));
    }
    if iw > 0 {
        spans.push(Span::styled(info, s.muted));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;
    use ratatui::Terminal;

    fn draw(w: u16, h: u16, panel: Panel) -> (Terminal<TestBackend>, Rect) {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        let mut inner = Rect::default();
        term.draw(|f| inner = render(f, f.area(), &panel, &Theme::test()))
            .unwrap();
        (term, inner)
    }

    fn row(term: &Terminal<TestBackend>, y: u16) -> String {
        let buf = term.backend().buffer();
        (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect()
    }

    const P: Panel = Panel {
        title: "Permisos",
        info: "bash",
        tone: Tone::Neutral,
    };

    #[test]
    fn header_has_title_fill_and_info() {
        let (term, inner) = draw(30, 4, P);
        assert_eq!(
            row(&term, 0),
            format!("╭ Permisos {} bash ╮", "╱".repeat(12))
        );
        assert!(row(&term, 3).starts_with('╰') && row(&term, 3).ends_with('╯'));
        assert_eq!(inner, Rect::new(1, 1, 28, 2));
    }

    #[test]
    fn narrow_drops_info_and_fill() {
        let (term, _) = draw(12, 3, P);
        assert_eq!(row(&term, 0), "╭ Permisos ╮");
    }

    #[test]
    fn tone_colors_the_border() {
        let (term, _) = draw(
            20,
            3,
            Panel {
                tone: Tone::Error,
                ..P
            },
        );
        assert_eq!(term.backend().buffer()[(0, 1)].fg, Color::Red);
    }

    #[test]
    fn wide_title_keeps_frame_width() {
        // Las celdas de continuación de un carácter ancho no son fiables como
        // texto; basta con que las esquinas sigan en su sitio.
        let (term, _) = draw(
            14,
            3,
            Panel {
                title: "日本語",
                info: "",
                tone: Tone::Neutral,
            },
        );
        let top = row(&term, 0);
        assert!(top.starts_with('╭') && top.ends_with('╮'));
        assert!(top.contains('日'));
    }

    #[test]
    fn tiny_area_draws_nothing() {
        for (w, h) in [(0, 0), (1, 5), (5, 1)] {
            let (_, inner) = draw(w, h, P);
            assert_eq!(inner, Rect::default());
        }
    }
}
