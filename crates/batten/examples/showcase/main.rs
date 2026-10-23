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
use matrix::draw_matrix;
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
    let [filters, body, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(1),
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
    let [theme_area, query_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);
    let muted = theme.style(batten::state::StyleToken::Muted);
    // Provisional hasta la Tarea 3 (pestañas de tema): los siete temas en texto, el activo entre
    // corchetes.
    let names: Vec<String> = matrix::COLUMNS
        .iter()
        .enumerate()
        .map(|(column, (name, _))| {
            if column == show.theme_column() {
                format!("[{name}]")
            } else {
                name.to_string()
            }
        })
        .collect();
    f.render_widget(
        Paragraph::new(Line::styled(names.join("  "), muted)),
        theme_area,
    );

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
                Focus::Index => "↑/↓: variante · ←/→: tema · Tab: abrir · q: salir",
                Focus::Demo => "F7/Shift+F7: tema · Tab/Esc: volver al índice",
                Focus::Matrix => "←/→/↑/↓: celda · Enter: abrir · Esc: índice · F7: paleta",
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
                "F7/Shift+F7: tema · Tab/Esc: volver al índice"
            } else {
                "↑/↓: variante · ←/→: tema · Tab: abrir · q: salir"
            },
            theme.style(batten::state::StyleToken::Muted),
        ),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

/// Pie de una línea con el tema activo. El detalle del componente (tira y nota) vuelve en la
/// Tarea 3.
fn draw_footer(f: &mut Frame, area: Rect, show: &Showcase, theme: &Theme) {
    f.render_widget(
        Paragraph::new(format!(
            "batten showcase · F7/Shift+F7 tema: {}",
            show.theme_preset().name()
        ))
        .style(theme.style(batten::state::StyleToken::Muted)),
        area,
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
