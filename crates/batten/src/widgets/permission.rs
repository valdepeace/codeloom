//! Diálogo de permisos: panel centrado con cuerpo (texto o diff), botones y ayuda.

use super::button::button_row;
use super::diff;
use super::key_help::key_help;
use super::panel::{render as render_panel, Panel};
use super::theme::Theme;
use crate::state::{PermissionBody, PermissionDialog, Tone};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Span;
use ratatui::widgets::{Clear, Paragraph, Wrap};
use ratatui::Frame;

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

/// Rectángulo del diálogo: 80 % del ancho (máx. 100) y 80 % del alto
/// (mín. 8 filas), centrado y nunca mayor que la pantalla.
pub fn area(screen: Rect) -> Rect {
    let w = ((screen.width as u32 * 8 / 10) as u16).min(100);
    let h = ((screen.height as u32 * 8 / 10) as u16)
        .max(8)
        .min(screen.height);
    Rect {
        x: screen.x + (screen.width - w) / 2,
        y: screen.y + (screen.height - h) / 2,
        width: w,
        height: h,
    }
}

/// Pinta el diálogo sobre `screen` (limpia su área antes).
pub fn render(frame: &mut Frame, screen: Rect, dialog: &PermissionDialog, theme: &Theme) {
    let outer = area(screen);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Diff;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn area_is_centered_and_bounded() {
        assert_eq!(area(Rect::new(0, 0, 120, 40)), Rect::new(12, 4, 96, 32));
        assert_eq!(area(Rect::new(0, 0, 200, 50)), Rect::new(50, 5, 100, 40));
        assert_eq!(area(Rect::new(0, 0, 20, 5)), Rect::new(2, 0, 16, 5));
        assert_eq!(area(Rect::new(0, 0, 0, 0)), Rect::new(0, 0, 0, 0));
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
