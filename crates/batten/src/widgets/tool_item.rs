//! Widget para una sola herramienta en el transcript.

use crate::state::{StyleToken, ToolItem, ToolStatus};
use crate::widgets::{Anim, Theme};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

fn status_token(status: ToolStatus) -> StyleToken {
    match status {
        ToolStatus::Pending => StyleToken::ToolPending,
        ToolStatus::Running => StyleToken::ToolRunning,
        ToolStatus::Success => StyleToken::ToolSuccess,
        ToolStatus::Error => StyleToken::ToolError,
        ToolStatus::Canceled => StyleToken::ToolCanceled,
        ToolStatus::AwaitingPermission => StyleToken::Warning,
    }
}

pub fn render(frame: &mut Frame, area: Rect, item: &ToolItem, anim: &Anim, theme: &Theme) {
    let status = item.status();
    let glyph = match status {
        ToolStatus::Success => theme.glyphs.tool_success,
        ToolStatus::Error => theme.glyphs.tool_error,
        _ => theme.glyphs.tool_pending,
    };
    let style = theme.style(status_token(status));

    // Cabecera: icono + nombre + anim si está girando
    let mut header = vec![
        Span::styled(format!("{glyph} "), style),
        Span::styled(format!("{} ", item.name), theme.style(StyleToken::ToolName)),
    ];
    if item.is_spinning() {
        let spin = anim.render("…", theme);
        header.extend(spin.spans);
    }

    let mut lines = vec![Line::from(header)];

    // Cuerpo por estado
    let body = match status {
        ToolStatus::AwaitingPermission => Some("Requesting permission…"),
        ToolStatus::Running => Some("Waiting for tool response…"),
        ToolStatus::Canceled => Some("Canceled."),
        _ if item.expanded => Some("(expanded output)"),
        _ => None,
    };
    if let Some(text) = body {
        lines.push(Line::from(Span::styled(format!("  {text}"), style)));
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ToolItem;
    use crate::widgets::{Anim, Theme};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn buffer(item: &ToolItem, w: u16, h: u16) -> String {
        let backend = TestBackend::new(w, h);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f, f.area(), item, &Anim { frame: 0 }, &Theme::test()))
            .unwrap();
        term.backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn shows_name_and_success_glyph() {
        let mut t = ToolItem::new("c1", "view");
        t.start();
        t.set_result(false);
        let out = buffer(&t, 40, 3);
        assert!(out.contains("view"));
        assert!(out.contains('✓'));
    }

    #[test]
    fn error_uses_the_error_glyph() {
        let mut t = ToolItem::new("c1", "bash");
        t.set_result(true);
        assert!(buffer(&t, 40, 3).contains('×'));
    }

    #[test]
    fn awaiting_permission_shows_its_message() {
        let mut t = ToolItem::new("c1", "bash");
        t.await_permission();
        assert!(buffer(&t, 40, 3).contains("permission"));
    }

    #[test]
    fn zero_size_does_not_panic() {
        let t = ToolItem::new("c1", "view");
        let _ = buffer(&t, 0, 0);
    }
}
