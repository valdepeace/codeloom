//! Showcase navegable de los widgets de batten.
//! Uso: `cargo run -p batten --example showcase`

mod app;
mod demo;
mod demos;
mod matrix;
mod prompt_keys;
mod theme_view;

use app::{route, Action, Focus, IndexRow, Showcase};
use batten::widgets::{Anim, Theme, FRAME_INTERVAL};
use matrix::{draw_matrix, matrix_cell_at, matrix_name_at, Cell};
use ratatui::crossterm::event::{self, Event, MouseButton, MouseEventKind};
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;
use std::time::{Duration, Instant};

const INDEX_WIDTH: u16 = 34;
const WIDE_MIN: u16 = 70;
/// Columnas entre dos pestañas de tema.
const TAB_GAP: u16 = 1;
/// Filas de cabecera del panel de contenido (cabecera del demo o de la matriz).
const HEADER_HEIGHT: u16 = 3;
/// Pasos del índice por cada muesca de la rueda del ratón.
const WHEEL_STEPS: usize = 3;

fn main() -> std::io::Result<()> {
    let mut show = Showcase::new();
    let mut active_preset = show.theme_preset();
    let mut theme = Theme::preset(active_preset);
    let mut anim = Anim::new();
    let mut terminal = ratatui::init();
    ratatui::crossterm::execute!(std::io::stdout(), event::EnableMouseCapture)?;
    let mut last_tick = Instant::now();

    while !show.should_quit() {
        if last_tick.elapsed() >= FRAME_INTERVAL {
            anim.advance();
            show.tick(&anim);
            last_tick = Instant::now();
        }
        terminal.draw(|f| draw(f, &mut show, &anim, &theme))?;
        if event::poll(Duration::from_millis(16))? {
            match event::read()? {
                Event::Key(k) => {
                    if route(&mut show, k.kind, k.code, k.modifiers) == Action::Quit {
                        break;
                    }
                    let preset = show.theme_preset();
                    if preset != active_preset {
                        active_preset = preset;
                        theme = Theme::preset(preset);
                    }
                }
                Event::Mouse(mouse)
                    if matches!(
                        mouse.kind,
                        MouseEventKind::Down(MouseButton::Left)
                            | MouseEventKind::ScrollUp
                            | MouseEventKind::ScrollDown
                    ) =>
                {
                    let screen = terminal.size()?.into();
                    handle_mouse(&mut show, mouse.kind, mouse.column, mouse.row, screen);
                }
                _ => {}
            }
        }
    }

    ratatui::crossterm::execute!(std::io::stdout(), event::DisableMouseCapture)?;
    ratatui::restore();
    Ok(())
}

fn draw(f: &mut Frame, show: &mut Showcase, _anim: &Anim, theme: &Theme) {
    let area = f.area();
    if area.width == 0 || area.height == 0 {
        return;
    }
    f.render_widget(
        Block::default().style(theme.style(batten::state::StyleToken::Default)),
        area,
    );
    let footer_lines = footer_texts(show, area.width);
    let geo = geometry(area, footer_height(show));
    draw_filters(f, geo.tabs, geo.query, show, theme);

    // En la matriz no se consulta `fullscreen()`: el panel de contenido es siempre la matriz.
    let matrix_mode = show.focus() == Focus::Matrix;
    let fullscreen = !matrix_mode && show.current().is_some_and(|demo| demo.fullscreen());

    if let Some(ia) = geo.index {
        draw_index(f, ia, show, theme);
        if !fullscreen {
            draw_header(f, geo.header, show, theme);
        }
    } else if !fullscreen {
        draw_collapsed(f, geo.header, show, theme);
    }

    if matrix_mode {
        draw_matrix(f, geo.content, show, theme);
    } else if let Some(demo) = show.current() {
        demo.render(
            f,
            if demo.fullscreen() {
                geo.panel
            } else {
                geo.content
            },
            theme,
        );
    }
    draw_footer(f, geo.footer, &footer_lines, theme);
}

/// Áreas de la pantalla. Es la única fuente de la geometría: `draw` y `handle_mouse` la usan, así
/// que el clic cae siempre en lo que se ve.
struct Geometry {
    /// Fila de pestañas de tema.
    tabs: Rect,
    /// Fila del filtro de texto.
    query: Rect,
    /// Columna del índice; `None` en terminal estrecha, donde no hay índice.
    index: Option<Rect>,
    /// Panel de contenido completo: cabecera y contenido.
    panel: Rect,
    /// Cabecera del panel (demo o matriz).
    header: Rect,
    /// Contenido del panel: el demo, o la matriz.
    content: Rect,
    /// Pie, de una o dos líneas.
    footer: Rect,
}

/// Reparte la pantalla en áreas. `footer_height` es la altura del pie (`footer_height(show)`).
fn geometry(screen: Rect, footer_height: u16) -> Geometry {
    let [top, body, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(footer_height),
    ])
    .areas(screen);
    let [tabs, query] = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(top);

    let (index, panel) = if screen.width >= WIDE_MIN {
        let cols =
            Layout::horizontal([Constraint::Length(INDEX_WIDTH), Constraint::Min(1)]).split(body);
        (Some(cols[0]), cols[1])
    } else {
        (None, body)
    };
    let [header, content] =
        Layout::vertical([Constraint::Length(HEADER_HEIGHT), Constraint::Min(1)]).areas(panel);

    Geometry {
        tabs,
        query,
        index,
        panel,
        header,
        content,
        footer,
    }
}

/// Ancho de una pestaña de tema: el nombre con un espacio a cada lado.
fn tab_width(name: &str) -> u16 {
    name.chars().count() as u16 + 2
}

/// Posición de cada pestaña de tema visible en la fila de pestañas de un área de `width`
/// columnas: índice en `matrix::COLUMNS`, x de inicio y ancho.
///
/// Las pestañas van separadas por `TAB_GAP`. Una que no cabe entera se recorta a lo que queda, y
/// las siguientes no aparecen. El dibujo y el ratón usan esta misma función, así que no pueden
/// divergir.
pub(crate) fn tab_spans_layout(width: u16) -> Vec<(usize, u16, u16)> {
    let mut spans = Vec::new();
    let mut x = 0u16;
    for (column, (name, _)) in matrix::COLUMNS.iter().enumerate() {
        if x >= width {
            break;
        }
        let full = tab_width(name);
        let shown = full.min(width - x);
        spans.push((column, x, shown));
        if shown < full {
            break;
        }
        x = x.saturating_add(full).saturating_add(TAB_GAP);
    }
    spans
}

fn draw_filters(f: &mut Frame, tabs_area: Rect, query_area: Rect, show: &Showcase, theme: &Theme) {
    draw_tabs(f, tabs_area, show, theme);

    let muted = theme.style(batten::state::StyleToken::Muted);
    let cursor = if show.focus() == Focus::TextFilter {
        "▏"
    } else {
        ""
    };
    let query = if show.query().is_empty() && show.focus() != Focus::TextFilter {
        "componente · ejemplo".to_string()
    } else {
        show.query().to_string()
    };
    let query_style = if show.focus() == Focus::TextFilter {
        theme.style(batten::state::StyleToken::Heading)
    } else {
        muted
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Filter: ", muted),
            Span::styled(query, query_style),
            Span::styled(cursor, theme.style(batten::state::StyleToken::Accent)),
        ])),
        query_area,
    );
}

/// Fila de pestañas: los temas de `matrix::COLUMNS`, la activa en negrita y en vídeo inverso.
fn draw_tabs(f: &mut Frame, area: Rect, show: &Showcase, theme: &Theme) {
    let active = show.theme_column();
    let muted = theme.style(batten::state::StyleToken::Muted);
    let active_style = theme
        .style(batten::state::StyleToken::Accent)
        .add_modifier(Modifier::BOLD | Modifier::REVERSED);
    let mut spans = Vec::new();
    let mut x = 0u16;
    for (column, x0, width) in tab_spans_layout(area.width) {
        if x0 > x {
            spans.push(Span::raw(" ".repeat(usize::from(x0 - x))));
        }
        let Some((name, _)) = matrix::COLUMNS.get(column) else {
            break;
        };
        let style = if column == active {
            active_style
        } else {
            muted
        };
        let text = matrix::fit(&format!(" {name} "), usize::from(width));
        spans.push(Span::styled(text, style));
        x = x0 + width;
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Atiende un evento de ratón sobre la geometría de `screen`: clic izquierdo y rueda.
///
/// Con `Focus::Matrix` la rueda mueve el cursor de la matriz, esté donde esté el puntero, y un
/// clic fuera de la tabla no cambia el foco ni cierra la matriz. Fuera de la matriz, la rueda solo
/// actúa sobre el índice.
fn handle_mouse(show: &mut Showcase, kind: MouseEventKind, column: u16, row: u16, screen: Rect) {
    if screen.width == 0 || screen.height == 0 {
        return;
    }
    // La altura del pie sale del estado actual, el mismo que pintó el último `draw`.
    let geo = geometry(screen, footer_height(show));
    match kind {
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
            let up = kind == MouseEventKind::ScrollUp;
            if show.focus() == Focus::Matrix {
                show.matrix_move(if up { -1 } else { 1 }, 0);
            } else if geo.index.is_some_and(|area| inside(area, column, row)) {
                for _ in 0..WHEEL_STEPS {
                    if up {
                        show.prev();
                    } else {
                        show.next();
                    }
                }
            }
        }
        MouseEventKind::Down(MouseButton::Left) => click(show, &geo, column, row),
        _ => {}
    }
}

/// `true` si el punto (`column`, `row`) cae dentro de `area`.
fn inside(area: Rect, column: u16, row: u16) -> bool {
    area.contains(Position::new(column, row))
}

/// Resuelve un clic izquierdo sobre la geometría de `geo`.
fn click(show: &mut Showcase, geo: &Geometry, column: u16, row: u16) {
    if inside(geo.tabs, column, row) {
        if let Some(theme) = tab_at(geo.tabs, column) {
            show.set_theme_column(theme);
            show.set_focus(Focus::Index);
        }
        return;
    }
    if inside(geo.query, column, row) {
        show.set_focus(Focus::TextFilter);
        return;
    }
    if let Some(index) = geo.index.filter(|area| inside(*area, column, row)) {
        index_click(show, index, row);
        return;
    }
    if show.focus() == Focus::Matrix {
        matrix_click(show, geo.content, column, row);
        return;
    }
    if inside(geo.panel, column, row) {
        show.set_focus(Focus::Demo);
    }
}

/// Índice de la pestaña de tema bajo `column`, si esa columna pinta una (o parte de una).
fn tab_at(tabs: Rect, column: u16) -> Option<usize> {
    let x = column - tabs.x;
    tab_spans_layout(tabs.width)
        .into_iter()
        .find(|&(_, x0, shown)| x >= x0 && x < x0 + shown)
        .map(|(theme, _, _)| theme)
}

/// Clic en el índice: selecciona y abre la fila, salvo que sea cabecera, aviso o hueco.
fn index_click(show: &mut Showcase, area: Rect, row: u16) {
    let offset = usize::from(row - area.y);
    let (start, visible) = show.visible_index_rows(area.height);
    let total = show.index_rows().len();
    if index_notice(start, visible.len(), total, offset).is_some() {
        return;
    }
    if let Some(IndexRow {
        catalog_index: Some(index),
        ..
    }) = visible.get(offset)
    {
        show.select(*index, true);
    }
}

/// Clic en la matriz (solo con `Focus::Matrix`): una celda mueve el cursor a ella; el nombre de
/// una fila lo lleva a esa fila, en la columna en la que ya estaba. Nada abre.
fn matrix_click(show: &mut Showcase, content: Rect, column: u16, row: u16) {
    if let Some((matrix_row, matrix_column)) = matrix_cell_at(content, show, column, row) {
        show.matrix_set_cursor(matrix_row, matrix_column);
    } else if let Some(matrix_row) = matrix_name_at(content, show, column, row) {
        let (_, current_column) = show.matrix_cursor();
        show.matrix_set_cursor(matrix_row, current_column);
    }
}

/// Aviso que ocupa la fila `offset` (0 = primera visible) del índice, si la hay. `start` es el
/// primer visible, `visible` el número de filas visibles y `total` el de filas del índice.
/// El dibujo y el ratón usan esta misma regla: un aviso no es seleccionable.
fn index_notice(start: usize, visible: usize, total: usize, offset: usize) -> Option<&'static str> {
    if offset == 0 && start > 0 {
        return Some("  ↑ más resultados");
    }
    // La última fila visible avisa si queda lista por debajo. La condición anterior comparaba
    // `start + offset + 1` con `total` y a la vez exigía `start + visible < total`, dos cosas que
    // no pueden cumplirse juntas, de modo que este aviso no salía nunca.
    if offset + 1 == visible && start + visible < total {
        return Some("  ↓ más resultados");
    }
    None
}

fn draw_index(f: &mut Frame, area: Rect, show: &Showcase, theme: &Theme) {
    let (start, rows) = show.visible_index_rows(area.height);
    let all_rows = show.index_rows();
    let muted = theme.style(batten::state::StyleToken::Muted);
    let mut lines = Vec::with_capacity(rows.len());
    for (offset, row) in rows.iter().enumerate() {
        if let Some(notice) = index_notice(start, rows.len(), all_rows.len(), offset) {
            lines.push(Line::styled(notice, muted));
            continue;
        }
        let mut spans = Vec::new();
        // El símbolo del componente va aparte, con su estilo de `draw_matrix`.
        let symbol = |row: &IndexRow| {
            let cell = if row.generic {
                Cell::Generic
            } else {
                Cell::Own(1)
            };
            let (glyph, style) = matrix::glyph(cell, theme);
            Span::styled(format!("{glyph} "), style)
        };
        match row.catalog_index {
            Some(i) => {
                let selected = i == show.selected();
                let focused = selected && show.focus() == Focus::Index;
                let marker = if focused {
                    "▸ "
                } else if selected {
                    "• "
                } else {
                    "  "
                };
                let mut style = theme.style(batten::state::StyleToken::Default);
                if focused {
                    style = style.add_modifier(Modifier::BOLD);
                }
                spans.push(Span::styled(format!("  {marker}"), style));
                if row.marked {
                    spans.push(symbol(row));
                }
                spans.push(Span::styled(row.text.clone(), style));
            }
            None => {
                // Cabecera de componente con varios ejemplos, o el aviso «Sin resultados».
                spans.push(Span::styled("  ", muted));
                if row.marked {
                    spans.push(symbol(row));
                }
                spans.push(Span::styled(row.text.clone(), muted));
            }
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), area);
}

fn draw_header(f: &mut Frame, area: Rect, show: &Showcase, theme: &Theme) {
    let component = show.component_at(show.selected()).unwrap_or("");
    let variant = show.variant_at(show.selected()).unwrap_or("");
    let hint = show.current_hint();
    let lines = vec![
        Line::styled(
            component.to_string(),
            theme
                .style(batten::state::StyleToken::Heading)
                .add_modifier(Modifier::BOLD),
        ),
        Line::styled(
            format!("{variant} · {hint}"),
            theme.style(batten::state::StyleToken::Muted),
        ),
        Line::styled(
            help(show.focus()),
            theme.style(batten::state::StyleToken::Muted),
        ),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

/// Ayuda de teclas de una línea para cada foco; la usan la cabecera y la versión estrecha.
fn help(focus: Focus) -> &'static str {
    match focus {
        Focus::Index => "↑/↓: ejemplo · ←/→: tema · Tab: abrir · /: filtrar · m: matriz · q: salir",
        Focus::Demo => "F7: tema · Tab/Esc: volver",
        Focus::Matrix => "←/→/↑/↓: celda · Enter: abrir · Esc: índice · F7: paleta",
        Focus::TextFilter => "Escribe para filtrar · Enter: índice · Esc: limpiar",
    }
}

fn draw_collapsed(f: &mut Frame, area: Rect, show: &Showcase, theme: &Theme) {
    let component = show.component_at(show.selected()).unwrap_or("");
    let variant = show.variant_at(show.selected()).unwrap_or("");
    let lines = vec![
        Line::styled(
            component.to_string(),
            theme.style(batten::state::StyleToken::Heading),
        ),
        Line::styled(
            format!("{variant} · {}", show.current_hint()),
            theme.style(batten::state::StyleToken::Muted),
        ),
        Line::styled(
            help(show.focus()),
            theme.style(batten::state::StyleToken::Muted),
        ),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

/// Textos del pie, una línea cada uno, para un área de `width` columnas.
///
/// La línea 1 da el tema, el recuento de sus componentes (propios `●`, genéricos `◐`) y cuántos
/// faltan. La línea 2 existe solo si el componente seleccionado es genérico: explica el estado.
/// El sufijo de F7 se añade solo si la línea 1 cabe entera con él.
fn footer_texts(show: &Showcase, width: u16) -> Vec<String> {
    let view = show.view();
    let generic = view.entries.iter().filter(|entry| entry.generic).count();
    let own = view.entries.len() - generic;
    let (name, _) = matrix::COLUMNS[show.theme_column()];
    let base = format!(
        "Tema: {name} · {} componentes: {own} ● · {generic} ◐ · faltan {} (m: matriz)",
        view.entries.len(),
        view.missing
    );
    let with_keys = format!("{base} · F7/Shift+F7 tema");
    let line = if with_keys.chars().count() <= usize::from(width) {
        with_keys
    } else {
        base
    };
    let mut lines = vec![matrix::fit(&line, usize::from(width))];
    if let Some(component) = footer_note_component(show) {
        let note = matrix::status_note(component, show.theme_column(), Cell::Generic);
        lines.push(matrix::fit(&note, usize::from(width)));
    }
    lines
}

/// Componente seleccionado si es genérico en el tema activo: solo entonces el pie tiene segunda
/// línea (la nota de estado). Es la única condición de la altura del pie.
fn footer_note_component(show: &Showcase) -> Option<&str> {
    let component = show.component_at(show.selected())?;
    let generic = show
        .view()
        .entries
        .iter()
        .find(|entry| entry.component == component)
        .is_some_and(|entry| entry.generic);
    generic.then_some(component)
}

/// Altura del pie, en filas: 2 si el componente seleccionado es genérico, 1 si no.
fn footer_height(show: &Showcase) -> u16 {
    if footer_note_component(show).is_some() {
        2
    } else {
        1
    }
}

/// Dibuja las líneas del pie, una por fila del área.
fn draw_footer(f: &mut Frame, area: Rect, lines: &[String], theme: &Theme) {
    let muted = theme.style(batten::state::StyleToken::Muted);
    let rows: Vec<Line> = lines
        .iter()
        .map(|line| Line::styled(line.as_str(), muted))
        .collect();
    f.render_widget(Paragraph::new(rows), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
    use ratatui::Terminal;

    #[test]
    fn draw_does_not_panic_at_any_size() {
        let anim = Anim::new();
        let mut show = Showcase::new();
        let theme = Theme::preset(show.theme_preset());
        for (w, h) in [(0u16, 0u16), (1, 1), (60, 20), (120, 40)] {
            let backend = TestBackend::new(w, h);
            let mut term = Terminal::new(backend).unwrap();
            term.draw(|f| draw(f, &mut show, &anim, &theme)).unwrap();
        }
    }

    /// Cabecera de columnas de la matriz tal como se dibuja (sin el hueco del nombre).
    const HEADER: &str = "Batten Martty Mindfork Pawan Crush MastraCode CodeLoom";

    fn press(show: &mut Showcase, code: KeyCode) {
        route(show, KeyEventKind::Press, code, KeyModifiers::NONE);
    }

    /// Dibuja el showcase completo en un buffer de `w`×`h`.
    fn render(show: &mut Showcase, w: u16, h: u16) -> Buffer {
        let anim = Anim::new();
        let theme = Theme::preset(show.theme_preset());
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| draw(f, show, &anim, &theme)).unwrap();
        term.backend().buffer().clone()
    }

    /// Líneas de texto del buffer desde la columna `x0`.
    fn lines(buffer: &Buffer, x0: u16) -> Vec<String> {
        (0..buffer.area.height)
            .map(|y| {
                (x0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect()
            })
            .collect()
    }

    /// Filas de la matriz: las que hay entre la cabecera de columnas y la leyenda.
    fn matrix_table(lines: &[String]) -> &[String] {
        let head = lines
            .iter()
            .position(|line| line.contains(HEADER))
            .expect("cabecera de columnas");
        let legend = lines
            .iter()
            .position(|line| line.contains("genérico"))
            .expect("leyenda");
        &lines[head + 1..legend]
    }

    /// Mueve el cursor de la matriz a la celda (componente, columna), buscándolas por nombre.
    fn place(show: &mut Showcase, component: &str, column: &str) {
        let row = show
            .matrix()
            .rows
            .iter()
            .position(|row| row.component == component)
            .expect("fila del componente");
        let column = matrix::COLUMNS
            .iter()
            .position(|(name, _)| *name == column)
            .expect("columna");
        for _ in 0..1000 {
            press(show, KeyCode::Up);
            press(show, KeyCode::Left);
        }
        for _ in 0..row {
            press(show, KeyCode::Down);
        }
        for _ in 0..column {
            press(show, KeyCode::Right);
        }
    }

    #[test]
    fn matrix_shows_the_three_symbols_and_all_columns() {
        let mut show = Showcase::new();
        press(&mut show, KeyCode::Char('m'));
        let buffer = render(&mut show, 100, 40);
        let lines = lines(&buffer, INDEX_WIDTH);
        assert!(
            lines.iter().any(|line| line.contains(HEADER)),
            "faltan los siete nombres de columna"
        );
        let table = matrix_table(&lines).join("\n");
        for symbol in ["●", "◐", "·"] {
            assert!(table.contains(symbol), "falta {symbol} en la matriz");
        }
    }

    #[test]
    fn matrix_marks_the_cursor_column() {
        let mut show = Showcase::new();
        press(&mut show, KeyCode::Char('m'));
        place(&mut show, "Composer", "Crush");
        let buffer = render(&mut show, 100, 40);
        let reversed: Vec<(u16, u16)> = (INDEX_WIDTH..buffer.area.width)
            .flat_map(|x| (0..buffer.area.height).map(move |y| (x, y)))
            .filter(|&(x, y)| buffer[(x, y)].modifier.contains(Modifier::REVERSED))
            .collect();
        assert!(!reversed.is_empty(), "ninguna celda invertida");
        let y = reversed[0].1;
        assert!(reversed.iter().all(|&(_, row)| row == y));
        // La celda invertida es la de Composer en Crush: un demo propio, «●».
        assert_eq!(buffer[(reversed[0].0, y)].symbol(), "●");

        // El encabezado de Crush va resaltado en negrita.
        let header_y = lines(&buffer, INDEX_WIDTH)
            .iter()
            .position(|line| line.contains(HEADER))
            .expect("cabecera de columnas") as u16;
        let header = lines(&buffer, INDEX_WIDTH).swap_remove(usize::from(header_y));
        let crush_x = INDEX_WIDTH + header.find("Crush").expect("Crush en la cabecera") as u16;
        assert!(buffer[(crush_x, header_y)]
            .modifier
            .contains(Modifier::BOLD));
    }

    #[test]
    fn matrix_scrolls_to_keep_the_cursor_visible() {
        let mut show = Showcase::new();
        press(&mut show, KeyCode::Char('m'));
        let rows = show.matrix().rows.len();
        let first = show.matrix().rows[0].component;
        let last = show.matrix().rows[rows - 1].component;
        for _ in 0..rows {
            press(&mut show, KeyCode::Char('j'));
        }
        let buffer = render(&mut show, 100, 40);
        let table = matrix_table(&lines(&buffer, INDEX_WIDTH)).to_vec();
        // Los nombres se recortan al ancho de la columna de nombre: se compara el prefijo.
        let prefix = |name: &str| name.chars().take(6).collect::<String>();
        assert!(
            table.iter().any(|line| line.starts_with(&prefix(last))),
            "la última fila ({last}) debe verse con el cursor al final"
        );
        assert!(
            table.iter().all(|line| !line.starts_with(&prefix(first))),
            "la primera fila ({first}) debe haber salido del área"
        );
    }

    #[test]
    fn matrix_fits_in_a_narrow_terminal_without_panic() {
        let mut show = Showcase::new();
        press(&mut show, KeyCode::Char('m'));
        let buffer = render(&mut show, 60, 20);
        assert!(lines(&buffer, 0).iter().any(|line| line.contains(HEADER)));
        render(&mut show, 20, 5);
    }

    #[test]
    fn footer_names_the_cell_under_the_cursor() {
        let mut show = Showcase::new();
        press(&mut show, KeyCode::Char('m'));
        place(&mut show, "Composer", "Pawan");
        let buffer = render(&mut show, 100, 40);
        assert!(
            lines(&buffer, INDEX_WIDTH)
                .iter()
                .any(|line| line.contains("Composer · Pawan: sin demo propio")),
            "el pie debe nombrar la celda bajo el cursor"
        );
    }

    /// Índice del demo del catálogo con ese componente y esa variante exacta.
    fn demo_index(show: &Showcase, component: &str, variant: &str) -> usize {
        (0..show.len())
            .find(|&i| {
                show.component_at(i) == Some(component) && show.variant_at(i) == Some(variant)
            })
            .expect("demo en el catálogo")
    }

    #[test]
    fn footer_stays_one_line_without_a_row() {
        let mut show = Showcase::new();
        // dotmax es librería: el componente no tiene fila en la matriz.
        show.select(
            demo_index(&show, "Braille graphics", "dotmax · visual lab"),
            false,
        );
        let buffer = render(&mut show, 100, 30);
        let lines = lines(&buffer, 0);
        // ADAPTADO: el pie nuevo empieza por el tema; sin fila en la matriz no hay línea de nota.
        assert!(
            lines[29].starts_with("Tema: Batten · "),
            "pie de una línea: {}",
            lines[29]
        );
        assert!(lines.iter().all(|line| !line.contains("parcialmente")));
        assert!(lines.iter().all(|line| !line.contains(" · Martty ●")));
    }

    /// Índice de columna de `matrix::COLUMNS` por nombre.
    fn theme_index(name: &str) -> usize {
        matrix::COLUMNS
            .iter()
            .position(|(column, _)| *column == name)
            .expect("columna")
    }

    /// Texto de la columna del índice (las primeras `INDEX_WIDTH` columnas) de cada fila.
    fn index_text(buffer: &Buffer) -> Vec<String> {
        lines(buffer, 0)
            .into_iter()
            .map(|line| line.chars().take(usize::from(INDEX_WIDTH)).collect())
            .collect()
    }

    /// Primer demo del catálogo con ese componente y un proveedor dado como prefijo de variante.
    fn demo_of(show: &Showcase, component: &str, provider: &str) -> usize {
        (0..show.len())
            .find(|&i| {
                show.component_at(i) == Some(component)
                    && show.variant_at(i).is_some_and(|v| v.starts_with(provider))
            })
            .expect("demo en el catálogo")
    }

    #[test]
    fn tabs_show_the_seven_themes_and_mark_the_active() {
        let mut show = Showcase::new();
        let buffer = render(&mut show, 100, 30);
        let row = lines(&buffer, 0).swap_remove(0);
        for (name, _) in matrix::COLUMNS {
            assert!(row.contains(name), "falta la pestaña {name}: {row}");
        }
        let bold = |buffer: &Buffer, line: &str, name: &str| {
            let x = line.find(name).expect("pestaña") as u16;
            buffer[(x, 0)].modifier.contains(Modifier::BOLD)
        };
        assert!(bold(&buffer, &row, "Batten"));
        assert!(!bold(&buffer, &row, "Crush"));

        show.set_theme_column(theme_index("Crush"));
        let buffer = render(&mut show, 100, 30);
        let row = lines(&buffer, 0).swap_remove(0);
        assert!(bold(&buffer, &row, "Crush"));
        assert!(!bold(&buffer, &row, "Batten"));
    }

    #[test]
    fn index_marks_generic_components() {
        // Pawan: Composer es genérico (◐) y Transcript tiene demo propio (●).
        let mut show = Showcase::new();
        show.set_theme_column(theme_index("Pawan"));
        let buffer = render(&mut show, 100, 60);
        let index = index_text(&buffer);
        let composer = index
            .iter()
            .find(|l| l.contains("Composer"))
            .expect("Composer");
        assert!(composer.contains('◐'), "Composer sin ◐: {composer}");
        let transcript = index
            .iter()
            .find(|l| l.contains("Transcript"))
            .expect("Transcript");
        assert!(transcript.contains('●'), "Transcript sin ●: {transcript}");
    }

    #[test]
    fn index_indents_child_examples() {
        // Crush: Assistant message tiene varios ejemplos, con filas hijas sangradas bajo la cabecera.
        let mut show = Showcase::new();
        show.set_theme_column(theme_index("Crush"));
        let buffer = render(&mut show, 100, 60);
        let index = index_text(&buffer);
        let header = index
            .iter()
            .find(|l| l.contains("Assistant message ─"))
            .expect("cabecera de Assistant message");
        assert!(header.contains('●'), "cabecera sin ●: {header}");
        // Columna en caracteres (no en bytes: «●» ocupa tres bytes) donde empieza un texto.
        let column_of =
            |line: &str, needle: &str| line.find(needle).map(|byte| line[..byte].chars().count());
        let name_x = column_of(header, "Assistant").expect("nombre");
        for child in ["pending", "refusal", "summary"] {
            let line = index
                .iter()
                .find(|l| l.contains(child))
                .unwrap_or_else(|| panic!("falta la fila hija {child}"));
            // Dos columnas más que el nombre de la cabecera: la hija va sangrada.
            assert_eq!(
                column_of(line, child),
                Some(name_x + 2),
                "sangría de {child}: {line}"
            );
        }
    }

    #[test]
    fn footer_counts_components_and_missing() {
        let mut show = Showcase::new();
        show.set_theme_column(theme_index("Pawan"));
        let buffer = render(&mut show, 100, 30);
        let all = lines(&buffer, 0);
        assert!(
            all.iter()
                .any(|l| l.contains("26 componentes: 9 ● · 17 ◐ · faltan 31")),
            "recuento de Pawan: {all:?}"
        );
    }

    #[test]
    fn footer_notes_partial_implementation_for_generic() {
        let mut show = Showcase::new();
        show.set_theme_column(theme_index("Pawan"));
        show.select(demo_of(&show, "Composer", "Batten"), false);
        let buffer = render(&mut show, 100, 30);
        let all = lines(&buffer, 0);
        // La línea 2 del pie es la última de la pantalla.
        assert!(
            all[29].contains("parcialmente implementado"),
            "línea 2 del pie: {}",
            all[29]
        );
        assert!(
            all[28].contains("componentes:"),
            "línea 1 del pie: {}",
            all[28]
        );
    }

    #[test]
    fn footer_is_one_line_for_an_own_component() {
        let mut show = Showcase::new();
        show.set_theme_column(theme_index("Crush"));
        show.select(demo_of(&show, "Composer", "Crush"), false);
        let buffer = render(&mut show, 100, 30);
        let all = lines(&buffer, 0);
        assert!(all[29].contains("componentes:"), "pie: {}", all[29]);
        assert!(all.iter().all(|l| !l.contains("parcialmente")));
    }

    #[test]
    fn narrow_terminal_does_not_panic() {
        for (name, component, variant) in [
            ("Pawan", "Composer", "Batten"),
            ("Crush", "Composer", "Crush"),
        ] {
            let mut show = Showcase::new();
            show.set_theme_column(theme_index(name));
            show.select(demo_of(&show, component, variant), false);
            for (w, h) in [(60u16, 20u16), (30, 8), (20, 5)] {
                render(&mut show, w, h);
            }
        }
    }

    #[test]
    fn tab_layout_positions_match_what_is_drawn() {
        let mut show = Showcase::new();
        for width in [100u16, 60] {
            let buffer = render(&mut show, width, 30);
            let layout = tab_spans_layout(width);
            assert!(!layout.is_empty());
            for (column, x, shown) in layout {
                let (name, _) = matrix::COLUMNS[column];
                let drawn: String = (x..x + shown).map(|cx| buffer[(cx, 0)].symbol()).collect();
                assert_eq!(
                    drawn,
                    matrix::fit(&format!(" {name} "), usize::from(shown)),
                    "pestaña {name} a {width} columnas"
                );
            }
        }
        // A 60 columnas la última pestaña no cabe entera: aparece recortada.
        let layout = tab_spans_layout(60);
        let (column, _, shown) = *layout.last().expect("pestañas");
        let (name, _) = matrix::COLUMNS[column];
        assert!(shown < tab_width(name), "{name} debería recortarse");
    }

    const LEFT: MouseEventKind = MouseEventKind::Down(MouseButton::Left);
    const WHEEL_UP: MouseEventKind = MouseEventKind::ScrollUp;
    const WHEEL_DOWN: MouseEventKind = MouseEventKind::ScrollDown;

    /// Pantalla de `w`×`h` celdas con origen en (0, 0), como la de `draw`.
    fn screen(w: u16, h: u16) -> Rect {
        Rect::new(0, 0, w, h)
    }

    /// Pulsación izquierda en (`x`, `y`) de una pantalla de `w`×`h`.
    fn click_at(show: &mut Showcase, x: u16, y: u16, w: u16, h: u16) {
        handle_mouse(show, LEFT, x, y, screen(w, h));
    }

    /// Showcase con el tema de la columna `theme` y el primer demo de `component` de ese proveedor.
    fn state(theme: &str, component: &str, provider: &str) -> Showcase {
        let mut show = Showcase::new();
        show.set_theme_column(theme_index(theme));
        show.select(demo_of(&show, component, provider), false);
        show
    }

    /// Columna de la primera celda visible de la pestaña `theme` en una fila de `width` columnas.
    fn tab_x(width: u16, theme: usize) -> u16 {
        tab_spans_layout(width)
            .iter()
            .find(|(column, _, _)| *column == theme)
            .map(|(_, x, _)| *x)
            .expect("pestaña visible")
    }

    /// Texto de la fila `y` del buffer entre las columnas `x0` (incluida) y `x1` (excluida).
    fn row_text(buffer: &Buffer, x0: u16, x1: u16, y: u16) -> String {
        (x0..x1).map(|x| buffer[(x, y)].symbol()).collect()
    }

    /// Fila de pantalla en la que el índice pinta `text`. Debe aparecer una sola vez.
    fn index_row_of(buffer: &Buffer, text: &str) -> u16 {
        let rows: Vec<usize> = index_text(buffer)
            .iter()
            .enumerate()
            .filter(|(_, line)| line.contains(text))
            .map(|(y, _)| y)
            .collect();
        assert_eq!(rows.len(), 1, "«{text}» en el índice: filas {rows:?}");
        rows[0] as u16
    }

    /// Columna (en caracteres) en la que la fila `y` del índice pinta `text`.
    fn index_column_of(buffer: &Buffer, y: u16, text: &str) -> u16 {
        let line = &index_text(buffer)[usize::from(y)];
        let byte = line
            .find(text)
            .unwrap_or_else(|| panic!("«{text}» no está en la fila {y}: {line}"));
        line[..byte].chars().count() as u16
    }

    /// Fila seleccionable del índice visible en una pantalla `w`×`h`, distinta de la seleccionada,
    /// con un texto corto que aparece una sola vez en el índice. Devuelve (índice del catálogo,
    /// texto, fila de pantalla en la que debe pintarse).
    fn visible_target(show: &Showcase, w: u16, h: u16) -> (usize, String, u16) {
        let index = geometry(screen(w, h), footer_height(show))
            .index
            .expect("índice visible");
        let (start, visible) = show.visible_index_rows(index.height);
        let total = show.index_rows();
        visible
            .iter()
            .enumerate()
            .filter(|(offset, _)| {
                index_notice(start, visible.len(), total.len(), *offset).is_none()
            })
            .find_map(|(offset, row)| {
                let target = row.catalog_index.filter(|&i| i != show.selected())?;
                let text = row.text.trim().to_string();
                let unique = total
                    .iter()
                    .filter(|other| other.text.contains(&text))
                    .count()
                    == 1;
                (unique && text.chars().count() <= 20)
                    .then(|| (target, text, index.y + offset as u16))
            })
            .expect("fila seleccionable visible")
    }

    /// Celda invertida (la del cursor) dentro de `area`: su primera columna y su fila.
    fn reversed_cell(buffer: &Buffer, area: Rect) -> (u16, u16) {
        (area.y..area.bottom())
            .flat_map(|y| (area.x..area.right()).map(move |x| (x, y)))
            .find(|&(x, y)| buffer[(x, y)].modifier.contains(Modifier::REVERSED))
            .expect("celda invertida")
    }

    /// Fila de la matriz cuyo nombre empieza en la columna `x` de `area`, o `None`.
    fn matrix_row_of(buffer: &Buffer, area: Rect, name: &str) -> Option<u16> {
        (area.y..area.bottom())
            .find(|&y| row_text(buffer, area.x, area.right(), y).starts_with(name))
    }

    #[test]
    fn click_on_a_tab_switches_the_theme() {
        let pawan = theme_index("Pawan");
        let mut show = Showcase::new();
        show.open_matrix();
        click_at(&mut show, tab_x(100, pawan) + 1, 0, 100, 30);
        assert_eq!(show.theme_column(), pawan);
        assert_eq!(show.focus(), Focus::Index);

        // El hueco entre dos pestañas no hace nada: el tema y el foco se quedan como estaban.
        let mut show = Showcase::new();
        show.open_matrix();
        let gap = tab_x(100, 0) + tab_width("Batten");
        click_at(&mut show, gap, 0, 100, 30);
        assert_eq!(show.theme_column(), 0);
        assert_eq!(show.focus(), Focus::Matrix);

        // A 40 columnas, Crush se ve recortada a 3 celdas y MastraCode ya no aparece: la recortada
        // sí responde en lo que se ve.
        let crush = theme_index("Crush");
        let mastra = theme_index("MastraCode");
        assert!(tab_spans_layout(40)
            .iter()
            .all(|(column, _, _)| *column != mastra));
        let mut show = Showcase::new();
        click_at(&mut show, tab_x(40, crush) + 2, 0, 40, 30);
        assert_eq!(show.theme_column(), crush);
    }

    #[test]
    fn click_on_an_index_row_selects_and_opens_it() {
        let mut show = Showcase::new();
        let (target, text, y) = visible_target(&show, 100, 30);
        let buffer = render(&mut show, 100, 30);
        assert_eq!(index_row_of(&buffer, &text), y, "fila pintada de «{text}»");
        click_at(&mut show, 1, y, 100, 30);
        assert_eq!(show.selected(), target);
        assert_eq!(show.focus(), Focus::Demo);
    }

    #[test]
    fn click_on_a_child_example_selects_that_example() {
        // Crush: «Assistant message» tiene filas hijas. Partimos de «pending» para que la fila
        // «refusal», vecina, quede visible sin depender del desplazamiento.
        let mut show = state("Crush", "Assistant message", "Crush");
        let refusal = (0..show.len())
            .find(|&i| {
                show.component_at(i) == Some("Assistant message")
                    && show.variant_at(i).is_some_and(|v| v.ends_with("refusal"))
            })
            .expect("demo refusal");
        let pending = (0..show.len())
            .find(|&i| {
                show.component_at(i) == Some("Assistant message")
                    && show.variant_at(i).is_some_and(|v| v.ends_with("pending"))
            })
            .expect("demo pending");
        show.select(pending, false);
        let buffer = render(&mut show, 100, 30);
        let y = index_row_of(&buffer, "refusal");
        click_at(&mut show, 5, y, 100, 30);
        assert_eq!(show.selected(), refusal);
        assert_eq!(show.focus(), Focus::Demo);
    }

    #[test]
    fn click_on_a_header_row_does_nothing() {
        let mut show = state("Crush", "Assistant message", "Crush");
        let before = show.selected();
        let buffer = render(&mut show, 100, 30);
        let y = index_row_of(&buffer, "Assistant message ─");
        click_at(&mut show, 1, y, 100, 30);
        assert_eq!(show.selected(), before);
        assert_eq!(show.focus(), Focus::Index);
    }

    #[test]
    fn wheel_scrolls_the_index_with_a_cap() {
        let mut show = Showcase::new();
        let list = show.filtered_indices();
        assert!(
            list.len() > WHEEL_STEPS,
            "hacen falta más de {WHEEL_STEPS} filas"
        );
        // Con el foco en el demo, la rueda sobre el índice lo devuelve al índice.
        show.select(list[0], true);
        handle_mouse(&mut show, WHEEL_DOWN, 1, 10, screen(100, 30));
        assert_eq!(show.selected(), list[WHEEL_STEPS]);
        assert_eq!(show.focus(), Focus::Index);
        handle_mouse(&mut show, WHEEL_UP, 1, 10, screen(100, 30));
        assert_eq!(show.selected(), list[0]);
        // Con tope: en el último elemento sigue en el último, y el foco no cambia.
        for _ in 0..list.len() {
            handle_mouse(&mut show, WHEEL_DOWN, 1, 10, screen(100, 30));
        }
        assert_eq!(show.selected(), *list.last().expect("filas"));
        handle_mouse(&mut show, WHEEL_DOWN, 1, 10, screen(100, 30));
        assert_eq!(show.selected(), *list.last().expect("filas"));
        assert_eq!(show.focus(), Focus::Index);
    }

    #[test]
    fn wheel_on_the_matrix_moves_the_cursor() {
        let mut show = Showcase::new();
        show.open_matrix();
        // Dentro de la tabla y fuera de ella, la rueda mueve la fila igual.
        handle_mouse(&mut show, WHEEL_DOWN, 50, 10, screen(100, 30));
        assert_eq!(show.matrix_cursor(), (1, 0));
        handle_mouse(&mut show, WHEEL_DOWN, 1, 29, screen(100, 30));
        assert_eq!(show.matrix_cursor(), (2, 0));
        handle_mouse(&mut show, WHEEL_UP, 50, 2, screen(100, 30));
        assert_eq!(show.matrix_cursor(), (1, 0));
        // Sobre el índice, con la matriz abierta, también mueve el cursor de la matriz.
        handle_mouse(&mut show, WHEEL_DOWN, 1, 10, screen(100, 30));
        assert_eq!(show.matrix_cursor(), (2, 0));
        assert_eq!(show.focus(), Focus::Matrix);
    }

    #[test]
    fn click_on_a_matrix_cell_moves_the_cursor_and_keeps_the_matrix() {
        let mut show = Showcase::new();
        show.open_matrix();
        let buffer = render(&mut show, 100, 30);
        let content = geometry(screen(100, 30), footer_height(&show)).content;
        // La fila de Composer y la cabecera de Crush, tal como se pintan.
        let composer_y = matrix_row_of(&buffer, content, "Composer").expect("fila de Composer");
        let header = row_text(&buffer, content.x, content.right(), content.y);
        let crush_x = content.x + header.find("Crush").expect("cabecera de Crush") as u16;
        let composer = show
            .matrix()
            .rows
            .iter()
            .position(|row| row.component == "Composer")
            .expect("fila");
        click_at(&mut show, crush_x + 1, composer_y, 100, 30);
        assert_eq!(show.matrix_cursor(), (composer, theme_index("Crush")));
        assert_eq!(show.focus(), Focus::Matrix);
        assert_eq!(
            show.theme_column(),
            0,
            "un clic en una celda no cambia el tema"
        );
    }

    #[test]
    fn click_outside_the_matrix_does_not_close_it() {
        let mut show = Showcase::new();
        show.open_matrix();
        show.matrix_set_cursor(3, 2);
        let buffer = render(&mut show, 100, 30);
        let content = geometry(screen(100, 30), footer_height(&show)).content;
        let legend_y = (content.y..content.bottom())
            .find(|&y| row_text(&buffer, content.x, content.right(), y).contains("genérico"))
            .expect("leyenda");
        // Leyenda, pie y cabecera del panel: fuera de la tabla.
        click_at(&mut show, 60, legend_y, 100, 30);
        assert_eq!(show.focus(), Focus::Matrix);
        click_at(&mut show, 50, 29, 100, 30);
        assert_eq!(show.focus(), Focus::Matrix);
        click_at(&mut show, 60, 3, 100, 30);
        assert_eq!(show.focus(), Focus::Matrix);
        assert_eq!(show.matrix_cursor(), (3, 2));
    }

    #[test]
    fn click_on_the_demo_panel_focuses_the_demo() {
        let mut show = Showcase::new();
        click_at(&mut show, 60, 10, 100, 30);
        assert_eq!(show.focus(), Focus::Demo);
        // En terminal estrecha no hay índice: el panel ocupa todo el cuerpo y también enfoca.
        let mut show = Showcase::new();
        click_at(&mut show, 30, 10, 60, 20);
        assert_eq!(show.focus(), Focus::Demo);
    }

    #[test]
    fn mouse_and_draw_share_the_same_geometry() {
        // Pawan con Composer seleccionado tiene pie de 2 líneas; Crush con Composer propio, de 1.
        for (w, h) in [(100u16, 30u16), (80, 24), (70, 20), (60, 20)] {
            for (theme, provider, footer) in [("Pawan", "Batten", 2u16), ("Crush", "Crush", 1)] {
                let label = format!("{theme} {w}x{h}");
                let mut show = state(theme, "Composer", provider);
                assert_eq!(footer_height(&show), footer, "pie de {label}");

                // Índice (solo con terminal ancha): la fila pintada de un ejemplo es la que se
                // selecciona al hacer clic en su texto.
                if w >= WIDE_MIN {
                    let (target, text, y) = visible_target(&show, w, h);
                    let buffer = render(&mut show, w, h);
                    assert!(
                        index_text(&buffer)[usize::from(y)].contains(&text),
                        "{label}: «{text}» no se pinta en la fila {y}"
                    );
                    let x = index_column_of(&buffer, y, &text);
                    click_at(&mut show, x + 1, y, w, h);
                    assert_eq!(show.selected(), target, "{label}: clic en «{text}»");
                }

                // Matriz (todas las anchuras): la celda invertida pintada es la que el ratón
                // reconoce. Con el cursor en la última fila, su posición depende de la altura del
                // pie, así que un desfase movería el cursor.
                let mut show = state(theme, "Composer", provider);
                show.open_matrix();
                let last = show.matrix().rows.len() - 1;
                show.matrix_set_cursor(last, 2);
                let buffer = render(&mut show, w, h);
                let content = geometry(screen(w, h), footer_height(&show)).content;
                let (x, y) = reversed_cell(&buffer, content);
                click_at(&mut show, x, y, w, h);
                assert_eq!(show.matrix_cursor(), (last, 2), "{label}: celda pintada");
                let header = row_text(&buffer, content.x, content.right(), content.y);
                let crush_x = content.x + header.find("Crush").expect("cabecera") as u16;
                click_at(&mut show, crush_x, y, w, h);
                assert_eq!(
                    show.matrix_cursor(),
                    (last, theme_index("Crush")),
                    "{label}: columna pintada"
                );
            }
        }
    }

    /// El aviso de «más resultados» sale por arriba y por abajo, y solo cuando hay más.
    #[test]
    fn index_notice_marks_both_ends_only_when_there_is_more() {
        // Al principio de una lista larga: nada por arriba, aviso en la última fila visible.
        assert_eq!(index_notice(0, 10, 50, 0), None);
        assert_eq!(index_notice(0, 10, 50, 9), Some("  ↓ más resultados"));
        assert_eq!(index_notice(0, 10, 50, 8), None);
        // En medio: aviso arriba y abajo.
        assert_eq!(index_notice(20, 10, 50, 0), Some("  ↑ más resultados"));
        assert_eq!(index_notice(20, 10, 50, 9), Some("  ↓ más resultados"));
        // Al final: aviso arriba y ninguno abajo.
        assert_eq!(index_notice(40, 10, 50, 0), Some("  ↑ más resultados"));
        assert_eq!(index_notice(40, 10, 50, 9), None);
        // Si cabe todo, ninguno.
        assert_eq!(index_notice(0, 10, 10, 9), None);
    }

    /// En un terminal bajo, la última fila visible del índice dice que hay más por debajo.
    #[test]
    fn a_short_terminal_shows_the_more_below_notice() {
        let mut show = Showcase::new();
        let buffer = render(&mut show, 100, 12);
        let text = lines(&buffer, 0).join(
            "
",
        );
        assert!(text.contains("↓ más resultados"), "{text}");
    }
}
