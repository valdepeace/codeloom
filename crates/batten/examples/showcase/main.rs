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
use matrix::{draw_matrix, Cell};
use ratatui::crossterm::event::{self, Event, MouseButton, MouseEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;
use std::time::{Duration, Instant};

const INDEX_WIDTH: u16 = 34;
const WIDE_MIN: u16 = 70;
/// Columnas entre dos pestañas de tema.
const TAB_GAP: u16 = 1;

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
                Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
                    handle_mouse(&mut show, mouse.column, mouse.row, terminal.size()?.into());
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
    let [filters, body, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(footer_lines.len() as u16),
    ])
    .areas(area);
    draw_filters(f, filters, show, theme);

    let wide = area.width >= WIDE_MIN;
    let (index, panel) = if wide {
        let cols =
            Layout::horizontal([Constraint::Length(INDEX_WIDTH), Constraint::Min(1)]).split(body);
        (Some(cols[0]), cols[1])
    } else {
        (None, body)
    };

    let header_h = 3;
    let parts = Layout::vertical([Constraint::Length(header_h), Constraint::Min(1)]).split(panel);
    let (header, content) = (parts[0], parts[1]);

    // En la matriz no se consulta `fullscreen()`: el panel de contenido es siempre la matriz.
    let matrix_mode = show.focus() == Focus::Matrix;
    let fullscreen = !matrix_mode && show.current().is_some_and(|demo| demo.fullscreen());

    if let Some(ia) = index {
        draw_index(f, ia, show, theme);
        if !fullscreen {
            draw_header(f, header, show, theme);
        }
    } else {
        if !fullscreen {
            draw_collapsed(f, header, show, theme);
        }
    }

    if matrix_mode {
        draw_matrix(f, content, show, theme);
    } else if let Some(demo) = show.current() {
        demo.render(f, if demo.fullscreen() { panel } else { content }, theme);
    }
    draw_footer(f, footer, &footer_lines, theme);
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

fn draw_filters(f: &mut Frame, area: Rect, show: &Showcase, theme: &Theme) {
    let [tabs_area, query_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);
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

fn handle_mouse(show: &mut Showcase, column: u16, row: u16, screen: Rect) {
    if screen.width == 0 || screen.height < 3 {
        return;
    }
    let filter_end = screen.y.saturating_add(2);
    // La fila 0 (pestañas de tema) no hace nada hasta la Tarea 4 (ratón).
    if row == screen.y {
        return;
    }
    if row == screen.y.saturating_add(1) {
        show.set_focus(Focus::TextFilter);
        return;
    }
    if row < filter_end || row >= screen.y.saturating_add(screen.height).saturating_sub(1) {
        return;
    }

    let body_y = filter_end;
    let body_height = screen.height.saturating_sub(3);
    if screen.width >= WIDE_MIN && column < screen.x.saturating_add(INDEX_WIDTH) {
        let relative_row = row.saturating_sub(body_y);
        let (start, visible) = show.visible_index_rows(body_height);
        if start > 0 && relative_row == 0 {
            return;
        }
        let all_rows = show.index_rows();
        if start + visible.len() < all_rows.len() && usize::from(relative_row) + 1 == visible.len()
        {
            return;
        }
        if let Some(IndexRow {
            catalog_index: Some(index),
            ..
        }) = visible.get(usize::from(relative_row))
        {
            show.select(*index, true);
        }
    } else if screen.width < WIDE_MIN || column >= screen.x.saturating_add(INDEX_WIDTH) {
        show.set_focus(Focus::Demo);
    }
}

fn draw_index(f: &mut Frame, area: Rect, show: &Showcase, theme: &Theme) {
    let (start, rows) = show.visible_index_rows(area.height);
    let all_rows = show.index_rows();
    let muted = theme.style(batten::state::StyleToken::Muted);
    let mut lines = Vec::with_capacity(rows.len());
    for (offset, row) in rows.iter().enumerate() {
        if offset == 0 && start > 0 {
            lines.push(Line::styled("  ↑ más resultados", muted));
            continue;
        }
        if start + offset + 1 == all_rows.len() && start + rows.len() < all_rows.len() {
            lines.push(Line::styled("  ↓ más resultados", muted));
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
    if let Some(component) = show.component_at(show.selected()) {
        let generic_entry = view
            .entries
            .iter()
            .find(|entry| entry.component == component)
            .is_some_and(|entry| entry.generic);
        if generic_entry {
            let note = matrix::status_note(component, show.theme_column(), Cell::Generic);
            lines.push(matrix::fit(&note, usize::from(width)));
        }
    }
    lines
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
}
