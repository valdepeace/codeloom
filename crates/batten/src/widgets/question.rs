//! Diálogo de preguntas: ventana centrada con la pregunta, sus opciones
//! y una fila «Other…» para respuesta libre.

use super::theme::Theme;
use crate::state::{QuestionDialog, QuestionSpec};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

const MAX_WIDTH: u16 = 72;
const OTHER_LABEL: &str = "Other…";
const CARET: &str = "▏";
const FOOTER_MOVE: &str = "↑↓ move · space select · enter confirm · ←→ question · esc cancel";
const FOOTER_EDIT: &str = "type your answer · enter done · esc back";

/// Filas que ocupa `line` al envolverse en `inner` columnas.
fn rows_of(line: &Line, inner: usize) -> usize {
    line.width().max(1).div_ceil(inner.max(1))
}

/// Marcador de una fila: `( )`/`(•)` en única, `[ ]`/`[x]` en múltiple.
fn marker(multi: bool, on: bool) -> &'static str {
    match (multi, on) {
        (false, false) => "( )",
        (false, true) => "(•)",
        (true, false) => "[ ]",
        (true, true) => "[x]",
    }
}

/// Líneas del cuerpo: pregunta, línea en blanco, opciones y «Other…».
fn body_lines(dialog: &QuestionDialog, theme: &Theme) -> Vec<Line<'static>> {
    let spec: &QuestionSpec = dialog.spec();
    let s = &theme.styles;
    let mut lines = vec![Line::from(spec.question.clone()), Line::from("")];
    let cursor = dialog.cursor();
    for (i, (label, description)) in spec.options.iter().enumerate() {
        let focused = cursor == i;
        let style = if focused { s.accent } else { s.default };
        let prefix = if focused { "› " } else { "  " };
        let on = dialog.is_selected(i);
        lines.push(Line::from(vec![
            Span::styled(prefix, style),
            Span::styled(format!("{} ", marker(spec.multi, on)), style),
            Span::styled(label.clone(), style.add_modifier(Modifier::BOLD)),
        ]));
        if !description.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("    {description}"),
                s.muted,
            )));
        }
    }
    let focused = cursor == spec.options.len();
    let style = if focused { s.accent } else { s.default };
    let prefix = if focused { "› " } else { "  " };
    let text = dialog.other_text();
    let has_text = !text.trim().is_empty();
    let answered = has_text || dialog.editing_other();
    let mut other = vec![
        Span::styled(prefix, style),
        Span::styled(format!("{} ", marker(spec.multi, has_text)), style),
    ];
    if answered {
        other.push(Span::styled(text.to_string(), style));
        if dialog.editing_other() {
            other.push(Span::styled(CARET, style));
        }
    } else {
        other.push(Span::styled(OTHER_LABEL, style));
    }
    lines.push(Line::from(other));
    lines
}

/// Título del marco: ` Question i/n · header `; sin ` · ` si el header está vacío.
fn title(dialog: &QuestionDialog) -> String {
    let header = dialog.spec().header.trim();
    let base = format!(" Question {}/{}", dialog.current() + 1, dialog.len());
    if header.is_empty() {
        format!("{base} ")
    } else {
        format!("{base} · {header} ")
    }
}

/// Rectángulo centrado, de ancho `min(72, screen.width - 4)` y alto según el
/// contenido, acotado a la pantalla.
pub fn area(screen: Rect, dialog: &QuestionDialog, theme: &Theme) -> Rect {
    let w = MAX_WIDTH.min(screen.width.saturating_sub(4));
    let inner = usize::from(w.saturating_sub(2));
    let body_rows: usize = body_lines(dialog, theme)
        .iter()
        .map(|line| rows_of(line, inner))
        .sum();
    // Dos bordes más la fila de pie.
    let wanted = (body_rows + 3).min(usize::from(u16::MAX)) as u16;
    let h = wanted.min(screen.height);
    Rect {
        x: screen.x + screen.width.saturating_sub(w) / 2,
        y: screen.y + screen.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}

/// Pinta el diálogo sobre `screen` (limpia su área antes).
pub fn render(frame: &mut Frame, screen: Rect, dialog: &QuestionDialog, theme: &Theme) {
    let outer = area(screen, dialog, theme);
    if outer.width < 3 || outer.height < 3 {
        return;
    }
    frame.render_widget(Clear, outer);
    let block = Block::default()
        .title(Span::styled(title(dialog), theme.styles.heading))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.styles.accent);
    let inner = block.inner(outer);
    frame.render_widget(block, outer);
    if inner.width == 0 || inner.height < 2 {
        return;
    }
    let [body, footer] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(inner);
    let text = if dialog.editing_other() {
        FOOTER_EDIT
    } else {
        FOOTER_MOVE
    };
    frame.render_widget(Paragraph::new(text).style(theme.styles.muted), footer);
    let paragraph = Paragraph::new(body_lines(dialog, theme)).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, body);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{QuestionKey, QuestionSpec};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn spec(multi: bool) -> QuestionSpec {
        QuestionSpec {
            header: "Stack".into(),
            question: "Which framework?".into(),
            options: vec![
                ("Alpha".into(), "the first one".into()),
                ("Beta".into(), String::new()),
            ],
            multi,
        }
    }

    fn dialog(multi: bool) -> QuestionDialog {
        QuestionDialog::new(vec![spec(multi), spec(multi)])
    }

    fn screen_text(dialog: &QuestionDialog, w: u16, h: u16) -> String {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| render(f, f.area(), dialog, &Theme::test()))
            .unwrap();
        term.backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn shows_question_options_other_and_footer() {
        let d = dialog(false);
        let s = screen_text(&d, 120, 40);
        for needle in [
            "Question 1/2",
            "Stack",
            "Which framework?",
            "Alpha",
            "the first one",
            "Beta",
            "Other",
            "move",
        ] {
            assert!(s.contains(needle), "falta {needle:?}");
        }
    }

    #[test]
    fn single_choice_marks_the_selected_option() {
        let mut d = dialog(false);
        d.handle(QuestionKey::Char(' '));
        let s = screen_text(&d, 120, 40);
        assert!(s.contains("(•)"), "falta marcador de selección");
        assert!(s.contains("( )"), "falta marcador vacío");
    }

    #[test]
    fn multi_choice_uses_checkboxes() {
        let mut d = dialog(true);
        d.handle(QuestionKey::Char(' '));
        let s = screen_text(&d, 120, 40);
        assert!(s.contains("[x]"), "falta casilla marcada");
        assert!(s.contains("[ ]"), "falta casilla vacía");
    }

    #[test]
    fn editing_other_shows_typed_text_and_editing_footer() {
        let mut d = dialog(false);
        d.handle(QuestionKey::Up);
        d.handle(QuestionKey::Enter);
        d.handle(QuestionKey::Char('h'));
        d.handle(QuestionKey::Char('i'));
        let s = screen_text(&d, 120, 40);
        assert!(s.contains("hi▏"), "falta el texto con cursor");
        assert!(s.contains("type your answer"), "falta el pie de edición");
    }

    #[test]
    fn small_screens_do_not_panic() {
        let d = dialog(true);
        for (w, h) in [(1, 1), (10, 3), (40, 8), (120, 40)] {
            screen_text(&d, w, h);
        }
    }
}
