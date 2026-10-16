//! Vista del transcript: ventana visible + scroll + wraps CJK-aware.

use crate::state::{Line, LineKind, Selection, StyleToken, Transcript, Viewport};
use crate::widgets::Theme;
use ratatui::buffer::{Buffer, CellDiffOption};
use ratatui::layout::Rect;
use ratatui::text::{Line as TLine, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;
use std::num::NonZeroU16;
use unicode_width::UnicodeWidthStr;

fn prefix(kind: LineKind) -> &'static str {
    match kind {
        LineKind::User => "you: ",
        LineKind::Assistant => "loom: ",
        LineKind::System => "sys: ",
        LineKind::Thinking => "… ",
        LineKind::ToolCall | LineKind::ToolResult => "⚙ ",
        LineKind::Todo => "☐ ",
        LineKind::Plan => "plan: ",
        LineKind::Banner => "",
        LineKind::Error => "! ",
    }
}

fn kind_token(kind: LineKind) -> StyleToken {
    match kind {
        LineKind::User => StyleToken::User,
        LineKind::Assistant => StyleToken::Assistant,
        LineKind::System => StyleToken::System,
        LineKind::Thinking => StyleToken::Muted,
        LineKind::ToolCall | LineKind::ToolResult => StyleToken::ToolName,
        LineKind::Todo => StyleToken::Heading,
        LineKind::Plan => StyleToken::Heading,
        LineKind::Banner => StyleToken::Accent,
        LineKind::Error => StyleToken::Error,
    }
}

/// Convierte una `Line` de estado en lineas ratatui, aplicando los `span` por
/// offsets sobre el texto plano (los huecos usan el token del kind).
fn render_line(line: &Line, theme: &Theme) -> Vec<TLine<'static>> {
    let base = theme.style(kind_token(line.kind));
    let prefix_text = prefix(line.kind);
    let text = line.plain();
    let mut spans: Vec<Span> = vec![Span::styled(prefix_text.to_string(), base)];

    if line.spans.is_empty() {
        spans.push(Span::styled(text.to_string(), base));
        return vec![TLine::from(spans)];
    }

    let mut spans_sorted = line.spans.clone();
    spans_sorted.sort_by_key(|s| s.start);
    let mut cursor = 0usize;
    for span in spans_sorted {
        let start = span.start.min(text.len());
        let end = (span.start + span.len).min(text.len());
        if start < cursor {
            continue;
        }
        if cursor < start {
            spans.push(Span::styled(text[cursor..start].to_string(), base));
        }
        spans.push(Span::styled(
            text[start..end].to_string(),
            theme.style(span.token),
        ));
        cursor = end;
    }
    if cursor < text.len() {
        spans.push(Span::styled(text[cursor..].to_string(), base));
    }
    vec![TLine::from(spans)]
}

/// Altura en filas de una línea a `width` columnas (mínimo 1).
fn measure(line: &Line, width: u16, theme: &Theme) -> usize {
    Paragraph::new(render_line(line, theme))
        .wrap(Wrap { trim: false })
        .line_count(width)
        .max(1)
}

#[derive(Debug, Clone)]
struct EntryLayout {
    rev: u64,
    heights: Vec<usize>,
}

/// Caché de alturas (filas tras el wrap) por entrada y ancho. La guarda quien
/// usa el widget; `sync` solo re-mide las entradas cuya revisión cambió.
#[derive(Debug, Default, Clone)]
pub struct Layout {
    width: u16,
    entries: Vec<EntryLayout>,
    /// Por línea global: fila donde empieza y altura.
    row_starts: Vec<usize>,
    heights: Vec<usize>,
    total_rows: usize,
    measured: usize,
}

impl Layout {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pone la caché al día para `width`.
    pub fn sync(&mut self, transcript: &Transcript, width: u16, theme: &Theme) {
        if width != self.width {
            self.entries.clear();
            self.width = width;
        }
        self.entries.truncate(transcript.entry_count());
        let mut first_changed = None;
        for i in 0..transcript.entry_count() {
            let rev = transcript.entry_rev(i);
            if self.entries.get(i).is_some_and(|e| e.rev == rev) {
                continue;
            }
            let heights = transcript
                .entry_lines(i)
                .iter()
                .map(|l| measure(l, width, theme))
                .collect();
            self.measured += 1;
            let layout = EntryLayout { rev, heights };
            if i < self.entries.len() {
                self.entries[i] = layout;
            } else {
                self.entries.push(layout);
            }
            first_changed.get_or_insert(i);
        }
        if let Some(entry) = first_changed {
            self.reindex_from(entry, transcript.entry_start(entry));
        }
    }

    fn reindex_from(&mut self, entry: usize, line: usize) {
        self.row_starts.truncate(line);
        self.heights.truncate(line);
        let mut acc = match line {
            0 => 0,
            _ => self.row_starts[line - 1] + self.heights[line - 1],
        };
        for e in &self.entries[entry..] {
            for &h in &e.heights {
                self.row_starts.push(acc);
                self.heights.push(h);
                acc += h;
            }
        }
        self.total_rows = acc;
    }

    pub fn total_rows(&self) -> usize {
        self.total_rows
    }

    /// Entradas medidas desde la creación (para tests de rendimiento).
    pub fn measured(&self) -> usize {
        self.measured
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    /// Fila global donde empieza la línea `line`.
    pub fn row_of(&self, line: usize) -> usize {
        self.row_starts
            .get(line)
            .copied()
            .unwrap_or(self.total_rows)
    }

    /// Filas que ocupa la línea `line`.
    pub fn height_of(&self, line: usize) -> usize {
        self.heights.get(line).copied().unwrap_or(0)
    }

    /// `(línea, fila dentro de la línea)` que ocupa la fila `row`.
    pub fn line_at_row(&self, row: usize) -> Option<(usize, usize)> {
        if self.heights.is_empty() {
            return None;
        }
        let row = row.min(self.total_rows.saturating_sub(1));
        let line = self.row_starts.partition_point(|&s| s <= row) - 1;
        Some((line, row - self.row_starts[line]))
    }
}

fn osc8(url: &str, text: &str) -> String {
    format!("\x1b]8;;{url}\x1b\\{text}\x1b]8;;\x1b\\")
}

/// Envuelve con OSC 8 los enlaces de las líneas que se ven enteras. Técnica
/// de hyperrat, un tramo por fila: la primera celda lleva la secuencia con
/// el texto del tramo y ancho forzado; el resto se deja en blanco y fuera
/// del diff (el terminal ya escribió ahí el texto).
fn apply_links(
    buf: &mut Buffer,
    area: Rect,
    transcript: &Transcript,
    layout: &Layout,
    top: usize,
    theme: &Theme,
) {
    if !theme.hyperlinks {
        return;
    }
    let bottom = top + area.height as usize;
    let Some((mut i, _)) = layout.line_at_row(top) else {
        return;
    };
    while i < transcript.len() && layout.row_of(i) < bottom {
        let (start, h) = (layout.row_of(i), layout.height_of(i));
        if let Some(line) = transcript.line(i) {
            if !line.links.is_empty() && start >= top && start + h <= bottom {
                let y0 = area.y + (start - top) as u16;
                link_line(buf, area, y0, h as u16, line);
            }
        }
        i += 1;
    }
}

/// Celda visible: posición, texto y ancho.
struct VisibleCell {
    x: u16,
    y: u16,
    symbol: String,
    width: usize,
}

fn link_line(buf: &mut Buffer, area: Rect, y0: u16, h: u16, line: &Line) {
    let mut cells = Vec::new();
    for y in y0..y0 + h {
        let mut x = area.x;
        while x < area.x + area.width {
            let symbol = buf[(x, y)].symbol().to_string();
            let width = symbol.width().max(1);
            cells.push(VisibleCell {
                x,
                y,
                symbol,
                width,
            });
            x += width as u16;
        }
    }
    // Texto sin espacios con el índice de la celda de cada símbolo: el wrap
    // mueve o rellena espacios, así que se compara sin ellos.
    let mut ns = String::new();
    let mut ns_cells: Vec<(usize, usize)> = Vec::new();
    for (j, c) in cells.iter().enumerate() {
        if !c.symbol.trim().is_empty() {
            ns_cells.push((ns.len(), j));
            ns.push_str(&c.symbol);
        }
    }
    let strip = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let prefix_len = strip(prefix(line.kind)).len();
    for link in &line.links {
        let Some(label) = line.text.get(link.start..link.start + link.len) else {
            continue;
        };
        let label = strip(label);
        if label.is_empty() {
            continue;
        }
        let nth = strip(&line.text[..link.start])
            .matches(label.as_str())
            .count();
        let Some(pos) = ns
            .get(prefix_len..)
            .and_then(|rest| rest.match_indices(label.as_str()).nth(nth))
            .map(|(p, _)| p + prefix_len)
        else {
            continue;
        };
        let end = pos + label.len();
        let covered: Vec<usize> = ns_cells
            .iter()
            .filter(|(off, _)| *off >= pos && *off < end)
            .map(|&(_, j)| j)
            .collect();
        let (Some(&first), Some(&last)) = (covered.first(), covered.last()) else {
            continue;
        };
        let mut j = first;
        while j <= last {
            let y = cells[j].y;
            let mut k = j;
            while k < last && cells[k + 1].y == y {
                k += 1;
            }
            // Tramo de la fila: de la primera a la última celda cubierta en ella.
            let row_last = covered
                .iter()
                .copied()
                .filter(|&c| cells[c].y == y)
                .max()
                .unwrap_or(j);
            let k = k.min(row_last);
            let text: String = cells[j..=k].iter().map(|c| c.symbol.as_str()).collect();
            let width: usize = cells[j..=k].iter().map(|c| c.width).sum();
            if let Some(w) = NonZeroU16::new(width as u16) {
                buf[(cells[j].x, y)]
                    .set_symbol(&osc8(&link.url, &text))
                    .set_diff_option(CellDiffOption::ForcedWidth(w));
                for c in &cells[j + 1..=k] {
                    buf[(c.x, c.y)]
                        .set_symbol(" ")
                        .set_diff_option(CellDiffOption::Skip);
                }
            }
            // Siguiente fila con celdas cubiertas.
            j = match covered.iter().copied().find(|&c| cells[c].y > y) {
                Some(next) => next,
                None => break,
            };
        }
    }
}

/// Pinta el transcript: solo las líneas visibles. `layout` cachea alturas;
/// `viewport` recibe la geometría y el ancla; `selection` se resalta y su
/// extremo se mantiene visible.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    transcript: &Transcript,
    viewport: &mut Viewport,
    layout: &mut Layout,
    selection: Option<&Selection>,
    theme: &Theme,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let rows = area.height as usize;
    let prev_width = layout.width();
    // Ancho de texto: si ya se medía sin la columna de la scrollbar y sigue
    // desbordando, se mantiene (así no se invalida la caché cada frame).
    let narrow = area.width.saturating_sub(1).max(1);
    let text_w = if area.width >= 2 && prev_width == narrow {
        layout.sync(transcript, narrow, theme);
        if layout.total_rows() > rows {
            narrow
        } else {
            layout.sync(transcript, area.width, theme);
            area.width
        }
    } else {
        layout.sync(transcript, area.width, theme);
        if area.width >= 2 && layout.total_rows() > rows {
            layout.sync(transcript, narrow, theme);
            narrow
        } else {
            area.width
        }
    };
    let overflow = text_w < area.width;

    if prev_width != 0 && prev_width != text_w && !viewport.is_following() {
        if let Some((line, row)) = viewport.anchor() {
            let h = layout.height_of(line).max(1);
            viewport.restore_top(layout.row_of(line) + row.min(h - 1));
        }
    }
    viewport.set_geometry(layout.total_rows(), rows);
    if let (Some(s), false) = (selection, transcript.is_empty()) {
        let head = s.head().min(transcript.len() - 1);
        let start = layout.row_of(head);
        viewport.ensure_visible(start, start + layout.height_of(head));
    }

    let text_area = Rect {
        width: text_w,
        ..area
    };
    if let Some((first, skip)) = layout.line_at_row(viewport.top()) {
        viewport.set_anchor(first, skip);
        let sel_style = theme.styles.selection;
        let mut lines = Vec::new();
        let mut filled = 0;
        let mut i = first;
        while i < transcript.len() && filled < rows + skip {
            if let Some(line) = transcript.line(i) {
                let mut out = render_line(line, theme);
                if selection.is_some_and(|s| s.contains(i)) {
                    for tl in &mut out {
                        tl.style = tl.style.patch(sel_style);
                    }
                }
                lines.extend(out);
            }
            filled += layout.height_of(i);
            i += 1;
        }
        let skip = u16::try_from(skip).unwrap_or(u16::MAX);
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .scroll((skip, 0)),
            text_area,
        );
        apply_links(
            frame.buffer_mut(),
            text_area,
            transcript,
            layout,
            viewport.top(),
            theme,
        );
    }
    if overflow {
        let bar = Rect {
            x: area.x + area.width - 1,
            width: 1,
            ..area
        };
        crate::widgets::scrollbar::render(frame, bar, viewport.scroll(), theme);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Line, LineKind, Transcript, Viewport};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn draw(
        transcript: &Transcript,
        vp: &mut Viewport,
        layout: &mut Layout,
        w: u16,
        h: u16,
    ) -> String {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| render(f, f.area(), transcript, vp, layout, None, &Theme::test()))
            .unwrap();
        term.backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    fn buffer(transcript: &Transcript, w: u16, h: u16) -> String {
        draw(transcript, &mut Viewport::new(), &mut Layout::new(), w, h)
    }

    #[test]
    fn shows_user_and_assistant_prefixes() {
        let mut t = Transcript::new();
        t.push(Line::new(LineKind::User, "hola"));
        t.push(Line::new(LineKind::Assistant, "buenas"));
        let out = buffer(&t, 40, 10);
        assert!(out.contains("you:"));
        assert!(out.contains("hola"));
        assert!(out.contains("buenas"));
    }

    #[test]
    fn empty_transcript_does_not_panic() {
        let _ = buffer(&Transcript::new(), 40, 10);
    }

    #[test]
    fn zero_and_tiny_sizes_do_not_panic() {
        let mut t = Transcript::new();
        t.push(Line::new(LineKind::Assistant, "x"));
        let _ = buffer(&t, 0, 0);
        let _ = buffer(&t, 1, 1);
    }

    #[test]
    fn wraps_a_long_unbroken_token_without_panicking() {
        let mut t = Transcript::new();
        let url = "https://example.com/".to_string() + &"a".repeat(300);
        t.push(Line::new(LineKind::Assistant, url));
        let out = buffer(&t, 20, 5);
        // No se desborda: no hay panico y el buffer tiene la altura pedida.
        assert_eq!(out.chars().count(), 20 * 5);
    }

    #[test]
    fn truncates_on_display_width_not_bytes() {
        // CJK ocupa 2 columnas cada uno; ASCII 1.
        let mut t = Transcript::new();
        t.push(Line::new(LineKind::Assistant, "日本語 テキスト 🚀 hola"));
        let _ = buffer(&t, 12, 4); // no debe entrar en panico por anchura
    }

    #[test]
    fn shows_only_the_viewport_window() {
        let mut t = Transcript::new();
        for i in 0..50 {
            t.push(Line::new(LineKind::Assistant, format!("line {i}")));
        }
        let out = buffer(&t, 40, 5); // 5 filas -> ventana 45..50
        assert!(out.contains("line 49"));
        assert!(!out.contains("line 0"));
    }

    #[test]
    fn shows_a_scrollbar_when_content_overflows() {
        let mut t = Transcript::new();
        for i in 0..50 {
            t.push(Line::new(LineKind::Assistant, format!("line {i}")));
        }
        let out = buffer(&t, 20, 5);
        assert!(
            out.contains('\u{2551}') || out.contains('\u{2588}'),
            "esperaba una barra de scroll"
        );
    }

    #[test]
    fn no_scrollbar_when_content_fits() {
        let mut t = Transcript::new();
        t.push(Line::new(LineKind::Assistant, "hi"));
        let out = buffer(&t, 20, 5);
        assert!(!out.contains('\u{2551}') && !out.contains('\u{2588}'));
    }

    #[test]
    fn wrapped_lines_count_as_rows_when_following() {
        let mut t = Transcript::new();
        t.push(Line::new(LineKind::User, "inicio"));
        t.push(Line::new(LineKind::Assistant, "palabra ".repeat(20)));
        t.push(Line::new(LineKind::Assistant, "fin"));
        let out = buffer(&t, 20, 3);
        assert!(out.contains("fin"), "la última línea debe verse");
        assert!(!out.contains("inicio"));
    }

    #[test]
    fn reading_up_keeps_the_view_while_content_grows() {
        let mut t = Transcript::new();
        for i in 0..50 {
            t.push(Line::new(LineKind::Assistant, format!("line {i:02}")));
        }
        let mut vp = Viewport::new();
        let mut layout = Layout::new();
        draw(&t, &mut vp, &mut layout, 30, 5);
        vp.scroll_up(10);
        let before = draw(&t, &mut vp, &mut layout, 30, 5);
        for i in 50..70 {
            t.push(Line::new(LineKind::Assistant, format!("line {i:02}")));
        }
        let after = draw(&t, &mut vp, &mut layout, 30, 5);
        assert!(before.contains("line 35"));
        assert!(after.contains("line 35"));
        assert!(!after.contains("line 69"));
    }

    #[test]
    fn selected_lines_are_highlighted() {
        use crate::state::Selection;
        let mut t = Transcript::new();
        for s in ["uno", "dos", "tres"] {
            t.push(Line::new(LineKind::Assistant, s));
        }
        let mut term = Terminal::new(TestBackend::new(30, 5)).unwrap();
        let mut vp = Viewport::new();
        let mut layout = Layout::new();
        let sel = Selection::at(1);
        term.draw(|f| {
            render(
                f,
                f.area(),
                &t,
                &mut vp,
                &mut layout,
                Some(&sel),
                &Theme::test(),
            )
        })
        .unwrap();
        let buf = term.backend().buffer();
        assert_eq!(buf[(0, 1)].bg, ratatui::style::Color::Indexed(238));
        assert_ne!(buf[(0, 0)].bg, ratatui::style::Color::Indexed(238));
    }

    #[test]
    fn the_selection_head_is_scrolled_into_view() {
        use crate::state::Selection;
        let mut t = Transcript::new();
        for i in 0..50 {
            t.push(Line::new(LineKind::Assistant, format!("line {i:02}")));
        }
        let mut term = Terminal::new(TestBackend::new(30, 5)).unwrap();
        let mut vp = Viewport::new();
        let mut layout = Layout::new();
        let sel = Selection::at(0);
        term.draw(|f| {
            render(
                f,
                f.area(),
                &t,
                &mut vp,
                &mut layout,
                Some(&sel),
                &Theme::test(),
            )
        })
        .unwrap();
        let out: String = term
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(out.contains("line 00"));
        assert!(!vp.is_following());
    }

    #[test]
    fn following_shows_new_content() {
        let mut t = Transcript::new();
        for i in 0..50 {
            t.push(Line::new(LineKind::Assistant, format!("line {i:02}")));
        }
        let mut vp = Viewport::new();
        let mut layout = Layout::new();
        draw(&t, &mut vp, &mut layout, 30, 5);
        t.push(Line::new(LineKind::Assistant, "nueva"));
        assert!(draw(&t, &mut vp, &mut layout, 30, 5).contains("nueva"));
    }

    fn long_transcript(n: usize) -> (Transcript, crate::state::EntryId) {
        let mut t = Transcript::new();
        for i in 0..n {
            t.push_entry(
                LineKind::Assistant,
                crate::state::Format::Plain,
                format!("l{i:05}"),
            );
        }
        let last = t.push_entry(LineKind::Assistant, crate::state::Format::Plain, "fin");
        (t, last)
    }

    #[test]
    fn only_changed_entries_are_measured() {
        let (mut t, last) = long_transcript(1000);
        let (mut vp, mut layout) = (Viewport::new(), Layout::new());
        draw(&t, &mut vp, &mut layout, 40, 10);
        draw(&t, &mut vp, &mut layout, 40, 10);
        let before = layout.measured();
        t.append(last, " y más");
        let out = draw(&t, &mut vp, &mut layout, 40, 10);
        assert_eq!(layout.measured(), before + 1);
        assert!(out.contains("fin y más"));
    }

    #[test]
    fn width_change_remeasures_everything() {
        let (t, _) = long_transcript(100);
        let (mut vp, mut layout) = (Viewport::new(), Layout::new());
        draw(&t, &mut vp, &mut layout, 40, 10);
        let before = layout.measured();
        draw(&t, &mut vp, &mut layout, 60, 10);
        assert!(layout.measured() >= before + 101);
    }

    #[test]
    fn line_at_row_handles_multi_row_lines() {
        let mut t = Transcript::new();
        t.push(Line::new(LineKind::Assistant, "corta"));
        t.push(Line::new(LineKind::Assistant, "palabra ".repeat(10)));
        t.push(Line::new(LineKind::Assistant, "otra"));
        let mut layout = Layout::new();
        layout.sync(&t, 20, &Theme::test());
        let h1 = layout.height_of(1);
        assert!(h1 > 1);
        assert_eq!(layout.line_at_row(0), Some((0, 0)));
        assert_eq!(layout.line_at_row(1), Some((1, 0)));
        assert_eq!(layout.line_at_row(h1), Some((1, h1 - 1)));
        assert_eq!(layout.line_at_row(h1 + 1), Some((2, 0)));
        assert_eq!(layout.line_at_row(10_000), Some((2, 0)));
    }

    #[test]
    fn rows_beyond_u16_are_reachable() {
        let mut t = Transcript::new();
        for i in 0..70_000 {
            t.push(Line::new(LineKind::Banner, format!("l{i:05}")));
        }
        let (mut vp, mut layout) = (Viewport::new(), Layout::new());
        let out = draw(&t, &mut vp, &mut layout, 20, 5);
        assert!(out.contains("l69999"));
        vp.to_top();
        let out = draw(&t, &mut vp, &mut layout, 20, 5);
        assert!(out.contains("l00000"));
    }

    #[test]
    fn width_change_keeps_the_anchor_line_on_top() {
        let mut t = Transcript::new();
        for i in 0..200 {
            t.push(Line::new(
                LineKind::Assistant,
                format!("a{i:03} {}", "xx ".repeat(15)),
            ));
        }
        let (mut vp, mut layout) = (Viewport::new(), Layout::new());
        draw(&t, &mut vp, &mut layout, 40, 10);
        vp.scroll_up(100);
        draw(&t, &mut vp, &mut layout, 40, 10);
        let (line, _) = vp.anchor().unwrap();
        draw(&t, &mut vp, &mut layout, 25, 10);
        assert_eq!(vp.anchor().unwrap().0, line);
    }

    /// Buffer del frame (no el del backend: `TestBackend` solo copia las
    /// celdas que emite el diff, y las `Skip` no se emiten).
    fn link_buffer(text: &str, w: u16, h: u16, theme: &Theme) -> Buffer {
        let mut t = Transcript::new();
        t.push_entry(LineKind::Assistant, crate::state::Format::Markdown, text);
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        let (mut vp, mut layout) = (Viewport::new(), Layout::new());
        let mut frame_buf = Buffer::empty(Rect::new(0, 0, w, h));
        term.draw(|f| {
            render(f, f.area(), &t, &mut vp, &mut layout, None, theme);
            frame_buf = f.buffer_mut().clone();
        })
        .unwrap();
        frame_buf
    }

    #[test]
    fn links_are_wrapped_in_osc8() {
        let buf = link_buffer("ver [docs](https://x.y) ya", 40, 3, &Theme::test());
        let osc = "\x1b]8;;https://x.y\x1b\\docs\x1b]8;;\x1b\\";
        let i = buf
            .content
            .iter()
            .position(|c| c.symbol() == osc)
            .expect("celda con OSC 8");
        assert_eq!(
            buf.content[i].diff_option,
            CellDiffOption::ForcedWidth(NonZeroU16::new(4).unwrap())
        );
        assert_eq!(buf.content[i + 1].diff_option, CellDiffOption::Skip);
        assert_eq!(buf.content[i + 1].symbol(), " ");
    }

    #[test]
    fn wrapped_links_get_one_segment_per_row() {
        let buf = link_buffer(
            "[documentacion muy larga](https://x.y)",
            20,
            4,
            &Theme::test(),
        );
        let n = buf
            .content
            .iter()
            .filter(|c| c.symbol().starts_with("\x1b]8;;https://x.y"))
            .count();
        assert_eq!(n, 2);
    }

    #[test]
    fn hyperlinks_can_be_disabled() {
        let mut th = Theme::test();
        th.hyperlinks = false;
        let buf = link_buffer("ver [docs](https://x.y) ya", 40, 3, &th);
        assert!(buf.content.iter().all(|c| !c.symbol().contains("\x1b]8")));
    }
}
