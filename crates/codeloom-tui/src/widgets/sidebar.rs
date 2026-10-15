use crate::app::App;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

/// Barra lateral derecha al estilo Crush: el wordmark fijo arriba y, debajo,
/// la informacion de la sesion (estado, cwd, modelo) mas los huecos de las
/// secciones que llegarán en m2.
const WORDMARK: [&str; 5] = [
    "█    ▄▀▀▄ ▄▀▀▄ █  █",
    "█    █  █ █  █ ████",
    "█    █  █ █  █ █  █",
    "█    █  █ █  █ █  █",
    "▀▀▀  ▀▄▄▀ ▀▄▄▀ █  █",
];

const GRAD_FROM: (u8, u8, u8) = (0, 212, 255); // cyan
const GRAD_TO: (u8, u8, u8) = (124, 92, 255); // violeta

fn lerp(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round() as u8
}

fn grad_line(text: &str) -> Line<'static> {
    let n = text.chars().count().max(1);
    let spans: Vec<Span> = text
        .chars()
        .enumerate()
        .map(|(i, c)| {
            let t = if n <= 1 {
                0.0
            } else {
                i as f32 / (n - 1) as f32
            };
            let color = Color::Rgb(
                lerp(GRAD_FROM.0, GRAD_TO.0, t),
                lerp(GRAD_FROM.1, GRAD_TO.1, t),
                lerp(GRAD_FROM.2, GRAD_TO.2, t),
            );
            Span::styled(c.to_string(), Style::default().fg(color))
        })
        .collect();
    Line::from(spans)
}

fn section(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        title.to_string(),
        Style::default()
            .fg(Color::Indexed(244))
            .add_modifier(Modifier::BOLD),
    ))
}

fn field_line(width: usize) -> Line<'static> {
    Line::from(Span::styled(
        "╱".repeat(width),
        Style::default().fg(Color::Rgb(60, 70, 110)),
    ))
}

/// Altura minima para el wordmark en bloques; por debajo se usa el compacto
/// de una linea (igual que hace Crush con su `SmallRender`).
const LOGO_MIN_HEIGHT: usize = 24;

fn compact_logo(width: usize) -> Vec<Line<'static>> {
    let label = Span::styled(
        "CodeLoom",
        Style::default()
            .fg(Color::Indexed(250))
            .add_modifier(Modifier::BOLD),
    );
    let remaining = width.saturating_sub("CodeLoom".len() + 1);
    let mut spans = vec![label];
    if remaining > 0 {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            "╱".repeat(remaining),
            Style::default().fg(Color::Rgb(60, 70, 110)),
        ));
    }
    vec![Line::from(spans), Line::from("")]
}

pub fn lines(app: &App, width: usize, height: usize) -> Vec<Line<'static>> {
    let mut out = Vec::new();

    if height < LOGO_MIN_HEIGHT {
        out.extend(compact_logo(width));
    } else {
        // Logo: dos campos de diagonales arriba, el wordmark y un campo abajo,
        // como el compacto de Crush.
        out.push(field_line(width));
        out.push(field_line(width));
        for row in WORDMARK {
            out.push(grad_line(row));
        }
        out.push(field_line(width));
        out.push(Line::from(Span::styled(
            "CodeLoom",
            Style::default()
                .fg(Color::Indexed(250))
                .add_modifier(Modifier::BOLD),
        )));
        out.push(Line::from(""));
    }

    out.push(section("estado"));
    out.push(Line::from(format!("  {}", app.status)));
    out.push(Line::from(""));
    out.push(section("cwd"));
    out.push(Line::from(format!(
        "  {}",
        truncate(&app.cwd, width.saturating_sub(3))
    )));
    out.push(Line::from(""));
    out.push(section("modelo"));
    out.push(Line::from(format!("  {}", app.model)));
    out.push(Line::from(""));

    out.push(section("agente"));
    out.push(Line::from(Span::styled(
        "  steps    —",
        Style::default().fg(Color::Indexed(240)),
    )));
    out.push(Line::from(""));
    out.push(section("subagentes / jobs"));
    out.push(Line::from(Span::styled(
        "  (m2)",
        Style::default().fg(Color::Indexed(240)),
    )));

    out
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let kept: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{kept}…")
    }
}

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default().borders(Borders::LEFT);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let width = inner.width.saturating_sub(2) as usize;
    let content = lines(app, width, inner.height as usize);
    frame.render_widget(Paragraph::new(content).wrap(Wrap { trim: false }), inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(app: &App, w: u16, h: u16) -> String {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let backend = TestBackend::new(w, h);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f, f.area(), app)).unwrap();
        term.backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn shows_wordmark_label_and_session_info() {
        let app = App::with_context("/home/andres/proyecto", "mistral-nemo");
        let content = rendered(&app, 32, 40);
        assert!(content.contains("CodeLoom"));
        assert!(content.contains("mistral-nemo"));
        assert!(content.contains("andres"));
        assert!(content.contains("subagentes"));
    }

    #[test]
    fn logo_is_the_block_wordmark() {
        let app = App::new();
        let content = rendered(&app, 32, 40);
        assert!(content.contains("█"));
        assert!(content.contains("╱"));
    }
}
