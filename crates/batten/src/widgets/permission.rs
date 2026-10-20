//! Diálogo de permisos: panel centrado con cuerpo (texto o diff), botones y ayuda.

use super::button::button_row;
use super::diff;
use super::key_help::key_help;
use super::panel::{render as render_panel, Panel};
use super::theme::Theme;
use crate::state::{PermissionBody, PermissionDialog, Tone};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph, Wrap};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

const BUTTONS: [(&str, Tone); 3] = [
    ("Permitir", Tone::Success),
    ("Permitir en la sesión", Tone::Info),
    ("Denegar", Tone::Error),
];

const HELP: [(&str, &str); 5] = [
    ("←→", "elegir"),
    ("enter", "confirmar"),
    ("a/s/d", "atajo"),
    ("↑↓", "scroll"),
    ("esc", "denegar"),
];

/// Rectángulo compacto, ampliado según el contenido y limitado a la pantalla.
pub fn area(screen: Rect, dialog: &PermissionDialog) -> Rect {
    let body_width = match &dialog.body {
        PermissionBody::Text(text) => text.lines().map(UnicodeWidthStr::width).max().unwrap_or(0),
        PermissionBody::Diff(diff) => diff
            .lines()
            .iter()
            .map(|line| line.text.width())
            .max()
            .unwrap_or(0),
    };
    let wanted_width = 52usize
        .max(body_width + 4)
        .max(dialog.subtitle.width() + 4)
        .max(dialog.tool.width() + 12)
        .min(80) as u16;
    let max_width = screen.width.saturating_sub(4).min(screen.width);
    let w = wanted_width.min(max_width);
    let inner_width = usize::from(w.saturating_sub(2)).max(1);
    let body_lines = match &dialog.body {
        PermissionBody::Text(text) => text
            .split('\n')
            .map(|line| line.width().max(1).div_ceil(inner_width))
            .sum::<usize>(),
        PermissionBody::Diff(diff) => diff
            .lines()
            .iter()
            .map(|line| line.text.width().max(1).div_ceil(inner_width))
            .sum::<usize>(),
    }
    .max(1)
    .min(usize::from(u16::MAX)) as u16;
    let wanted_height = 5 + u16::from(!dialog.subtitle.is_empty()) + body_lines;
    let max_height = ((u32::from(screen.height) * 8 / 10) as u16)
        .max(6)
        .min(screen.height);
    let h = wanted_height.min(max_height);
    Rect {
        x: screen.x + (screen.width - w) / 2,
        y: screen.y + (screen.height - h) / 2,
        width: w,
        height: h,
    }
}

/// Pinta el diálogo sobre `screen` (limpia su área antes).
pub fn render(frame: &mut Frame, screen: Rect, dialog: &PermissionDialog, theme: &Theme) {
    let outer = area(screen, dialog);
    if outer.width < 2 || outer.height < 2 {
        return;
    }
    frame.render_widget(Clear, outer);
    let panel = Panel {
        title: "Permiso",
        info: &dialog.tool,
        tone: Tone::Warning,
    };
    let inner = render_panel(frame, outer, &panel, theme);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let sub_h = u16::from(!dialog.subtitle.is_empty());
    let [sub, body, _gap, buttons, help] = Layout::vertical([
        Constraint::Length(sub_h),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);
    if sub_h == 1 {
        let line = Span::styled(dialog.subtitle.clone(), theme.styles.muted);
        frame.render_widget(Paragraph::new(line), sub);
    }
    match &dialog.body {
        PermissionBody::Text(t) => {
            let y = u16::try_from(dialog.scroll()).unwrap_or(u16::MAX);
            let p = Paragraph::new(t.clone())
                .wrap(Wrap { trim: false })
                .scroll((y, 0));
            frame.render_widget(p, body);
        }
        PermissionBody::Diff(d) => diff::render(frame, body, d, dialog.scroll(), theme),
    }
    let row = button_row(&BUTTONS, dialog.choice().index(), theme);
    frame.render_widget(Paragraph::new(row), buttons);
    frame.render_widget(Paragraph::new(key_help(&HELP, help.width, theme)), help);
}

/// Pinta el diálogo de aprobación de Pawan: caja elevada de 66 columnas,
/// argumentos en bloque y las teclas Y/N/A dentro de la última fila.
pub fn render_pawan(frame: &mut Frame, screen: Rect, tool: &str, arguments: &str) -> Rect {
    if screen.width < 40 || screen.height < 8 {
        return Rect::default();
    }
    let width = 66.min(screen.width.saturating_sub(4)).min(screen.width);
    let inner_width = width.saturating_sub(6).max(8) as usize;
    let chars: Vec<char> = arguments.trim().chars().collect();
    let arg_lines: Vec<String> = chars
        .chunks(inner_width)
        .take(3)
        .map(|row| row.iter().collect())
        .collect();
    let mut text = vec![Line::from(vec![
        Span::styled("tool  ", Style::default().fg(Color::Rgb(136, 136, 136))),
        Span::styled(
            tool,
            Style::default()
                .fg(Color::Rgb(0, 204, 204))
                .add_modifier(Modifier::BOLD),
        ),
    ])];
    text.push(Line::from(""));
    text.push(Line::styled(
        "arguments",
        Style::default().fg(Color::Rgb(136, 136, 136)),
    ));
    if arg_lines.is_empty() {
        text.push(Line::styled(
            "(none)",
            Style::default()
                .fg(Color::Rgb(42, 42, 42))
                .add_modifier(Modifier::ITALIC),
        ));
    } else {
        text.extend(
            arg_lines
                .iter()
                .map(|line| Line::styled(line.clone(), Style::default().fg(Color::White))),
        );
        if chars.len() > inner_width * 3 {
            if let Some(last) = text.last_mut() {
                let mut truncated = last.spans[0].content.to_string();
                truncated.pop();
                truncated.push('…');
                *last = Line::styled(truncated, Style::default().fg(Color::White));
            }
        }
    }
    text.push(Line::from(""));
    let key = |label: &str, color: Color| {
        Span::styled(
            format!(" {label} "),
            Style::default()
                .fg(Color::Rgb(0, 0, 0))
                .bg(color)
                .add_modifier(Modifier::BOLD),
        )
    };
    text.push(Line::from(vec![
        key("Y", Color::Rgb(80, 250, 123)),
        Span::styled(" Allow   ", Style::default().fg(Color::White)),
        key("N", Color::Rgb(255, 85, 85)),
        Span::styled(" Deny   ", Style::default().fg(Color::White)),
        key("A", Color::Rgb(0, 204, 204)),
        Span::styled(" Allow all", Style::default().fg(Color::White)),
    ]));

    let height = (text.len() as u16 + 4).min(screen.height.saturating_sub(2));
    if height < 4 {
        return Rect::default();
    }
    let popup = Rect::new(
        screen.x + screen.width.saturating_sub(width) / 2,
        screen.y + screen.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let shadow = Rect::new(
        popup.x.saturating_add(1),
        popup.y.saturating_add(1),
        width,
        height,
    );
    if shadow.right() <= screen.right() && shadow.bottom() <= screen.bottom() {
        frame.render_widget(Clear, shadow);
        frame.render_widget(
            Block::default().style(Style::default().bg(Color::Rgb(0, 0, 0))),
            shadow,
        );
    }
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(Span::styled(
            " ⚠  Permission Required ",
            Style::default()
                .fg(Color::Rgb(255, 184, 108))
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(0, 204, 204)))
        .padding(Padding::new(2, 2, 1, 1))
        .style(Style::default().bg(Color::Rgb(42, 42, 42)));
    frame.render_widget(Paragraph::new(text).block(block), popup);
    popup
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Diff;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn area_is_centered_and_bounded() {
        let d = PermissionDialog::new("bash", "", PermissionBody::Text("ls -la".into()));
        assert_eq!(area(Rect::new(0, 0, 120, 40), &d), Rect::new(34, 17, 52, 6));
        assert_eq!(area(Rect::new(0, 0, 200, 50), &d), Rect::new(74, 22, 52, 6));
        assert_eq!(area(Rect::new(0, 0, 20, 5), &d), Rect::new(2, 0, 16, 5));
        assert_eq!(area(Rect::new(0, 0, 0, 0), &d), Rect::new(0, 0, 0, 0));
    }

    #[test]
    fn area_grows_for_long_content_but_stays_bounded() {
        let d = PermissionDialog::new(
            "bash",
            "",
            PermissionBody::Text((0..30).map(|_| "x").collect::<Vec<_>>().join("\n")),
        );
        assert_eq!(area(Rect::new(0, 0, 120, 40), &d), Rect::new(34, 4, 52, 32));
    }

    fn screen_text(dialog: &PermissionDialog, w: u16, h: u16) -> String {
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
    fn shows_title_body_and_buttons() {
        let d = PermissionDialog::new(
            "edit",
            "src/main.rs",
            PermissionBody::Diff(Diff::new("a\n", "b\n", 3)),
        );
        let s = screen_text(&d, 120, 40);
        for needle in [
            "Permiso",
            "edit",
            "src/main.rs",
            "@@ -1,1 +1,1 @@",
            "Permitir",
            "Denegar",
        ] {
            assert!(s.contains(needle), "falta {needle:?}");
        }
    }

    #[test]
    fn small_screens_do_not_panic() {
        let d = PermissionDialog::new("bash", "", PermissionBody::Text("ls -la".into()));
        for (w, h) in [(0, 0), (1, 1), (20, 5), (120, 40)] {
            screen_text(&d, w, h);
        }
    }
}
