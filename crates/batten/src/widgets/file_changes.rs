//! Sección de ficheros modificados: título + `+A`/`-R` por fichero.

use crate::state::path_text::{split_path, truncate_path};
use crate::state::{FileChanges, StyleToken};
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
    let visible = if changes.len() > room {
        room.saturating_sub(1)
    } else {
        room
    };
    for c in changes.iter().take(visible) {
        let counts = format!(" +{} -{}", c.added, c.removed);
        let used = counts.chars().count();
        // El directorio queda apagado y el nombre resaltado; si no cabe, se recorta por la
        // izquierda para que el nombre del fichero no se pierda.
        let path = truncate_path(&c.path, width.saturating_sub(used));
        let (dir, name) = split_path(&path);
        let mut spans = Vec::with_capacity(4);
        if !dir.is_empty() {
            spans.push(Span::styled(
                dir.to_string(),
                theme.style(StyleToken::Muted),
            ));
        }
        spans.push(Span::styled(
            name.to_string(),
            theme.style(StyleToken::ToolPath),
        ));
        spans.push(Span::styled(
            format!(" +{}", c.added),
            theme.style(StyleToken::Success),
        ));
        spans.push(Span::styled(
            format!(" -{}", c.removed),
            theme.style(StyleToken::Error),
        ));
        lines.push(Line::from(spans));
    }
    let hidden = changes.len().saturating_sub(visible);
    if hidden > 0 {
        lines.push(Line::from(Span::styled(
            format!("… +{hidden} more files"),
            theme.style(StyleToken::Muted),
        )));
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
    fn reports_files_hidden_by_short_sidebar() {
        let mut c = FileChanges::new();
        for index in 0..5 {
            c.upsert(FileChange {
                path: format!("src/{index}.rs"),
                added: 1,
                removed: 0,
            });
        }
        let out = buffer(&c, 40, 3);
        assert!(out.contains("src/0.rs"));
        assert!(out.contains("+4 more files"));
    }

    #[test]
    fn zero_size_does_not_panic() {
        let _ = buffer(&sample(), 0, 0);
        let _ = buffer(&FileChanges::new(), 0, 0);
    }

    #[test]
    fn the_name_is_highlighted_and_the_directory_dimmed() {
        let theme = Theme::test();
        let mut c = FileChanges::new();
        c.upsert(FileChange {
            path: "crates/batten/src/lib.rs".into(),
            added: 1,
            removed: 0,
        });
        let mut term = Terminal::new(TestBackend::new(40, 4)).unwrap();
        term.draw(|f| render(f, f.area(), "ficheros", &c, &theme))
            .unwrap();
        let buf = term.backend().buffer();
        // Fila 1: «crates/batten/src/» apagado y «lib.rs» resaltado.
        let dir = &buf[(0, 1)];
        let nombre = &buf[("crates/batten/src/".len() as u16, 1)];

        assert_eq!(nombre.symbol(), "l");
        assert_eq!(Some(dir.fg), theme.style(StyleToken::Muted).fg);
        assert_eq!(Some(nombre.fg), theme.style(StyleToken::ToolPath).fg);
        assert_ne!(dir.fg, nombre.fg, "directorio y nombre deben distinguirse");
    }

    #[test]
    fn a_long_path_keeps_its_file_name_when_it_does_not_fit() {
        let mut c = FileChanges::new();
        c.upsert(FileChange {
            path: "a/very/long/path/that/will/not/fit/at/all/render.rs".into(),
            added: 12,
            removed: 3,
        });
        let out = buffer(&c, 28, 4);
        assert!(out.contains("render.rs"), "{out}");
        assert!(out.contains('…'), "{out}");
        assert!(out.contains("+12") && out.contains("-3"), "{out}");
    }
}
