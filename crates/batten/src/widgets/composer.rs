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

/// Ventana visible de una línea lógica, en índices de grafema `[inicio, fin)`,
/// junto con la columna del cursor dentro de la ventana.
fn line_window(line: &str, cursor: Option<usize>, width: usize) -> (usize, usize, usize) {
    let graphemes = line.graphemes(true).collect::<Vec<_>>();
    if width == 0 {
        return (0, 0, 0);
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
        return (0, end, 0);
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
    (start, end, cursor_col)
}

/// Texto visible de una línea según `line_window`. Solo lo usan las pruebas:
/// el render trabaja con rangos de bytes sobre el texto completo.
#[cfg(test)]
fn line_view(line: &str, cursor: Option<usize>, width: usize) -> (String, usize) {
    let graphemes = line.graphemes(true).collect::<Vec<_>>();
    let (start, end, col) = line_window(line, cursor, width);
    (graphemes[start..end].concat(), col)
}

/// Índice de byte del grafema `grapheme` de `line` (o el final si no existe).
fn grapheme_byte(line: &str, grapheme: usize) -> usize {
    line.grapheme_indices(true)
        .nth(grapheme)
        .map_or(line.len(), |(i, _)| i)
}

/// Rangos de byte `[inicio, fin)` de cada línea lógica visible dentro de
/// `text`, más la columna del cursor. Los rangos apuntan al texto completo del
/// composer, así que el resaltado de la selección se calcula sobre el mismo
/// texto que el cursor. El recorte horizontal es el de `line_window`.
fn horizontal_windows(
    text: &str,
    cursor_row: usize,
    cursor_index: usize,
    width: usize,
) -> (Vec<(usize, usize)>, usize) {
    let mut graphemes_before = 0;
    let mut line_start = 0;
    let mut cursor_col = 0;
    let mut ranges = Vec::new();
    for (row, line) in text.split('\n').enumerate() {
        let cursor = (row == cursor_row).then(|| cursor_index.saturating_sub(graphemes_before));
        let (start, end, col) = line_window(line, cursor, width);
        if row == cursor_row {
            cursor_col = col;
        }
        ranges.push((
            line_start + grapheme_byte(line, start),
            line_start + grapheme_byte(line, end),
        ));
        graphemes_before += line.graphemes(true).count() + 1;
        line_start += line.len() + 1;
    }
    (ranges, cursor_col)
}

/// Divide la fila `[row_start, row_end)` de `text` en (antes, seleccionado,
/// después). `sel` es la selección en bytes del texto completo. Los tres
/// tramos son subcadenas de la fila y su unión es la fila entera; sin
/// solapamiento con la selección, el tramo seleccionado queda vacío.
fn split_selected(
    text: &str,
    row_start: usize,
    row_end: usize,
    sel: Option<(usize, usize)>,
) -> [(&str, bool); 3] {
    let plain = [(&text[row_start..row_end], false), ("", false), ("", false)];
    let Some((sel_start, sel_end)) = sel else {
        return plain;
    };
    let start = sel_start.clamp(row_start, row_end);
    let end = sel_end.clamp(row_start, row_end);
    if start == end {
        return plain;
    }
    [
        (&text[row_start..start], false),
        (&text[start..end], true),
        (&text[end..row_end], false),
    ]
}

/// Añade a `spans` los tramos no vacíos de una fila, con el estilo llano o el
/// de selección según la marca de cada tramo.
fn push_parts<'a>(
    spans: &mut Vec<Span<'a>>,
    parts: [(&'a str, bool); 3],
    plain: Style,
    selected: Style,
) {
    for (part, is_selected) in parts {
        if !part.is_empty() {
            spans.push(Span::styled(
                part,
                if is_selected { selected } else { plain },
            ));
        }
    }
}

/// Indica si el salto de línea que cierra la fila `[.., row_end)` queda dentro
/// de la selección. Se muestra como una celda seleccionada de un espacio, solo
/// en el aspecto: el texto no cambia.
fn newline_selected(text: &str, row_end: usize, sel: Option<(usize, usize)>) -> bool {
    sel.is_some_and(|(start, end)| start <= row_end && row_end < end)
        && text.as_bytes().get(row_end) == Some(&b'\n')
}

/// Fila superior visible que mantiene el cursor a la vista desplazando lo
/// mínimo desde `view_top`. Con el cursor en la última fila equivale a
/// `cursor_row - (visible - 1)`, el cálculo que se usaba antes.
fn stable_top(view_top: usize, cursor_row: usize, visible: usize, total_rows: usize) -> usize {
    let visible = visible.max(1);
    let lowest = cursor_row.saturating_sub(visible - 1);
    view_top
        .clamp(lowest, cursor_row)
        .min(total_rows.saturating_sub(visible))
        // Red de seguridad: si el cursor cae fuera de las filas, nunca lo
        // dejamos por encima de su ventana mínima.
        .max(lowest)
}

/// Columnas del prefijo de cada línea del composer Classic (`❯ ` o `│ `).
const CLASSIC_PREFIX: u16 = 2;

/// Interior del marco del composer Classic: un borde redondeado por lado. El
/// título y el pie van sobre el marco y no reducen el interior.
fn classic_inner(area: Rect) -> Rect {
    Block::default().borders(Borders::ALL).inner(area)
}

/// Rangos visibles de cada línea lógica del composer Classic y la columna del
/// cursor, para un interior de `inner_width` celdas (descontado el prefijo).
/// El render y la traducción posición→cursor usan esta misma función.
fn classic_windows(composer: &Composer, inner_width: u16) -> (Vec<(usize, usize)>, usize) {
    // Se usa el cursor pintado por última vez, no el vivo: así la traducción de
    // una celda coincide con lo que se ve aunque el cursor se haya movido.
    let cursor = composer.painted_cursor();
    horizontal_windows(
        composer.text(),
        composer.row_of_grapheme(cursor),
        cursor,
        usize::from(inner_width.saturating_sub(CLASSIC_PREFIX)),
    )
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
    let inner = classic_inner(area);
    frame.render_widget(block, area);

    let (row, _) = composer.cursor_row_col();
    let scroll = stable_top(
        composer.view_top(),
        row,
        usize::from(inner.height),
        line_count,
    );
    composer.set_view_top(scroll);
    composer.mark_painted();
    let source = composer.text();
    let (ranges, col) = classic_windows(composer, inner.width);
    let input = theme.style(StyleToken::Default);
    let selected = theme.style(StyleToken::Selection);
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
        let sel = composer.selection_bytes();
        ranges
            .iter()
            .enumerate()
            .map(|(line, &(start, end))| {
                let mut spans = Vec::with_capacity(4);
                spans.push(Span::styled(if line == 0 { "❯ " } else { "│ " }, prompt));
                push_parts(
                    &mut spans,
                    split_selected(source, start, end, sel),
                    input,
                    selected,
                );
                if newline_selected(source, end, sel) {
                    spans.push(Span::styled(" ", selected));
                }
                Line::from(spans)
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
        let x = inner.x + CLASSIC_PREFIX + col as u16;
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
    crush_layout_at(composer, composer.cursor(), width)
}

/// Como `crush_layout`, pero con el cursor indicado. El cursor influye en las
/// filas (un cursor al final de una fila llena baja a una fila propia), así que
/// la traducción de celdas usa el pintado y el render, el vivo.
fn crush_layout_at(composer: &Composer, cursor: usize, width: u16) -> ComposerLayout {
    crate::state::composer_layout(
        composer.text(),
        cursor,
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
    composer.mark_painted();
    let (cursor_row, cursor_col) = layout.cursor;
    let scroll = stable_top(composer.view_top(), cursor_row, height, layout.rows.len());
    composer.set_view_top(scroll);
    let source = composer.text();
    let sel = composer.selection_bytes();
    let selected = theme.style(StyleToken::Selection);
    let lines = layout
        .rows
        .iter()
        .skip(scroll)
        .take(height)
        .map(|row| {
            let mut spans = Vec::with_capacity(4);
            spans.push(Span::styled(crush_gutter(row), prompt));
            push_parts(
                &mut spans,
                split_selected(source, row.start, row.end, sel),
                text_style,
                selected,
            );
            if newline_selected(source, row.end, sel) {
                spans.push(Span::styled(" ", selected));
            }
            Line::from(spans)
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

/// Fila (lógica o visual) bajo una celda de un marco de `height` filas que
/// empieza en `top` y muestra `total` filas. `offset` es la distancia desde el
/// borde superior del interior. El resultado se acota a las filas visibles que
/// existen: un clic fuera del texto cae en la primera o en la última.
fn visible_index(top: usize, height: usize, total: usize, offset: usize) -> usize {
    let last_row = total.saturating_sub(1);
    let first = top.min(last_row);
    let last = top
        .saturating_add(height)
        .min(total)
        .saturating_sub(1)
        .max(first);
    top.saturating_add(offset).clamp(first, last)
}

/// Celdas que ocupa la mitad izquierda de un grafema de `width` celdas. Siempre
/// al menos una: un clic sobre la celda de un grafema estrecho, o de anchura
/// cero, deja el cursor delante de él.
fn left_half(width: usize) -> usize {
    width.div_ceil(2).max(1)
}

/// Índice de byte de la frontera de grafema que corresponde a la columna `col`
/// (en celdas) del tramo `text[start..end]`. Un clic en la mitad izquierda de un
/// grafema deja el cursor antes de él; en la derecha, después. Más allá del
/// tramo devuelve `end`.
fn boundary_in_span(text: &str, start: usize, end: usize, col: usize) -> usize {
    let mut cells = 0;
    for (offset, grapheme) in text[start..end].grapheme_indices(true) {
        let width = UnicodeWidthStr::width(grapheme);
        if col < cells + left_half(width) {
            return start + offset;
        }
        cells += width;
    }
    end
}

/// Número de grafemas que preceden al byte `byte` de `text`. Usa la misma cuenta
/// que `Composer::cursor()`, de modo que los índices son intercambiables.
fn grapheme_count(text: &str, byte: usize) -> usize {
    text[..byte].graphemes(true).count()
}

/// Traduce una celda a un índice en el composer Crush. Las filas visuales
/// visibles son `view_top..view_top + altura`, con `view_top` el valor que dejó
/// el último render (consulta pura: no lo recalcula ni lo escribe).
fn hit_crush(composer: &Composer, area: Rect, x: u16, y: u16) -> usize {
    let text = composer.text();
    let layout = crush_layout_at(composer, composer.painted_cursor(), area.width);
    let index = visible_index(
        composer.view_top(),
        usize::from(area.height),
        layout.rows.len(),
        usize::from(y.saturating_sub(area.y)),
    );
    let row = &layout.rows[index];
    let gutter = UnicodeWidthStr::width(crush_gutter(row));
    let col = usize::from(x.saturating_sub(area.x)).saturating_sub(gutter);
    grapheme_count(text, boundary_in_span(text, row.start, row.end, col))
}

/// Traduce una celda a un índice en el composer Classic. Las líneas visibles son
/// `view_top..view_top + altura` en líneas lógicas; cada una muestra el tramo que
/// fijó `classic_windows`, tras un prefijo de `CLASSIC_PREFIX` columnas.
fn hit_classic(composer: &Composer, inner: Rect, x: u16, y: u16) -> usize {
    let text = composer.text();
    let line = visible_index(
        composer.view_top(),
        usize::from(inner.height),
        composer.lines(),
        usize::from(y.saturating_sub(inner.y)),
    );
    // `classic_windows` tiene una entrada por línea lógica y `line < lines()`.
    let (ranges, _) = classic_windows(composer, inner.width);
    let (start, end) = ranges[line];
    let col = usize::from(x.saturating_sub(inner.x)).saturating_sub(usize::from(CLASSIC_PREFIX));
    grapheme_count(text, boundary_in_span(text, start, end, col))
}

/// Índice de grafema del composer bajo la celda `(x, y)` del terminal: la misma
/// unidad que `Composer::cursor()` y `select_range`. `area` es exactamente el
/// rectángulo que se pasó a `render_for_look`.
///
/// Geometría de cada look (debe coincidir con su renderer):
/// - **Crush**, sin marco. Fila visual `view_top + (y - area.y)`, acotada a las
///   filas que existen. Cada fila lleva su prefijo: `> ` o `::: ` en la primera
///   fila de la línea y sangría de igual anchura en las demás. Las celdas del
///   prefijo caen al inicio de la fila; las que quedan tras el texto, al final.
///   Al final de una fila partida por ajuste, el índice coincide con el inicio
///   de la siguiente, que es donde el render pinta el cursor. En una fila que
///   cierra un salto de línea lógico, el final queda antes del `\n`.
/// - **Classic**, marco redondeado de una celda por lado; el interior es
///   `classic_inner(area)`. Línea lógica `view_top + (y - interior.y)`, sin
///   ajuste de línea. Cada línea empieza con un prefijo de `CLASSIC_PREFIX`
///   columnas y muestra solo el tramo de `classic_windows` (recorte horizontal
///   alrededor del cursor). Las celdas del borde se acotan al interior, y las
///   posteriores al tramo, a su final.
///
/// Un clic en la mitad derecha de un carácter ancho deja el cursor después de
/// él. Devuelve `None` solo si el área es nula o, en Classic, si no queda
/// interior; un composer vacío devuelve `Some(0)`.
pub fn hit_index(theme: &Theme, composer: &Composer, area: Rect, x: u16, y: u16) -> Option<usize> {
    let inner = match theme.look.composer {
        ComposerStyle::Classic => classic_inner(area),
        ComposerStyle::Crush => area,
    };
    if area.is_empty() || inner.is_empty() {
        return None;
    }
    if composer.is_empty() {
        return Some(0);
    }
    Some(match theme.look.composer {
        ComposerStyle::Classic => hit_classic(composer, inner, x, y),
        ComposerStyle::Crush => hit_crush(composer, area, x, y),
    })
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

    #[test]
    fn split_selected_without_selection() {
        assert_eq!(
            split_selected("abc\ndef", 4, 7, None),
            [("def", false), ("", false), ("", false)]
        );
    }

    #[test]
    fn split_selected_partial_selection() {
        assert_eq!(
            split_selected("abcdef", 0, 6, Some((2, 4))),
            [("ab", false), ("cd", true), ("ef", false)]
        );
        // Los offsets son bytes: la «é» ocupa dos y el corte cae entre grafemas.
        assert_eq!(
            split_selected("héllo", 0, 6, Some((1, 3))),
            [("h", false), ("é", true), ("llo", false)]
        );
    }

    #[test]
    fn split_selected_full_row() {
        assert_eq!(
            split_selected("abc\ndef", 4, 7, Some((4, 7))),
            [("", false), ("def", true), ("", false)]
        );
    }

    #[test]
    fn split_selected_outside_the_row() {
        let text = "abc\ndef\nghi";
        let plain = [("def", false), ("", false), ("", false)];
        assert_eq!(split_selected(text, 4, 7, Some((0, 3))), plain);
        assert_eq!(split_selected(text, 4, 7, Some((8, 11))), plain);
    }

    #[test]
    fn split_selected_starts_before_the_row_and_ends_inside() {
        assert_eq!(
            split_selected("abc\ndef", 4, 7, Some((2, 5))),
            [("", false), ("d", true), ("ef", false)]
        );
    }

    /// Convierte el búfer de un terminal de prueba en una cadena por fila.
    fn screen_rows(term: &Terminal<TestBackend>, w: u16) -> Vec<String> {
        term.backend()
            .buffer()
            .content
            .chunks(usize::from(w).max(1))
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect()
    }

    /// Pinta `render_crush` y devuelve sus filas de pantalla.
    fn crush_rows(c: &Composer, w: u16, h: u16, theme: &Theme) -> Vec<String> {
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
        screen_rows(&term, w)
    }

    /// Pinta `render_focused_with_model` (Classic) y devuelve sus filas.
    fn classic_rows(c: &Composer, w: u16, h: u16, theme: &Theme) -> Vec<String> {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| render_focused_with_model(f, f.area(), c, true, "", theme))
            .unwrap();
        screen_rows(&term, w)
    }

    #[test]
    fn crush_view_top_stays_when_cursor_moves_inside_the_viewport() {
        let theme = Theme::preset(crate::widgets::ThemePreset::Crush);
        let mut c = Composer::new();
        c.set_text("1\n2\n3\n4\n5\n6\n7\n8");
        c.set_cursor(8); // línea "5", fila 4
        c.set_view_top(3);
        let _ = crush_rows(&c, 20, 3, &theme);
        assert_eq!(c.view_top(), 3);
        // Se mueve a la fila 3 (dentro de la vista): la vista no debe desplazarse.
        c.set_cursor(6);
        let rows = crush_rows(&c, 20, 3, &theme);
        assert_eq!(c.view_top(), 3);
        assert!(rows[0].contains('4'), "{rows:?}");
    }

    #[test]
    fn crush_view_top_follows_the_cursor_past_the_edges() {
        let theme = Theme::preset(crate::widgets::ThemePreset::Crush);
        let mut c = Composer::new();
        c.set_text("1\n2\n3\n4\n5\n6\n7\n8");
        c.set_view_top(3);
        c.set_cursor(0); // por encima de la vista
        let rows = crush_rows(&c, 20, 3, &theme);
        assert_eq!(c.view_top(), 0);
        assert!(rows[0].contains('1'), "{rows:?}");
        c.set_cursor(14); // línea "8", por debajo de la vista
        let rows = crush_rows(&c, 20, 3, &theme);
        assert_eq!(c.view_top(), 5);
        assert!(rows[2].contains('8'), "{rows:?}");
    }

    #[test]
    fn crush_typing_at_the_end_scrolls_like_before() {
        let theme = Theme::preset(crate::widgets::ThemePreset::Crush);
        let mut c = Composer::new();
        for step in 0..10 {
            c.insert_str(&format!("{step}\n"));
            let _ = crush_rows(&c, 20, 3, &theme);
            let row = crush_layout(&c, 20).cursor.0;
            // Cálculo anterior a la tarea: el cursor queda en la última fila.
            assert_eq!(c.view_top(), row.saturating_sub(2), "paso {step}");
        }
    }

    #[test]
    fn classic_view_top_stays_when_cursor_moves_inside_the_viewport() {
        let theme = Theme::test();
        let mut c = Composer::new();
        c.set_text("1\n2\n3\n4\n5\n6\n7\n8");
        c.set_cursor(8); // línea "5", fila lógica 4
        c.set_view_top(3);
        let _ = classic_rows(&c, 20, 5, &theme); // 3 filas útiles dentro del marco
        assert_eq!(c.view_top(), 3);
        c.set_cursor(6); // línea "4", dentro de la vista
        let rows = classic_rows(&c, 20, 5, &theme);
        assert_eq!(c.view_top(), 3);
        assert!(rows[1].contains('4'), "{rows:?}");
    }

    #[test]
    fn classic_typing_at_the_end_scrolls_like_before() {
        let theme = Theme::test();
        let mut c = Composer::new();
        for step in 0..10 {
            c.insert_str(&format!("{step}\n"));
            let _ = classic_rows(&c, 20, 5, &theme);
            let (row, _) = c.cursor_row_col();
            assert_eq!(c.view_top(), row.saturating_sub(2), "paso {step}");
        }
    }

    #[test]
    fn crush_selection_is_highlighted_with_the_selection_style() {
        let theme = Theme::preset(crate::widgets::ThemePreset::Crush);
        let sel_bg = theme.style(StyleToken::Selection).bg;
        assert!(sel_bg.is_some());
        let mut c = Composer::new();
        c.set_text("uno dos");
        c.select_range(0, 3); // "uno"
        let mut term = Terminal::new(TestBackend::new(20, 3)).unwrap();
        term.draw(|f| {
            render_crush(f, f.area(), &c, true, "", &theme);
        })
        .unwrap();
        let buf = term.backend().buffer();
        for x in 2..5 {
            assert_eq!(buf[(x, 0)].bg, sel_bg.unwrap(), "celda {x} seleccionada");
        }
        for x in 5..8 {
            assert_ne!(buf[(x, 0)].bg, sel_bg.unwrap(), "celda {x} sin seleccionar");
        }
    }

    #[test]
    fn classic_selection_is_highlighted() {
        let theme = Theme::test();
        let sel_bg = theme.style(StyleToken::Selection).bg;
        assert!(sel_bg.is_some());
        let mut c = Composer::new();
        c.set_text("ab\ncd");
        c.select_range(1, 4); // "b\nc"
        let mut term = Terminal::new(TestBackend::new(20, 4)).unwrap();
        term.draw(|f| render_focused_with_model(f, f.area(), &c, true, "", &theme))
            .unwrap();
        let buf = term.backend().buffer();
        let selected = |x: u16, y: u16| buf[(x, y)].bg == sel_bg.unwrap();
        // Fila 1: "❯ ab" (la «b» y el salto de línea seleccionado, tras él no).
        assert!(!selected(3, 1), "la «a» no está seleccionada");
        assert!(selected(4, 1), "la «b» sí");
        assert!(selected(5, 1), "el salto de línea se marca con una celda");
        assert!(!selected(6, 1));
        // Fila 2: "│ cd" (solo la «c»).
        assert!(selected(3, 2));
        assert!(!selected(4, 2));
    }

    /// Pinta `render_for_look` con el cursor puesto y devuelve la posición del
    /// cursor que fijó el renderer.
    fn cursor_after_render(c: &Composer, area: Rect, theme: &Theme) -> Option<(u16, u16)> {
        let mut term = Terminal::new(TestBackend::new(area.right(), area.bottom())).unwrap();
        term.draw(|f| render_for_look(f, area, c, true, "", theme))
            .unwrap();
        term.backend_mut()
            .get_cursor_position()
            .ok()
            .map(|p| (p.x, p.y))
    }

    #[test]
    fn hit_crush_first_row_start_and_end() {
        let theme = Theme::preset(crate::widgets::ThemePreset::Crush);
        let mut c = Composer::new();
        c.set_text("abc");
        let area = Rect::new(2, 1, 20, 3);
        // Prefijo "> " en las celdas x = 2 y 3: cae al inicio de la fila.
        assert_eq!(hit_index(&theme, &c, area, 2, 1), Some(0));
        assert_eq!(hit_index(&theme, &c, area, 3, 1), Some(0));
        assert_eq!(hit_index(&theme, &c, area, 4, 1), Some(0));
        // Un carácter estrecho: su celda deja el cursor delante de él.
        assert_eq!(hit_index(&theme, &c, area, 5, 1), Some(1));
        assert_eq!(hit_index(&theme, &c, area, 6, 1), Some(2));
        // Más allá del final de la fila: tras la última «c».
        assert_eq!(hit_index(&theme, &c, area, 7, 1), Some(3));
        assert_eq!(hit_index(&theme, &c, area, 19, 1), Some(3));
    }

    #[test]
    fn hit_crush_wrapped_rows() {
        let theme = Theme::preset(crate::widgets::ThemePreset::Crush);
        let mut c = Composer::new();
        // Ancho 5: el prefijo deja 3 celdas en la primera fila → "ab" + "日c".
        c.set_text("ab日c");
        let area = Rect::new(0, 0, 5, 3);
        assert_eq!(hit_index(&theme, &c, area, 3, 0), Some(1)); // «b»
                                                                // Fin de la fila 0 = inicio de la fila 1: la posición es la misma.
        assert_eq!(hit_index(&theme, &c, area, 4, 0), Some(2));
        // Sangría "  " de la fila 1, sobre el «日».
        assert_eq!(hit_index(&theme, &c, area, 2, 1), Some(2));
        assert_eq!(hit_index(&theme, &c, area, 3, 1), Some(3)); // mitad derecha
        assert_eq!(hit_index(&theme, &c, area, 4, 1), Some(3)); // «c»
    }

    #[test]
    fn hit_crush_respects_view_top() {
        let theme = Theme::preset(crate::widgets::ThemePreset::Crush);
        let mut c = Composer::new();
        c.set_text("1\n2\n3\n4\n5\n6\n7\n8"); // el cursor queda en la línea "8"
        let area = Rect::new(0, 0, 20, 3);
        let _ = cursor_after_render(&c, area, &theme);
        assert_eq!(c.view_top(), 5, "el render deja la vista en la fila 5");
        assert_eq!(hit_index(&theme, &c, area, 4, 0), Some(10)); // «6», inicio
        assert_eq!(hit_index(&theme, &c, area, 5, 0), Some(11)); // tras la «6»
        assert_eq!(hit_index(&theme, &c, area, 4, 1), Some(12)); // «7»
        assert_eq!(hit_index(&theme, &c, area, 4, 2), Some(14)); // «8»
        assert_eq!(hit_index(&theme, &c, area, 19, 2), Some(15)); // fin del texto
    }

    #[test]
    fn hit_crush_wide_chars() {
        let theme = Theme::preset(crate::widgets::ThemePreset::Crush);
        let mut c = Composer::new();
        c.set_text("日本語");
        let area = Rect::new(0, 0, 10, 3);
        assert_eq!(hit_index(&theme, &c, area, 2, 0), Some(0)); // mitad izquierda de «日»
        assert_eq!(hit_index(&theme, &c, area, 3, 0), Some(1)); // mitad derecha de «日»
        assert_eq!(hit_index(&theme, &c, area, 4, 0), Some(1)); // mitad izquierda de «本»
        assert_eq!(hit_index(&theme, &c, area, 5, 0), Some(2));
        assert_eq!(hit_index(&theme, &c, area, 7, 0), Some(3));
        assert_eq!(hit_index(&theme, &c, area, 9, 0), Some(3)); // más allá del final

        // Un grafema combinado ocupa una sola celda.
        c.set_text("e\u{301}x");
        assert_eq!(hit_index(&theme, &c, area, 2, 0), Some(0));
        assert_eq!(hit_index(&theme, &c, area, 3, 0), Some(1));
        assert_eq!(hit_index(&theme, &c, area, 4, 0), Some(2));
    }

    #[test]
    fn hit_crush_prefix_cells_map_to_row_start() {
        let theme = Theme::preset(crate::widgets::ThemePreset::Crush);
        let mut c = Composer::new();
        c.set_text("uno\ndos");
        let area = Rect::new(0, 0, 20, 3);
        assert_eq!(hit_index(&theme, &c, area, 0, 0), Some(0)); // "> "
        assert_eq!(hit_index(&theme, &c, area, 1, 0), Some(0));
        assert_eq!(hit_index(&theme, &c, area, 0, 1), Some(4)); // "::: "
        assert_eq!(hit_index(&theme, &c, area, 3, 1), Some(4));
    }

    #[test]
    fn hit_crush_out_of_range_is_clamped() {
        let theme = Theme::preset(crate::widgets::ThemePreset::Crush);
        let mut c = Composer::new();
        c.set_text("1\n2");
        let area = Rect::new(0, 1, 20, 3);
        assert_eq!(hit_index(&theme, &c, area, 2, 0), Some(0)); // por encima del área
        assert_eq!(hit_index(&theme, &c, area, 4, 4), Some(2)); // por debajo: última fila
        assert_eq!(hit_index(&theme, &c, area, 19, 1), Some(1)); // fin de la primera
        assert_eq!(
            hit_index(&theme, &c, area, u16::MAX, u16::MAX),
            Some(3),
            "esquina inferior derecha: fin del texto"
        );
    }

    #[test]
    fn hit_classic_single_and_multiline() {
        let theme = Theme::test();
        let mut c = Composer::new();
        c.set_text("abc\ndef");
        // Interior (1,1)-(18,3): el prefijo ocupa las celdas x = 1 y 2.
        let area = Rect::new(0, 0, 20, 5);
        assert_eq!(hit_index(&theme, &c, area, 3, 1), Some(0));
        assert_eq!(hit_index(&theme, &c, area, 5, 1), Some(2)); // antes de la «c»
        assert_eq!(hit_index(&theme, &c, area, 10, 1), Some(3)); // fin de la línea 0
        assert_eq!(hit_index(&theme, &c, area, 3, 2), Some(4)); // inicio de la línea 1
        assert_eq!(hit_index(&theme, &c, area, 5, 2), Some(6)); // antes de la «f»
    }

    #[test]
    fn classic_hit_follows_the_cursor_painted_last() {
        let theme = Theme::test();
        let mut c = Composer::new();
        // Línea más ancha que el interior: el recorte horizontal sigue al cursor.
        c.set_text("abcdefghijklmnopqrstuvwxyz0123456789");
        let area = Rect::new(0, 0, 20, 5);
        let _ = cursor_after_render(&c, area, &theme);
        // Primera celda visible de la ventana pintada (prefijo de 2 columnas).
        let first = hit_index(&theme, &c, area, 3, 1).unwrap();
        assert!(first > 0, "el render recorta la línea alrededor del cursor");
        // Movemos el cursor sin repintar: la ventana pintada no debe cambiar.
        c.set_cursor(0);
        for col in 0..15 {
            assert_eq!(
                hit_index(&theme, &c, area, 3 + col, 1),
                Some(first + usize::from(col)),
                "la celda {col} debe corresponder a lo último pintado"
            );
        }
    }

    #[test]
    fn crush_hit_follows_the_layout_painted_last() {
        let theme = Theme::preset(crate::widgets::ThemePreset::Crush);
        let mut c = Composer::new();
        // Con el cursor al final de una fila llena, el layout añade una fila propia.
        c.set_text("abcd");
        let area = Rect::new(0, 0, 6, 3);
        let _ = cursor_after_render(&c, area, &theme);
        c.set_cursor(0);
        // La fila 1 es la del cursor pintado: su sangría de 2 celdas cae al final.
        assert_eq!(hit_index(&theme, &c, area, 2, 1), Some(4));
    }

    #[test]
    fn hit_classic_border_cells_are_clamped() {
        let theme = Theme::test();
        let mut c = Composer::new();
        c.set_text("abc\ndef");
        let area = Rect::new(0, 0, 20, 5);
        assert_eq!(hit_index(&theme, &c, area, 0, 0), Some(0)); // esquina superior izquierda
        assert_eq!(hit_index(&theme, &c, area, 0, 4), Some(4)); // borde inferior: última línea
        assert_eq!(hit_index(&theme, &c, area, 19, 0), Some(3)); // borde derecho, línea 0
        assert_eq!(hit_index(&theme, &c, area, 19, 2), Some(7)); // borde derecho, línea 1
        assert_eq!(
            hit_index(&theme, &c, area, u16::MAX, u16::MAX),
            Some(7),
            "esquina inferior derecha"
        );
    }

    #[test]
    fn hit_classic_horizontal_scroll() {
        let theme = Theme::test();
        let mut c = Composer::new();
        c.set_text("abcdefghijklmnop"); // cursor al final: la vista muestra "lmnop"
        let area = Rect::new(0, 0, 10, 3); // interior de 8 celdas, 6 para el texto
        assert_eq!(hit_index(&theme, &c, area, 3, 1), Some(11)); // «l», primera visible
        assert_eq!(hit_index(&theme, &c, area, 5, 1), Some(13)); // antes de la «n»
        assert_eq!(hit_index(&theme, &c, area, 7, 1), Some(15)); // antes de la «p»
        assert_eq!(hit_index(&theme, &c, area, 8, 1), Some(16)); // fin de la línea
    }

    #[test]
    fn hit_empty_composer_is_zero() {
        let c = Composer::new();
        let area = Rect::new(0, 0, 20, 3);
        for theme in [
            Theme::test(),
            Theme::preset(crate::widgets::ThemePreset::Crush),
        ] {
            assert_eq!(hit_index(&theme, &c, area, 5, 1), Some(0));
            assert_eq!(hit_index(&theme, &c, area, 0, 0), Some(0));
        }
    }

    #[test]
    fn hit_with_empty_area_is_none() {
        let mut c = Composer::new();
        c.set_text("abc");
        let crush = Theme::preset(crate::widgets::ThemePreset::Crush);
        let classic = Theme::test();
        assert_eq!(hit_index(&crush, &c, Rect::new(0, 0, 0, 0), 0, 0), None);
        assert_eq!(hit_index(&crush, &c, Rect::new(0, 0, 20, 0), 0, 0), None);
        assert_eq!(hit_index(&classic, &c, Rect::new(0, 0, 0, 0), 0, 0), None);
        // Classic sin interior: el marco ocupa toda el área.
        assert_eq!(hit_index(&classic, &c, Rect::new(0, 0, 2, 5), 0, 0), None);
        assert_eq!(hit_index(&classic, &c, Rect::new(0, 0, 20, 2), 0, 0), None);
    }

    #[test]
    fn hit_and_cursor_agree() {
        // Texto con ajuste (Crush), recorte horizontal y desplazamiento vertical
        // (Classic), con caracteres anchos, emoji y acentos combinados.
        let text = "ab cdefghijklmnopqrstuvwxyz\nlínea 2 日本語 😀 👨‍👩‍👧 fin\n\nx";
        let cases = [
            (Theme::test(), Rect::new(0, 0, 16, 4)),
            (
                Theme::preset(crate::widgets::ThemePreset::Crush),
                Rect::new(1, 1, 12, 4),
            ),
        ];
        for (theme, area) in cases {
            let mut c = Composer::new();
            c.set_text(text);
            let total = text.graphemes(true).count();
            // Ida y vuelta: el desplazamiento se prueba subiendo y bajando.
            for pos in (0..=total).chain((0..=total).rev()) {
                c.set_cursor(pos);
                let (x, y) = cursor_after_render(&c, area, &theme)
                    .unwrap_or_else(|| panic!("sin cursor pintado en {pos}"));
                let inside = x >= area.x && x < area.right() && y >= area.y && y < area.bottom();
                assert!(
                    inside,
                    "{:?}: cursor {pos} fuera del área en ({x}, {y})",
                    theme.look.composer
                );
                assert_eq!(
                    hit_index(&theme, &c, area, x, y),
                    Some(pos),
                    "{:?}: el cursor {pos} se pintó en ({x}, {y})",
                    theme.look.composer
                );
            }
        }
    }
}
