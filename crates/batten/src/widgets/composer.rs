//! Widget para el composer de entrada.

use crate::state::{Composer, ComposerLayout, StyleToken, VisualRow};
use crate::widgets::{ComposerStyle, Theme};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use ratatui::Frame;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

fn line_view(line: &str, cursor: Option<usize>, width: usize) -> (String, usize) {
    let graphemes = line.graphemes(true).collect::<Vec<_>>();
    if width == 0 {
        return (String::new(), 0);
    }
    let Some(cursor) = cursor.map(|cursor| cursor.min(graphemes.len())) else {
        let mut used = 0;
        let mut end = 0;
        for grapheme in &graphemes {
            let next = used + UnicodeWidthStr::width(*grapheme);
            if next > width {
                break;
            }
            used = next;
            end += 1;
        }
        return (graphemes[..end].concat(), 0);
    };

    // Keep the caret in view while filling from the left first, as Herdr does.
    let mut start = cursor;
    let mut cursor_col = 0;
    for grapheme in graphemes[..cursor].iter().rev() {
        let next = cursor_col + UnicodeWidthStr::width(*grapheme);
        if next >= width {
            break;
        }
        cursor_col = next;
        start -= 1;
    }
    let mut end = cursor;
    let mut used = cursor_col;
    for grapheme in &graphemes[cursor..] {
        let next = used + UnicodeWidthStr::width(*grapheme);
        if next > width {
            break;
        }
        used = next;
        end += 1;
    }
    (graphemes[start..end].concat(), cursor_col)
}

fn horizontal_view(
    text: &str,
    cursor_row: usize,
    cursor_index: usize,
    width: usize,
) -> (String, usize) {
    let mut graphemes_before = 0;
    let mut cursor_col = 0;
    let lines = text
        .split('\n')
        .enumerate()
        .map(|(row, line)| {
            let cursor = (row == cursor_row).then(|| cursor_index.saturating_sub(graphemes_before));
            let (view, col) = line_view(line, cursor, width);
            if row == cursor_row {
                cursor_col = col;
            }
            graphemes_before += line.graphemes(true).count() + 1;
            view
        })
        .collect::<Vec<_>>();
    (lines.join("\n"), cursor_col)
}

/// Composer con foco (borde de acento y cursor).
pub fn render(frame: &mut Frame, area: Rect, composer: &Composer, theme: &Theme) {
    render_focused(frame, area, composer, true, theme);
}

/// Composer; sin foco, borde atenuado y sin cursor.
pub fn render_focused(
    frame: &mut Frame,
    area: Rect,
    composer: &Composer,
    focused: bool,
    theme: &Theme,
) {
    render_focused_with_model(frame, area, composer, focused, "", theme);
}

/// Composer principal con el modelo activo en su cabecera.
pub fn render_focused_with_model(
    frame: &mut Frame,
    area: Rect,
    composer: &Composer,
    focused: bool,
    model: &str,
    theme: &Theme,
) {
    if area.is_empty() {
        return;
    }
    let accent = theme.style(StyleToken::Accent);
    let muted = theme.style(StyleToken::Muted);
    let border = if focused {
        accent
    } else {
        theme.style(StyleToken::Dim)
    };
    let mut title = vec![
        Span::styled("✦ ", accent.add_modifier(Modifier::BOLD)),
        Span::styled("CODELOOM", theme.style(StyleToken::Heading)),
        Span::styled("  /  PROMPT", muted),
    ];
    if !model.is_empty() {
        title.push(Span::styled(format!("   ·   {model}"), muted));
    }
    let keys = "Ctrl+J newline  ·  Enter send  ·  / commands";
    let line_count = composer.lines();
    let stats = format!(
        "{line_count} {}  ·  {} chars",
        if line_count == 1 { "line" } else { "lines" },
        composer.text().graphemes(true).count()
    );
    let fill =
        usize::from(area.width.saturating_sub(2)).saturating_sub(keys.width() + stats.width() + 2);
    let footer = Line::from(vec![
        Span::styled(format!(" {keys}"), muted),
        Span::raw(" ".repeat(fill)),
        Span::styled(stats, accent),
        Span::raw(" "),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(Line::from(title))
        .title_bottom(footer)
        .border_style(border)
        .style(theme.style(StyleToken::Default));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let (row, _) = composer.cursor_row_col();
    let scroll = row.saturating_sub(usize::from(inner.height.saturating_sub(1)));
    let (text, col) = horizontal_view(
        composer.text(),
        row,
        composer.cursor(),
        usize::from(inner.width.saturating_sub(2)),
    );
    let input = theme.style(StyleToken::Default);
    let prompt = accent.add_modifier(Modifier::BOLD);
    let lines = if composer.is_empty() {
        vec![Line::from(vec![
            Span::styled("❯ ", prompt),
            Span::styled(
                "Describe what you want to build…",
                muted.add_modifier(Modifier::ITALIC),
            ),
        ])]
    } else {
        text.split('\n')
            .enumerate()
            .map(|(line, text)| {
                Line::from(vec![
                    Span::styled(if line == 0 { "❯ " } else { "│ " }, prompt),
                    Span::styled(text.to_string(), input),
                ])
            })
            .collect()
    };
    let text = Paragraph::new(Text::from(lines))
        .style(theme.style(StyleToken::Default))
        .scroll((scroll.min(usize::from(u16::MAX)) as u16, 0));
    frame.render_widget(text, inner);

    if !focused {
        return;
    }
    let row = row.saturating_sub(scroll);
    if row < usize::from(inner.height) && col < usize::from(inner.width) {
        let x = inner.x + 2 + col as u16;
        if x < inner.right() {
            frame.set_cursor_position((x, inner.y + row as u16));
        }
    }
}

/// Prefijo visual de una fila del composer Crush: `> ` en la primera fila de
/// la primera línea lógica, `::: ` en la primera fila de las demás y sangría
/// de igual anchura en las filas de continuación.
fn crush_gutter(row: &VisualRow) -> &'static str {
    match (row.line == 0, row.first) {
        (true, true) => "> ",
        (true, false) => "  ",
        (false, true) => "::: ",
        (false, false) => "    ",
    }
}

/// Layout de filas del composer Crush para un ancho de área dado. El ancho
/// descontado depende del prefijo de cada línea lógica (`> ` o `::: `).
fn crush_layout(composer: &Composer, width: u16) -> ComposerLayout {
    crate::state::composer_layout(
        composer.text(),
        composer.cursor(),
        usize::from(width.saturating_sub(2)),
        usize::from(width.saturating_sub(4)),
    )
}

/// Altura del composer según el look activo, acotada por `max`.
pub fn height_for_look(theme: &Theme, composer: &Composer, width: u16, max: u16) -> u16 {
    let max = usize::from(max);
    let height = match theme.look.composer {
        ComposerStyle::Classic => (composer.lines() + 2).min(max),
        ComposerStyle::Crush => crush_layout(composer, width)
            .rows
            .len()
            .clamp(3, 15)
            .min(max),
    };
    height as u16
}

/// Pinta el composer con el estilo que indica el look del tema.
pub fn render_for_look(
    frame: &mut Frame,
    area: Rect,
    composer: &Composer,
    focused: bool,
    model: &str,
    theme: &Theme,
) {
    match theme.look.composer {
        ComposerStyle::Classic => {
            render_focused_with_model(frame, area, composer, focused, model, theme);
        }
        ComposerStyle::Crush => {
            render_crush(
                frame,
                area,
                composer,
                focused,
                "Describe what you want to build…",
                theme,
            );
        }
    }
}

/// Composer de Crush: textarea sin marco, prompt morado por línea y placeholder
/// integrado en el primer renglón. Hace wrap por grafemas y desplaza
/// verticalmente para mantener el cursor visible.
pub fn render_crush(
    frame: &mut Frame,
    area: Rect,
    composer: &Composer,
    focused: bool,
    placeholder: &str,
    theme: &Theme,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let prompt = theme.style(StyleToken::Accent).add_modifier(Modifier::BOLD);
    let muted = theme.style(StyleToken::Muted);
    let text_style = theme.style(StyleToken::Default);
    let height = usize::from(area.height);

    if composer.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("> ", prompt),
                Span::styled(placeholder, muted),
            ]))
            .style(theme.style(StyleToken::Default)),
            area,
        );
        if focused && area.x + 2 < area.right() {
            frame.set_cursor_position((area.x + 2, area.y));
        }
        return;
    }

    let layout = crush_layout(composer, area.width);
    let (cursor_row, cursor_col) = layout.cursor;
    let scroll = cursor_row.saturating_sub(height - 1);
    let source = composer.text();
    let lines = layout
        .rows
        .iter()
        .skip(scroll)
        .take(height)
        .map(|row| {
            Line::from(vec![
                Span::styled(crush_gutter(row), prompt),
                Span::styled(&source[row.start..row.end], text_style),
            ])
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(Text::from(lines)).style(theme.style(StyleToken::Default)),
        area,
    );

    if focused && cursor_row >= scroll && cursor_row - scroll < height {
        let gutter = layout
            .rows
            .get(cursor_row)
            .map_or(0, |row| UnicodeWidthStr::width(crush_gutter(row)));
        let x = area.x as usize + gutter + cursor_col;
        let y = area.y as usize + (cursor_row - scroll);
        if x < usize::from(area.right()) && y < usize::from(area.bottom()) {
            frame.set_cursor_position((x as u16, y as u16));
        }
    }
}

/// Composer de Martty: borde redondeado, consejo y workspace en la línea
/// superior, estado/atajos en la inferior y prompt azul dentro del área.
pub fn render_martty(
    frame: &mut Frame,
    area: Rect,
    composer: &Composer,
    focused: bool,
    theme: &Theme,
) {
    if area.width < 4 || area.height < 3 {
        return;
    }
    let muted = theme.style(StyleToken::Muted);
    let accent = theme.style(StyleToken::Accent);
    let border = if focused {
        accent
    } else {
        theme.style(StyleToken::Border)
    };
    let tip_label = " Tip";
    let tip = " · type ! to run a command in the session's persistent local shell ";
    let workspace = "· ~/project/Martty:main ";
    let cap_fill = (area.width as usize)
        .saturating_sub(tip_label.width() + tip.width() + workspace.width() + 2);
    let cap = Line::from(vec![
        Span::styled(tip_label, accent.add_modifier(Modifier::BOLD)),
        Span::styled(tip, muted),
        Span::raw("─".repeat(cap_fill)),
        Span::styled(workspace, muted),
    ]);
    let left_width = "· Standard ".width()
        + "ctrl+shift+a".width()
        + " · ".width()
        + "Full access ".width()
        + "shift+tab".width();
    let right_width = "^K Keys · ".width() + "deepseek-v4-flash".width() + " · max".width();
    let meta_fill = (area.width as usize).saturating_sub(left_width + right_width);
    let meta = Line::from(vec![
        Span::styled("· Standard ", muted),
        Span::styled("ctrl+shift+a", accent),
        Span::styled(" · ", muted),
        Span::styled("Full access ", theme.style(StyleToken::Warning)),
        Span::styled("shift+tab", accent),
        Span::raw(" ".repeat(meta_fill)),
        Span::styled("^K Keys · ", muted),
        Span::styled("deepseek-v4-flash", accent),
        Span::styled(" · max", muted),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border)
        .title(cap)
        .title_bottom(meta)
        .style(theme.style(StyleToken::Default));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = Vec::new();
    if composer.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("❯ ", accent.add_modifier(Modifier::BOLD)),
            Span::styled(
                "Describe what you want to build…",
                muted.add_modifier(Modifier::ITALIC),
            ),
        ]));
    } else {
        for (row, text) in composer.text().split('\n').enumerate() {
            lines.push(Line::from(vec![
                Span::styled(
                    if row == 0 { "❯ " } else { "  " },
                    accent.add_modifier(Modifier::BOLD),
                ),
                Span::styled(text.to_string(), theme.style(StyleToken::Default)),
            ]));
        }
    }
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .style(theme.style(StyleToken::Default))
            .wrap(Wrap { trim: false }),
        inner,
    );
    if focused && !inner.is_empty() {
        let (row, col) = composer.cursor_row_col();
        let x = inner.x + 2 + col as u16;
        let y = inner.y + row as u16;
        if x < inner.right() && y < inner.bottom() {
            frame.set_cursor_position((x, y));
        }
    }
}

/// Composer de Mindfork: campo fino con título contextual, borde gris suave
/// y prompt verde. La barra de estado queda en la línea siguiente.
pub fn render_mindfork(
    frame: &mut Frame,
    area: Rect,
    composer: &Composer,
    focused: bool,
    theme: &Theme,
) {
    if area.width < 4 || area.height < 3 {
        return;
    }
    let background = Style::default().bg(ratatui::style::Color::Rgb(15, 17, 21));
    let muted = Style::default().fg(ratatui::style::Color::Rgb(133, 139, 146));
    let prompt = Style::default().fg(ratatui::style::Color::Rgb(111, 192, 130));
    let text_style = theme.style(StyleToken::Default);
    let border = if focused {
        Style::default().fg(ratatui::style::Color::Rgb(121, 169, 219))
    } else {
        Style::default().fg(ratatui::style::Color::Rgb(54, 58, 66))
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border)
        .title(Span::styled(
            " input · Enter send · Shift+Enter newline ",
            muted,
        ))
        .style(background);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = if composer.is_empty() {
        vec![Line::from(vec![
            Span::styled("› ", prompt),
            Span::styled("Ask anything…", muted),
        ])]
    } else {
        composer
            .text()
            .split('\n')
            .enumerate()
            .map(|(row, text)| {
                Line::from(vec![
                    Span::styled(if row == 0 { "› " } else { "  " }, prompt),
                    Span::styled(text.to_string(), text_style),
                ])
            })
            .collect()
    };
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .style(background)
            .wrap(Wrap { trim: false }),
        inner,
    );
    if focused && !inner.is_empty() {
        let (row, col) = composer.cursor_row_col();
        let x = inner.x + 2 + col as u16;
        let y = inner.y + row as u16;
        if x < inner.right() && y < inner.bottom() {
            frame.set_cursor_position((x, y));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Composer;
    use ratatui::backend::{Backend, TestBackend};
    use ratatui::Terminal;

    fn buffer(c: &Composer, w: u16, h: u16) -> String {
        let backend = TestBackend::new(w, h);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f, f.area(), c, &Theme::test()))
            .unwrap();
        term.backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn shows_title_and_text() {
        let mut c = Composer::new();
        c.push_char('h');
        c.push_char('i');
        let out = buffer(&c, 40, 3);
        assert!(out.contains("PROMPT"));
        assert!(out.contains("hi"));
    }

    #[test]
    fn shows_model_shortcuts_and_live_count() {
        let mut c = Composer::new();
        c.insert_str("hi");
        let mut term = Terminal::new(TestBackend::new(72, 3)).unwrap();
        term.draw(|f| {
            render_focused_with_model(f, f.area(), &c, true, "Qwen3-Coder", &Theme::test())
        })
        .unwrap();
        let out = term
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(out.contains("Qwen3-Coder"));
        assert!(out.contains("Ctrl+J newline"));
        assert!(out.contains("1 line  ·  2 chars"));
    }

    #[test]
    fn empty_composer_does_not_panic() {
        let _ = buffer(&Composer::new(), 20, 3);
        let _ = buffer(&Composer::new(), 0, 0);
    }

    #[test]
    fn places_the_cursor_at_the_end() {
        let mut c = Composer::new();
        c.insert_str("hi");
        let backend = TestBackend::new(20, 3);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f, f.area(), &c, &Theme::test()))
            .unwrap();
        let pos = term.backend_mut().get_cursor_position().unwrap();
        assert_eq!((pos.x, pos.y), (5, 1));
    }

    #[test]
    fn places_the_cursor_on_the_second_line() {
        let mut c = Composer::new();
        c.insert_str("a\nb");
        let backend = TestBackend::new(20, 4);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f, f.area(), &c, &Theme::test()))
            .unwrap();
        let pos = term.backend_mut().get_cursor_position().unwrap();
        assert_eq!((pos.x, pos.y), (4, 2));
    }

    #[test]
    fn scrolls_horizontally_to_keep_the_cursor_visible() {
        let mut c = Composer::new();
        c.set_text("abcdefghijklmnop");
        let mut term = Terminal::new(TestBackend::new(8, 3)).unwrap();
        term.draw(|f| render(f, f.area(), &c, &Theme::test()))
            .unwrap();
        let pos = term.backend_mut().get_cursor_position().unwrap();
        assert_eq!(pos.x, 6);
        let visible = term
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(visible.contains("nop"));
        assert!(!visible.contains("abcdef"));
    }

    #[test]
    fn horizontal_view_keeps_wide_graphemes_whole_and_shows_text_after_cursor() {
        assert_eq!(line_view("abcd👩‍💻efgh", Some(9), 6), ("efgh".into(), 4));
        assert_eq!(line_view("abcdefghijkl", Some(6), 6), ("bcdefg".into(), 5));
    }

    #[test]
    fn border_shows_focus() {
        use ratatui::style::Color;
        let c = Composer::new();
        for (focused, color) in [
            (true, Color::Rgb(0, 212, 255)),
            (false, Color::Indexed(240)),
        ] {
            let mut term = Terminal::new(TestBackend::new(20, 3)).unwrap();
            term.draw(|f| render_focused(f, f.area(), &c, focused, &Theme::test()))
                .unwrap();
            assert_eq!(term.backend().buffer()[(0, 0)].fg, color);
        }
    }

    #[test]
    fn tiny_area_does_not_panic() {
        let c = Composer::new();
        let _ = buffer(&c, 0, 0);
        let _ = buffer(&c, 1, 1);
    }

    /// Pinta `render_crush` en un terminal de prueba y devuelve la pantalla
    /// (una línea por fila) y la posición del cursor (x, y).
    fn draw(c: &Composer, w: u16, h: u16, theme: &Theme) -> (String, Option<(u16, u16)>) {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| {
            render_crush(
                f,
                f.area(),
                c,
                true,
                "Describe what you want to build…",
                theme,
            )
        })
        .unwrap();
        let screen = term
            .backend()
            .buffer()
            .content
            .chunks(usize::from(w).max(1))
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        let pos = term
            .backend_mut()
            .get_cursor_position()
            .ok()
            .map(|p| (p.x, p.y));
        (screen, pos)
    }

    #[test]
    fn classic_height_is_logical_lines_plus_frame_capped() {
        let theme = Theme::test();
        let mut c = Composer::new();
        c.set_text("a\nb\nc");
        assert_eq!(height_for_look(&theme, &c, 40, 20), 5);
        assert_eq!(height_for_look(&theme, &c, 40, 4), 4);
    }

    #[test]
    fn crush_height_follows_wrapped_rows_between_3_and_15() {
        let theme = Theme::preset(crate::widgets::ThemePreset::Crush);
        let mut c = Composer::new();
        assert_eq!(height_for_look(&theme, &c, 20, 30), 3);
        c.set_text(&"x".repeat(100)); // 100 celdas en filas de 18 -> 6 filas
        assert_eq!(height_for_look(&theme, &c, 20, 30), 6);
        c.set_text(&"x\n".repeat(40));
        assert_eq!(height_for_look(&theme, &c, 20, 30), 15);
        assert_eq!(height_for_look(&theme, &c, 20, 8), 8);
    }

    #[test]
    fn crush_cursor_follows_the_wrapped_row() {
        let theme = Theme::preset(crate::widgets::ThemePreset::Crush);
        let mut c = Composer::new();
        c.set_text("abcdefghij"); // ancho 8: "> " + 6 celdas por fila
        let (_, cursor) = draw(&c, 8, 4, &theme);
        assert_eq!(cursor, Some((2 + 4, 1))); // fila 1 ("ghij"), tras 4 celdas
    }

    #[test]
    fn crush_keeps_the_cursor_row_visible_when_overflowing() {
        let theme = Theme::preset(crate::widgets::ThemePreset::Crush);
        let mut c = Composer::new();
        c.set_text("1\n2\n3\n4\n5\n6");
        let (screen, cursor) = draw(&c, 20, 3, &theme);
        assert!(screen.contains('6'));
        assert!(!screen.contains("> 1"));
        assert_eq!(cursor.map(|p| p.1), Some(2));
    }

    #[test]
    fn crush_uses_prompt_and_continuation_gutters() {
        let theme = Theme::preset(crate::widgets::ThemePreset::Crush);
        let mut c = Composer::new();
        c.set_text("uno\ndos");
        let (screen, _) = draw(&c, 20, 3, &theme);
        assert!(screen.contains("> uno"));
        assert!(screen.contains("::: dos"));
    }

    #[test]
    fn crush_empty_shows_the_placeholder() {
        let theme = Theme::preset(crate::widgets::ThemePreset::Crush);
        let (screen, cursor) = draw(&Composer::new(), 40, 3, &theme);
        assert!(screen.contains("> Describe what you want to build"));
        assert_eq!(cursor, Some((2, 0)));
    }
}
