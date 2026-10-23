//! Showcase navegable de los widgets de batten.
//! Uso: `cargo run -p batten --example showcase`

mod app;
mod demo;
mod demos;
mod matrix;
mod prompt_keys;
mod theme_view;

use app::{route, Action, Focus, Showcase};
use batten::widgets::{Anim, Theme, FRAME_INTERVAL};
use matrix::{draw_matrix, fit, glyph, status_note, strip, MatrixRow};
use ratatui::crossterm::event::{self, Event, MouseButton, MouseEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;
use std::time::{Duration, Instant};

const INDEX_WIDTH: u16 = 34;
const WIDE_MIN: u16 = 70;

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
    // El pie ocupa dos líneas solo cuando muestra la vista entre temas del componente actual.
    let footer_height = if component_view(show).is_some() { 2 } else { 1 };
    let [filters, body, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(footer_height),
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
    draw_footer(f, footer, show, theme);
}

fn draw_filters(f: &mut Frame, area: Rect, show: &Showcase, theme: &Theme) {
    let [provider_area, query_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);
    let muted = theme.style(batten::state::StyleToken::Muted);
    let active = theme
        .style(batten::state::StyleToken::Accent)
        .add_modifier(Modifier::BOLD);
    let mut provider_spans = vec![Span::styled("Provider: ", muted)];
    let selected = show.provider_filter();
    let all_style = if selected.is_none() { active } else { muted };
    provider_spans.push(Span::styled(" All ", all_style));
    for provider in show.providers() {
        let style = if selected == Some(provider) {
            active
        } else {
            muted
        };
        provider_spans.push(Span::styled(format!(" {provider} "), style));
    }
    f.render_widget(Paragraph::new(Line::from(provider_spans)), provider_area);

    let cursor = if show.focus() == Focus::TextFilter {
        "▏"
    } else {
        ""
    };
    let query = if show.query().is_empty() && show.focus() != Focus::TextFilter {
        "provider · component · variant".to_string()
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

fn handle_mouse(show: &mut Showcase, column: u16, row: u16, screen: Rect) {
    if screen.width == 0 || screen.height < 3 {
        return;
    }
    let filter_end = screen.y.saturating_add(2);
    if row == screen.y {
        let mut x = screen.x.saturating_add(10);
        if column >= x && usize::from(column - x) < 5 {
            show.select_provider(None);
            show.set_focus(Focus::ProviderFilter);
            return;
        }
        x = x.saturating_add(5);
        for provider in show.providers() {
            let width = provider.chars().count() as u16 + 2;
            if column >= x && column < x.saturating_add(width) {
                show.select_provider(Some(provider));
                show.set_focus(Focus::ProviderFilter);
                return;
            }
            x = x.saturating_add(width);
        }
        show.set_focus(Focus::ProviderFilter);
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
        if let Some((Some(index), _)) = visible.get(usize::from(relative_row)) {
            show.select(*index, true);
        }
    } else if screen.width < WIDE_MIN || column >= screen.x.saturating_add(INDEX_WIDTH) {
        show.set_focus(Focus::Demo);
    }
}

fn draw_index(f: &mut Frame, area: Rect, show: &Showcase, theme: &Theme) {
    let (start, rows) = show.visible_index_rows(area.height);
    let all_rows = show.index_rows();
    let mut lines = Vec::with_capacity(rows.len());
    for (offset, (index, label)) in rows.iter().enumerate() {
        if offset == 0 && start > 0 {
            lines.push(Line::styled(
                "  ↑ más resultados",
                theme.style(batten::state::StyleToken::Muted),
            ));
            continue;
        }
        if start + offset + 1 == all_rows.len() && start + rows.len() < all_rows.len() {
            lines.push(Line::styled(
                "  ↓ más resultados",
                theme.style(batten::state::StyleToken::Muted),
            ));
            continue;
        }
        let Some(i) = index else {
            lines.push(Line::styled(
                format!("  {label}"),
                theme.style(batten::state::StyleToken::Muted),
            ));
            continue;
        };
        let selected = *i == show.selected();
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
        lines.push(Line::styled(format!("  {marker}{label}"), style));
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
            match show.focus() {
                Focus::Index => "↑/↓: variante · Tab: abrir · F7: paleta · q: salir",
                Focus::Demo => "F8: tema del componente · F7: paleta · Tab/Esc: volver al índice",
                Focus::Matrix => "←/→/↑/↓: celda · Enter: abrir · Esc: índice · F7: paleta",
                Focus::ProviderFilter => "←/→: proveedor · Tab/Enter: buscar · Esc: índice",
                Focus::TextFilter => "Escribe para filtrar · Enter: índice · Esc: limpiar",
            },
            theme.style(batten::state::StyleToken::Muted),
        ),
    ];
    f.render_widget(Paragraph::new(lines), area);
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
            if show.focus() == Focus::Demo {
                "F8: tema del componente · F7: paleta · Tab/Esc: volver al índice"
            } else {
                "↑/↓: variante · Tab: abrir · F7: paleta · q: salir"
            },
            theme.style(batten::state::StyleToken::Muted),
        ),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

/// Fila de la matriz del componente del demo actual y su columna, si el pie debe mostrar la vista
/// entre temas. `None` en la matriz, si el componente no tiene fila o si no hay columna actual.
fn component_view(show: &Showcase) -> Option<(&MatrixRow, usize)> {
    if show.focus() == Focus::Matrix {
        return None;
    }
    let component = show.component_at(show.selected())?;
    let row = show
        .matrix()
        .rows
        .iter()
        .find(|row| row.component == component)?;
    let column = show.current_column()?;
    (column < row.cells.len()).then_some((row, column))
}

/// Pie de la pantalla. Con fila para el componente actual, dos líneas: la tira de temas y la nota
/// de la columna actual; si no, la línea de siempre con el tema activo.
fn draw_footer(f: &mut Frame, area: Rect, show: &Showcase, theme: &Theme) {
    let preset = show.theme_preset();
    let Some((row, column)) = component_view(show) else {
        f.render_widget(
            Paragraph::new(format!(
                "batten showcase · F7/Shift+F7 tema: {}",
                preset.name()
            ))
            .style(theme.style(batten::state::StyleToken::Muted)),
            area,
        );
        return;
    };
    let muted = theme.style(batten::state::StyleToken::Muted);
    let current_style = theme
        .style(batten::state::StyleToken::Accent)
        .add_modifier(Modifier::BOLD);
    let mut spans = Vec::new();
    for (index, (name, cell, current)) in strip(row, Some(column)).into_iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(" · ", muted));
        }
        let (symbol, style) = glyph(cell, theme);
        if current {
            spans.push(Span::styled(format!("{name} {symbol}"), current_style));
        } else {
            spans.push(Span::styled(format!("{name} "), muted));
            spans.push(Span::styled(symbol, style));
        }
    }
    let [strip_area, note_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);
    f.render_widget(Paragraph::new(Line::from(spans)), strip_area);

    // La nota manda: el tema activo solo se añade si cabe entero detrás de ella. Antes, al no
    // caber, se recortaba la nota para conservar el tema y se perdía lo más útil.
    let width = usize::from(area.width);
    let note = status_note(row.component, column, row.cells[column]);
    let suffix = format!(" · F7/Shift+F7 tema: {}", preset.name());
    let text = if note.chars().count() + suffix.chars().count() <= width {
        format!("{note}{suffix}")
    } else {
        fit(&note, width)
    };
    f.render_widget(
        Paragraph::new(Line::styled(
            text,
            theme.style(batten::state::StyleToken::Default),
        )),
        note_area,
    );
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
    fn footer_shows_the_strip_and_the_note_for_a_component_with_a_row() {
        let mut show = Showcase::new();
        show.select(demo_index(&show, "Composer", "Crush"), false);
        let buffer = render(&mut show, 100, 30);
        let lines = lines(&buffer, 0);
        let (strip_line, note_line) = (&lines[28], &lines[29]);
        assert!(
            strip_line.contains(
                "Batten ● · Martty ● · Mindfork ● · Pawan ◐ · Crush ● · MastraCode ◐ · CodeLoom ◐"
            ),
            "tira de temas: {strip_line}"
        );
        assert!(
            note_line.contains("● Composer · Crush: demo propio"),
            "nota: {note_line}"
        );
        assert!(
            note_line.contains("F7/Shift+F7 tema:"),
            "nota sin tema: {note_line}"
        );
    }

    /// Si la nota es larga, manda la nota: no se recorta para dejar sitio al tema activo.
    #[test]
    fn a_long_note_is_not_cut_to_make_room_for_the_theme_name() {
        let mut show = Showcase::new();
        show.select(demo_index(&show, "Composer", "Crush"), false);
        // Shift+F8 desde Crush lleva a Pawan, que es una celda genérica.
        route(
            &mut show,
            KeyEventKind::Press,
            KeyCode::Tab,
            KeyModifiers::NONE,
        );
        route(
            &mut show,
            KeyEventKind::Press,
            KeyCode::F(8),
            KeyModifiers::SHIFT,
        );
        let buffer = render(&mut show, 100, 30);
        let lines = lines(&buffer, 0);
        assert!(
            lines[29]
                .contains("parcialmente implementado (genérico de Batten con la paleta de Pawan)"),
            "la nota debe verse entera: {}",
            lines[29]
        );
        assert!(!lines[29].contains('…'), "{}", lines[29]);
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
        assert!(
            lines[29].starts_with("batten showcase · F7/Shift+F7 tema:"),
            "pie de una línea: {}",
            lines[29]
        );
        assert!(lines.iter().all(|line| !line.contains(" · Martty ●")));
    }
}
