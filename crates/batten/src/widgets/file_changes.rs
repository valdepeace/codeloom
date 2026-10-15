//! Sección de ficheros modificados: título + `+A`/`-R` por fichero.

use crate::state::{FileChanges, StyleToken};
use crate::widgets::text::truncate;
use crate::widgets::Theme;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub fn render(frame: &mut Frame, area: Rect, title: &str, changes: &FileChanges, theme: &Theme) {
    let (add, rem) = changes.totals();
    let mut lines = vec![Line::from(vec![
        Span::styled(title.to_string(), theme.style(StyleToken::Heading)),
        Span::raw(" "),
        Span::styled(format!("+{add}"), theme.style(StyleToken::Success)),
        Span::raw(" "),
        Span::styled(format!("-{rem}"), theme.style(StyleToken::Error)),
    ])];

    let width = area.width as usize;
    let room = area.height.saturating_sub(1) as usize;
    for c in changes.iter().take(room) {
        let counts = format!(" +{} -{}", c.added, c.removed);
        let used = counts.chars().count();
        lines.push(Line::from(vec![
            Span::styled(
                truncate(&c.path, width.saturating_sub(used)),
                theme.style(StyleToken::Muted),
            ),
            Span::styled(format!(" +{}", c.added), theme.style(StyleToken::Success)),
            Span::styled(format!(" -{}", c.removed), theme.style(StyleToken::Error)),
        ]));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{FileChange, FileChanges};
    use crate::widgets::Theme;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn buffer(c: &FileChanges, w: u16, h: u16) -> String {
        let backend = TestBackend::new(w, h);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f, f.area(), "ficheros", c, &Theme::test()))
            .unwrap();
        term.backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    fn sample() -> FileChanges {
        let mut c = FileChanges::new();
        c.upsert(FileChange {
            path: "src/main.rs".into(),
            added: 2,
            removed: 1,
        });
        c
    }

    #[test]
    fn shows_title_paths_and_counts() {
        let out = buffer(&sample(), 40, 6);
        assert!(out.contains("ficheros"));
        assert!(out.contains("src/main.rs"));
        assert!(out.contains("+2"));
        assert!(out.contains("-1"));
    }

    #[test]
    fn truncates_long_paths() {
        let mut c = FileChanges::new();
        c.upsert(FileChange {
            path: "a/very/long/path/that/will/not/fit/at/all/file.rs".into(),
            added: 1,
            removed: 0,
        });
        let out = buffer(&c, 20, 4);
        assert!(out.contains('…'));
    }

    #[test]
    fn zero_size_does_not_panic() {
        let _ = buffer(&sample(), 0, 0);
        let _ = buffer(&FileChanges::new(), 0, 0);
    }
}
