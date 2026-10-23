//! Vista del transcript: ventana visible + scroll + wraps CJK-aware.

use crate::state::{
    CellPos, DiffKind, Line, LineKind, Selection, StyleToken, TextSelection, ToolStatus,
    Transcript, Viewport,
};
use crate::widgets::Theme;
use ratatui::buffer::{Buffer, CellDiffOption};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line as TLine, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};
use ratatui::Frame;
use std::num::NonZeroU16;
use unicode_width::UnicodeWidthStr;

fn prefix(kind: LineKind) -> &'static str {
    match kind {
        LineKind::User | LineKind::Assistant => "",
        LineKind::System => "sys: ",
        LineKind::Thinking => "… ",
        LineKind::ToolCall | LineKind::ToolResult => "⚙ ",
        LineKind::Todo => "☐ ",
        LineKind::Plan => "plan: ",
        LineKind::Banner => "",
        LineKind::Error => "! ",
        LineKind::ToolHead(_) => "● ",
        LineKind::ActivityHead(_) => "◆ ",
        LineKind::TodoItem(ToolStatus::Running) => "◐ ",
        LineKind::TodoItem(ToolStatus::Success) => "☑ ",
        LineKind::TodoItem(_) => "☐ ",
        LineKind::ToolBody | LineKind::ToolDiff(_) => "│ ",
        LineKind::ToolMore => "  ",
        LineKind::NodeHead(_) | LineKind::NodeBody => "",
        LineKind::Fold => "",
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
        LineKind::ToolHead(status) | LineKind::ActivityHead(status) => status_token(status),
        LineKind::TodoItem(ToolStatus::Running) => StyleToken::TodoInProgress,
        LineKind::TodoItem(ToolStatus::Success) => StyleToken::TodoCompleted,
        LineKind::TodoItem(_) => StyleToken::TodoPending,
        LineKind::ToolBody => StyleToken::Muted,
        LineKind::ToolDiff(DiffKind::Insert) => StyleToken::DiffInsert,
        LineKind::ToolDiff(DiffKind::Delete) => StyleToken::DiffDelete,
        LineKind::ToolDiff(DiffKind::Hunk) => StyleToken::DiffHunk,
        LineKind::ToolDiff(DiffKind::Context) => StyleToken::Muted,
        LineKind::ToolMore => StyleToken::Dim,
        LineKind::NodeHead(status) => status_token(status),
        LineKind::NodeBody => StyleToken::Muted,
        LineKind::Fold => StyleToken::Dim,
    }
}

/// Token de la cabecera de una tool o de un elemento de actividad según su estado.
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

/// Convierte una `Line` de estado en lineas ratatui, aplicando los `span` por
/// offsets sobre el texto plano (los huecos usan el token del kind).
fn render_line(line: &Line, theme: &Theme, entry_start: bool) -> Vec<TLine<'static>> {
    let base = theme.style(kind_token(line.kind));
    let role = match (entry_start, line.kind) {
        (true, LineKind::User) => Some(("You", theme.styles.user)),
        (true, LineKind::Assistant) => Some(("Assistant", theme.styles.assistant)),
        _ => None,
    };
    let text = line.plain();
    let mut rendered = Vec::new();
    if let Some((label, style)) = role {
        rendered.push(TLine::from(Span::styled(
            label,
            style.add_modifier(Modifier::BOLD),
        )));
    }
    let mut spans: Vec<Span> = vec![Span::styled(prefix(line.kind).to_string(), base)];

    if line.spans.is_empty() {
        spans.push(Span::styled(text.to_string(), base));
        rendered.push(TLine::from(spans));
        return rendered;
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
    rendered.push(TLine::from(spans));
    rendered
}

/// Altura en filas de una línea a `width` columnas (mínimo 1).
fn measure(line: &Line, width: u16, theme: &Theme, entry_start: bool) -> usize {
    Paragraph::new(render_line(line, theme, entry_start))
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
                .enumerate()
                .map(|(line, l)| measure(l, width, theme, line == 0))
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
            let header = usize::from(
                transcript.locate(i).is_some_and(|(_, row)| row == 0)
                    && matches!(line.kind, LineKind::User | LineKind::Assistant),
            );
            let body_start = start + header;
            let body_height = h.saturating_sub(header);
            if !line.links.is_empty() && body_start >= top && body_start + body_height <= bottom {
                let y0 = area.y + (body_start - top) as u16;
                link_line(buf, area, y0, body_height as u16, line);
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
                let entry_start = transcript.locate(i).is_some_and(|(_, line)| line == 0);
                let mut out = render_line(line, theme, entry_start);
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

/// Posición de documento bajo la celda `(x, y)` de la pantalla. Consulta pura
/// que acota `y` a las filas de `area` y `x` al ancho del texto, de modo que
/// nunca devuelve una posición fuera del diseño. `None` si no hay área o
/// contenido.
pub fn hit_cell(
    layout: &Layout,
    viewport: &Viewport,
    area: Rect,
    x: u16,
    y: u16,
) -> Option<CellPos> {
    if area.width == 0 || area.height == 0 || layout.total_rows() == 0 {
        return None;
    }
    let last_row = layout.total_rows() - 1;
    let bottom = area.y.saturating_add(area.height) - 1;
    let y = y.clamp(area.y, bottom);
    let row = (viewport.top() + usize::from(y - area.y)).min(last_row);
    let last_col = layout.width().max(1) - 1;
    let col = x.saturating_sub(area.x).min(last_col);
    Some(CellPos { row, col })
}

/// Resalta con `style` la selección sobre el búfer ya pintado. Solo parchea las
/// celdas de las filas visibles que caen dentro del rango: la primera fila
/// desde `start.col`, la última hasta `end.col` (incluida) y las intermedias
/// enteras. No re-renderiza el transcript.
pub fn paint_text_selection(
    buf: &mut Buffer,
    area: Rect,
    layout: &Layout,
    viewport: &Viewport,
    sel: &TextSelection,
    style: Style,
) {
    if sel.is_empty() || area.width == 0 || area.height == 0 || layout.total_rows() == 0 {
        return;
    }
    let (start, end) = sel.ordered();
    let last_row = layout.total_rows() - 1;
    if start.row > last_row {
        return;
    }
    let end_row = end.row.min(last_row);
    let top = viewport.top();
    let first = start.row.max(top);
    let last = end_row.min(top + usize::from(area.height) - 1);
    if first > last {
        return;
    }
    let text_w = layout.width().min(area.width);
    for row in first..=last {
        let dy = u16::try_from(row - top).unwrap_or(u16::MAX);
        let y = area.y + dy;
        let from = if row == start.row { start.col } else { 0 };
        let to = if row == end_row {
            end.col.saturating_add(1).min(text_w)
        } else {
            text_w
        };
        for x in from.min(text_w)..to.min(text_w) {
            if let Some(cell) = buf.cell_mut((area.x + x, y)) {
                cell.set_style(style);
            }
        }
    }
}

/// Lee de un búfer de una línea renderizada el texto de la fila `y` entre las
/// columnas `from` (incluida) y `to` (exclusiva; el llamador suma uno al extremo incluido). Un carácter ancho entra solo
/// si empieza dentro del rango: avanza por su ancho, así que su celda de
/// continuación nunca se copia suelta ni a medias.
fn row_text(buf: &Buffer, y: u16, width: u16, from: u16, to: u16) -> String {
    let mut out = String::new();
    let mut x: u16 = 0;
    while x < width {
        let symbol = buf[(x, y)].symbol();
        if x >= from && x < to {
            out.push_str(symbol);
        }
        x = x.saturating_add(symbol.width().max(1) as u16);
    }
    out.trim_end().to_string()
}

/// Texto plano de la selección. Re-renderiza fuera de pantalla, línea a línea y
/// con el mismo recorrido que `render`, solo las líneas que cubren el rango; así
/// el texto copiado es exactamente lo que se ve. Solo se llama al copiar.
pub fn selected_text(
    transcript: &Transcript,
    layout: &Layout,
    theme: &Theme,
    sel: &TextSelection,
) -> String {
    let text_w = layout.width();
    if sel.is_empty() || layout.total_rows() == 0 || text_w == 0 {
        return String::new();
    }
    let (start, end) = sel.ordered();
    let last_row = layout.total_rows() - 1;
    if start.row > last_row {
        return String::new();
    }
    let end_row = end.row.min(last_row);
    let Some((first, _)) = layout.line_at_row(start.row) else {
        return String::new();
    };
    let Some((last, _)) = layout.line_at_row(end_row) else {
        return String::new();
    };
    let mut rows = Vec::new();
    for i in first..=last {
        let (Some(line), line_row, h) = (transcript.line(i), layout.row_of(i), layout.height_of(i))
        else {
            break;
        };
        if h == 0 || start.row > line_row + h - 1 || end_row < line_row {
            continue;
        }
        let h16 = u16::try_from(h).unwrap_or(u16::MAX);
        let entry_start = transcript.locate(i).is_some_and(|(_, l)| l == 0);
        let area = Rect::new(0, 0, text_w, h16);
        let mut buf = Buffer::empty(area);
        Paragraph::new(render_line(line, theme, entry_start))
            .wrap(Wrap { trim: false })
            .render(area, &mut buf);
        let lo = start.row.max(line_row);
        let hi = end_row.min(line_row + h - 1);
        for row in lo..=hi {
            let y = u16::try_from(row - line_row).unwrap_or(u16::MAX);
            let from = if row == start.row { start.col } else { 0 };
            let to = if row == end_row {
                end.col.saturating_add(1).min(text_w)
            } else {
                text_w
            };
            rows.push(row_text(&buf, y, text_w, from, to));
        }
    }
    rows.join("\n")
}

/// Rol de mensaje de la vista Pawan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PawanRole {
    User,
    Assistant,
    System,
    Tool,
    Error,
}

/// Mensaje de ejemplo para el renderer Pawan.
pub struct PawanMessage<'a> {
    pub role: PawanRole,
    pub elapsed: &'a str,
    pub content: &'a str,
    pub streaming: bool,
}

/// Renderiza mensajes Pawan con badge de rol, tiempo y contenido indentado.
pub fn render_pawan(frame: &mut Frame, area: Rect, messages: &[PawanMessage<'_>]) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let muted = Style::default().fg(Color::Rgb(136, 136, 136));
    let mut lines = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        if index > 0 {
            lines.push(TLine::raw(""));
        }
        let (label, badge_color) = match message.role {
            PawanRole::User => (" You ", Color::Rgb(0, 204, 204)),
            PawanRole::Assistant => (" Pawan ", Color::Rgb(80, 250, 123)),
            PawanRole::System => (" System ", Color::Rgb(255, 184, 108)),
            PawanRole::Tool => (" Tool ", Color::Rgb(0, 153, 153)),
            PawanRole::Error => (" Error ", Color::Rgb(255, 85, 85)),
        };
        lines.push(TLine::from(vec![
            Span::styled(
                label,
                Style::default()
                    .fg(Color::Black)
                    .bg(badge_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  {}", message.elapsed), muted),
        ]));
        lines.extend(message.content.lines().map(|line| {
            TLine::from(vec![
                Span::raw("  "),
                Span::styled(line.to_string(), Style::default().fg(Color::White)),
            ])
        }));
        if message.streaming {
            lines.push(TLine::from(Span::styled(
                "  ▌",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::SLOW_BLINK),
            )));
        }
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

/// Estado de generación sin bloques, como el spinner de "Pawan is thinking…".
pub fn render_pawan_waiting(frame: &mut Frame, area: Rect, anim: &crate::widgets::Anim) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let glyph = anim.glyph(
        crate::widgets::spinner::PULSE,
        &Theme::preset(crate::widgets::ThemePreset::Pawan),
    );
    frame.render_widget(
        Paragraph::new(TLine::from(vec![
            Span::raw("  "),
            glyph,
            Span::styled(
                " Pawan is thinking…",
                Style::default()
                    .fg(Color::Rgb(255, 184, 108))
                    .add_modifier(Modifier::ITALIC),
            ),
        ])),
        area,
    );
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
    fn shows_crush_style_role_headers() {
        let mut t = Transcript::new();
        t.push(Line::new(LineKind::User, "hola"));
        t.push(Line::new(LineKind::Assistant, "buenas"));
        let out = buffer(&t, 40, 10);
        assert!(out.contains("You"));
        assert!(out.contains("Assistant"));
        assert!(!out.contains("loom:"));
        assert!(out.contains("hola"));
        assert!(out.contains("buenas"));
    }

    #[test]
    fn tool_entries_paint_a_prefix_that_is_not_part_of_the_text() {
        use crate::state::{tool_lines, ToolItem};
        let mut item = ToolItem::new("c1", "View");
        item.set_summary("src/lib.rs");
        item.set_result(false);
        item.set_output("uno\ndos");
        let mut t = Transcript::new();
        t.push_lines(tool_lines(&item));
        let out = buffer(&t, 40, 5);
        assert!(out.contains("● View"), "falta la cabecera de la tool");
        assert!(out.contains("│ uno"), "falta el cuerpo de la tool");
        assert!(!t.line(0).unwrap().plain().contains('●'));
        assert!(!t.line(1).unwrap().plain().contains('│'));
    }

    #[test]
    fn diff_lines_render_with_insert_and_delete_styles() {
        use crate::state::{DiffKind, StyleToken};
        let theme = Theme::test();
        let mut t = Transcript::new();
        let line = |kind: DiffKind, text: &str| {
            let mut l = Line::new(LineKind::ToolDiff(kind), text);
            l.spans.push(crate::state::SpanSpec {
                start: 0,
                len: text.len(),
                token: match kind {
                    DiffKind::Insert => StyleToken::DiffInsert,
                    DiffKind::Delete => StyleToken::DiffDelete,
                    _ => StyleToken::DiffHunk,
                },
            });
            l
        };
        t.push_lines(vec![
            line(DiffKind::Delete, "- uno"),
            line(DiffKind::Insert, "+ dos"),
        ]);
        let mut term = Terminal::new(TestBackend::new(20, 3)).unwrap();
        term.draw(|f| {
            render(
                f,
                f.area(),
                &t,
                &mut Viewport::new(),
                &mut Layout::new(),
                None,
                &theme,
            )
        })
        .unwrap();
        let buf = term.backend().buffer();
        // Con el prefijo «│ » el texto empieza en la columna 2.
        let deleted = theme.style(StyleToken::DiffDelete);
        let inserted = theme.style(StyleToken::DiffInsert);
        assert_ne!(deleted.fg, inserted.fg, "precondición: estilos distintos");
        assert_eq!(Some(buf[(2, 0)].fg), deleted.fg);
        assert_eq!(Some(buf[(2, 1)].fg), inserted.fg);
    }

    #[test]
    fn activity_entries_paint_a_prefix_that_is_not_part_of_the_text() {
        use crate::state::{activity_lines, ActivityView, ToolStatus};
        let lines = activity_lines(&ActivityView {
            kind: "job",
            label: "build",
            status: ToolStatus::Running,
            progress: None,
            body: None,
        });
        let mut t = Transcript::new();
        t.push_lines(lines);
        let out = buffer(&t, 40, 3);
        assert!(out.contains("◆ job"), "falta la cabecera de la actividad");
        assert!(!t.line(0).unwrap().plain().contains('◆'));
    }

    #[test]
    fn node_lines_have_no_glyph_prefix() {
        assert_eq!(prefix(LineKind::NodeHead(ToolStatus::Running)), "");
        assert_eq!(prefix(LineKind::NodeBody), "");
    }

    #[test]
    fn todo_entries_paint_a_prefix_that_is_not_part_of_the_text() {
        use crate::state::{todo_lines, ToolStatus};
        let lines = todo_lines(&[("uno", ToolStatus::Success), ("dos", ToolStatus::Running)]);
        let mut t = Transcript::new();
        t.push_lines(lines);
        let out = buffer(&t, 40, 4);
        assert!(out.contains("☑ uno"), "falta la tarea hecha");
        assert!(out.contains("◐ dos"), "falta la tarea en curso");
        assert!(!t.line(1).unwrap().plain().contains('☑'));
        assert!(!t.line(2).unwrap().plain().contains('◐'));
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
        assert!(out.contains('│'), "falta la pista de la barra");
        assert!(out.contains('█'), "falta el thumb de la barra");
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
        assert!(before.contains("line 42"));
        assert!(after.contains("line 42"));
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
        t.push(Line::new(LineKind::Banner, "corta"));
        t.push(Line::new(LineKind::Banner, "palabra ".repeat(10)));
        t.push(Line::new(LineKind::Banner, "otra"));
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

    fn banners(texts: &[&str]) -> Transcript {
        let mut t = Transcript::new();
        for s in texts {
            t.push(Line::new(LineKind::Banner, *s));
        }
        t
    }

    fn select(from: CellPos, to: CellPos) -> TextSelection {
        let mut sel = TextSelection::new(from);
        sel.set_head(to);
        sel
    }

    #[test]
    fn hit_cell_maps_screen_to_document_with_scroll() {
        let mut t = Transcript::new();
        for i in 0..50 {
            t.push(Line::new(LineKind::Banner, format!("line {i:02}")));
        }
        let (mut vp, mut layout) = (Viewport::new(), Layout::new());
        draw(&t, &mut vp, &mut layout, 30, 5);
        vp.scroll_up(10);
        draw(&t, &mut vp, &mut layout, 30, 5);
        let top = vp.top();
        assert!(top >= 2, "precondición: la vista debe estar desplazada");
        let area = Rect::new(0, 0, 30, 5);
        assert_eq!(
            hit_cell(&layout, &vp, area, 3, 2),
            Some(CellPos {
                row: top + 2,
                col: 3
            })
        );
    }

    #[test]
    fn hit_cell_clamps_outside_the_area() {
        let t = banners(&["uno", "dos", "tres"]);
        let (mut vp, mut layout) = (Viewport::new(), Layout::new());
        draw(&t, &mut vp, &mut layout, 20, 10);
        let area = Rect::new(2, 1, 20, 8);
        // Por encima y a la izquierda: primera fila visible, columna 0.
        assert_eq!(
            hit_cell(&layout, &vp, area, 0, 0),
            Some(CellPos { row: 0, col: 0 })
        );
        // Por debajo y a la derecha: última fila del documento, última columna.
        assert_eq!(
            hit_cell(&layout, &vp, area, 500, 500),
            Some(CellPos { row: 2, col: 19 })
        );
    }

    #[test]
    fn hit_cell_none_when_empty() {
        let (mut vp, mut layout) = (Viewport::new(), Layout::new());
        draw(&Transcript::new(), &mut vp, &mut layout, 20, 5);
        assert_eq!(hit_cell(&layout, &vp, Rect::new(0, 0, 20, 5), 1, 1), None);
        draw(&banners(&["uno"]), &mut vp, &mut layout, 20, 5);
        assert_eq!(hit_cell(&layout, &vp, Rect::new(0, 0, 0, 5), 0, 0), None);
    }

    #[test]
    fn paint_selects_partial_first_and_last_rows() {
        let t = banners(&["hello", "world", "fin"]);
        let (mut vp, mut layout) = (Viewport::new(), Layout::new());
        draw(&t, &mut vp, &mut layout, 10, 4);
        let area = Rect::new(0, 0, 10, 4);
        // Desde (fila 0, col 2) hasta (fila 2, col 1, incluida).
        let sel = select(CellPos { row: 0, col: 2 }, CellPos { row: 2, col: 1 });
        let mut buf = Buffer::empty(area);
        paint_text_selection(
            &mut buf,
            area,
            &layout,
            &vp,
            &sel,
            Style::default().bg(Color::Red),
        );
        let red = |x: u16, y: u16| buf[(x, y)].bg == Color::Red;
        assert!(!red(0, 0) && !red(1, 0), "la primera fila empieza en col 2");
        assert!(red(2, 0) && red(9, 0));
        assert!(red(0, 1) && red(9, 1), "las filas intermedias van enteras");
        assert!(
            red(0, 2) && red(1, 2) && !red(2, 2),
            "la última fila incluye la col 1"
        );
        assert!(!red(0, 3), "fila fuera del documento");
    }

    #[test]
    fn paint_selection_ignores_rows_out_of_view() {
        let mut t = Transcript::new();
        for i in 0..50 {
            t.push(Line::new(LineKind::Banner, format!("line {i:02}")));
        }
        let (mut vp, mut layout) = (Viewport::new(), Layout::new());
        draw(&t, &mut vp, &mut layout, 30, 5);
        vp.scroll_up(10);
        draw(&t, &mut vp, &mut layout, 30, 5);
        let top = vp.top();
        assert!(top >= 2, "precondición: la vista debe estar desplazada");
        let area = Rect::new(0, 0, 30, 5);
        let style = Style::default().bg(Color::Red);

        let above = select(CellPos { row: 0, col: 0 }, CellPos { row: 2, col: 3 });
        let mut buf = Buffer::empty(area);
        paint_text_selection(&mut buf, area, &layout, &vp, &above, style);
        assert!(buf.content.iter().all(|c| c.bg != Color::Red));

        // Empieza antes de la vista y acaba una fila dentro: solo se ve su tramo.
        let through = select(
            CellPos {
                row: top - 2,
                col: 0,
            },
            CellPos {
                row: top + 1,
                col: 4,
            },
        );
        let mut buf = Buffer::empty(area);
        paint_text_selection(&mut buf, area, &layout, &vp, &through, style);
        let red = |x: u16, y: u16| buf[(x, y)].bg == Color::Red;
        assert!(red(0, 0) && red(28, 0), "fila superior visible entera");
        assert!(!red(29, 0), "no se pinta bajo la barra de scroll");
        assert!(red(4, 1) && !red(5, 1), "la última fila incluye la col 4");
        assert!(!red(0, 2), "la selección no llega a la tercera fila");
    }

    #[test]
    fn selected_text_multirow_trims_trailing_spaces() {
        let t = banners(&["uno", "dos", "tres"]);
        let (mut vp, mut layout) = (Viewport::new(), Layout::new());
        draw(&t, &mut vp, &mut layout, 20, 5);
        let sel = select(CellPos { row: 0, col: 1 }, CellPos { row: 2, col: 2 });
        assert_eq!(
            selected_text(&t, &layout, &Theme::test(), &sel),
            "no\ndos\ntre"
        );
    }

    #[test]
    fn selected_text_clamps_rows_past_the_end() {
        let t = banners(&["uno", "dos", "tres"]);
        let (mut vp, mut layout) = (Viewport::new(), Layout::new());
        draw(&t, &mut vp, &mut layout, 20, 5);
        let sel = select(CellPos { row: 0, col: 0 }, CellPos { row: 99, col: 3 });
        assert_eq!(
            selected_text(&t, &layout, &Theme::test(), &sel),
            "uno\ndos\ntres"
        );
    }

    #[test]
    fn selected_text_includes_wrapped_rows_joined_by_newline() {
        let long = "palabra ".repeat(10);
        let t = banners(&[long.as_str()]);
        let (mut vp, mut layout) = (Viewport::new(), Layout::new());
        draw(&t, &mut vp, &mut layout, 20, 10);
        let h = layout.height_of(0);
        assert!(h > 1, "precondición: la línea debe ajustarse");
        let width = layout.width();
        let sel = select(
            CellPos { row: 0, col: 0 },
            CellPos {
                row: h - 1,
                col: width,
            },
        );
        let out = selected_text(&t, &layout, &Theme::test(), &sel);
        assert_eq!(out.split('\n').count(), h, "una fila por línea visual");
        let squash = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        assert_eq!(squash(&out), squash(&long));
    }

    #[test]
    fn selected_text_does_not_split_wide_chars() {
        let t = banners(&["日本語", "a🚀b"]);
        let (mut vp, mut layout) = (Viewport::new(), Layout::new());
        draw(&t, &mut vp, &mut layout, 20, 5);
        let theme = Theme::test();
        let full = select(CellPos { row: 0, col: 0 }, CellPos { row: 0, col: 6 });
        assert_eq!(selected_text(&t, &layout, &theme, &full), "日本語");
        let emoji = select(CellPos { row: 1, col: 0 }, CellPos { row: 1, col: 4 });
        assert_eq!(selected_text(&t, &layout, &theme, &emoji), "a🚀b");
        // Empieza en la mitad derecha de «日»: esa celda no se copia a medias.
        let half = select(CellPos { row: 0, col: 1 }, CellPos { row: 0, col: 6 });
        assert_eq!(selected_text(&t, &layout, &theme, &half), "本語");
    }

    #[test]
    fn selected_text_empty_selection_is_empty() {
        let t = banners(&["uno"]);
        let (mut vp, mut layout) = (Viewport::new(), Layout::new());
        draw(&t, &mut vp, &mut layout, 20, 5);
        let sel = TextSelection::new(CellPos { row: 0, col: 1 });
        assert_eq!(selected_text(&t, &layout, &Theme::test(), &sel), "");
    }
}
