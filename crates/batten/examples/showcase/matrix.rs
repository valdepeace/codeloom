//! Derivación pura de la matriz de componentes por tema del showcase.
//!
//! Filas = componentes; columnas = temas. Cada celda dice si el tema tiene reproducción propia
//! (`Own`), si solo existe el widget genérico de Batten pintado con su paleta (`Generic`) o si no
//! existe (`Missing`). No depende de la terminal: se prueba directamente sobre el catálogo.

use crate::app::Showcase;
use crate::demos::{strict_provider, CatalogItem};
use batten::state::StyleToken;
use batten::widgets::{Theme, ThemePreset};
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

/// Estado de una celda de la matriz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cell {
    /// Hay `n` demos (n ≥ 1) del proveedor de la columna.
    Own(usize),
    /// Sin demo propio, pero Batten tiene un demo del componente.
    Generic,
    /// No existe.
    Missing,
}

/// Columnas de la matriz, en este orden, y su preset (None = sin preset propio).
pub const COLUMNS: [(&str, Option<ThemePreset>); 7] = [
    ("Batten", Some(ThemePreset::Batten)),
    ("Martty", Some(ThemePreset::Martty)),
    ("Mindfork", Some(ThemePreset::Mindfork)),
    ("Pawan", Some(ThemePreset::Pawan)),
    ("Crush", Some(ThemePreset::Crush)),
    ("MastraCode", Some(ThemePreset::MastraCode)),
    ("CodeLoom", Some(ThemePreset::CodeLoom)),
];

/// Proveedor de la columna Batten, usado como respaldo para las celdas `Generic`.
pub(crate) const BATTEN: &str = "Batten";

pub struct Matrix {
    /// Componentes con al menos un demo de un proveedor de columna, en el orden de primera
    /// aparición en el catálogo.
    pub rows: Vec<MatrixRow>,
    /// (componente, variante) cuyo proveedor no se reconoce; no cuentan en ninguna celda.
    pub left_out: Vec<(&'static str, &'static str)>,
    /// Componentes cuyos demos son solo de dotmax (librería, no tema), sin repetir.
    pub library: Vec<&'static str>,
}

pub struct MatrixRow {
    pub component: &'static str,
    pub cells: [Cell; 7],
}

/// Deriva la matriz a partir del catálogo.
pub fn build(items: &[CatalogItem]) -> Matrix {
    let mut components: Vec<&'static str> = Vec::new();
    for item in items {
        if !components.contains(&item.component) {
            components.push(item.component);
        }
    }

    let mut rows = Vec::new();
    let mut library = Vec::new();
    for &component in &components {
        let known = items.iter().any(|item| {
            item.component == component && strict_provider(item.variant).is_some_and(is_column)
        });
        if known {
            let cells = std::array::from_fn(|column| cell(items, component, column));
            rows.push(MatrixRow { component, cells });
            continue;
        }
        let dotmax = items.iter().any(|item| {
            item.component == component && strict_provider(item.variant) == Some("dotmax")
        });
        if dotmax {
            library.push(component);
        }
    }

    let left_out = items
        .iter()
        .filter(|item| strict_provider(item.variant).is_none())
        .map(|item| (item.component, item.variant))
        .collect();

    Matrix {
        rows,
        left_out,
        library,
    }
}

/// Índice del primer demo del catálogo para esa celda: el del proveedor de la columna si es
/// `Own`, el de Batten si es `Generic`, `None` si es `Missing`.
pub fn target(items: &[CatalogItem], component: &str, column: usize) -> Option<usize> {
    let own = provider_of_column(column)?;
    let first = |provider: &str| {
        items.iter().position(|item| {
            item.component == component && strict_provider(item.variant) == Some(provider)
        })
    };
    first(own).or_else(|| first(BATTEN))
}

/// Estado de la celda (componente, columna) según el catálogo.
fn cell(items: &[CatalogItem], component: &str, column: usize) -> Cell {
    let Some(provider) = provider_of_column(column) else {
        return Cell::Missing;
    };
    let own = items
        .iter()
        .filter(|item| {
            item.component == component && strict_provider(item.variant) == Some(provider)
        })
        .count();
    if own > 0 {
        return Cell::Own(own);
    }
    let has_batten = items
        .iter()
        .any(|item| item.component == component && strict_provider(item.variant) == Some(BATTEN));
    if has_batten {
        Cell::Generic
    } else {
        Cell::Missing
    }
}

/// Nombre de proveedor de una columna, o `None` si el índice está fuera de `COLUMNS`.
pub(crate) fn provider_of_column(column: usize) -> Option<&'static str> {
    COLUMNS.get(column).map(|(name, _)| *name)
}

/// Indica si un proveedor corresponde a una de las columnas de la matriz.
fn is_column(provider: &str) -> bool {
    COLUMNS.iter().any(|(name, _)| *name == provider)
}

/// Símbolo de una celda y su estilo: `●` con el de éxito, `◐` con el de aviso, `·` con el atenuado.
pub fn glyph(cell: Cell, theme: &Theme) -> (&'static str, Style) {
    match cell {
        Cell::Own(_) => ("●", theme.style(StyleToken::Success)),
        Cell::Generic => ("◐", theme.style(StyleToken::Warning)),
        Cell::Missing => ("·", theme.style(StyleToken::Muted)),
    }
}

/// Nota de estado de una celda (componente, columna), para el pie de la vista entre temas.
pub fn status_note(component: &str, column: usize, cell: Cell) -> String {
    let Some((name, _)) = COLUMNS.get(column) else {
        return String::new();
    };
    match cell {
        Cell::Own(1) => format!("● {component} · {name}: demo propio"),
        Cell::Own(n) => format!("● {component} · {name}: {n} variantes propias"),
        // Breve a propósito: el pie recorta por la derecha y no debe perder lo esencial.
        Cell::Generic => format!(
            "◐ {component} · {name}: parcialmente implementado (genérico de Batten con la paleta de {name})"
        ),
        Cell::Missing => format!("· {component} · {name}: no existe"),
    }
}

/// Ancho mínimo, en columnas, para dibujar la matriz; por debajo solo se muestra un aviso.
const MIN_WIDTH: u16 = 30;
/// Alto mínimo, en filas, para dibujar la matriz; por debajo solo se muestra un aviso.
const MIN_HEIGHT: u16 = 3;
/// Espacio entre el texto de una columna y la siguiente.
const GAP: usize = 1;

/// `true` si `area` tiene el tamaño mínimo para dibujar la matriz.
fn matrix_fits(area: Rect) -> bool {
    area.width >= MIN_WIDTH && area.height >= MIN_HEIGHT
}

/// Nota de la matriz para `area`, si cabe. La comparten el dibujo y el ratón: si no, la tabla no
/// tendría la misma altura en ambos.
fn note_for(area: Rect, matrix: &Matrix) -> Option<String> {
    // La nota solo cabe si, además de cabecera, una fila, leyenda y celda, queda una línea.
    note_text(matrix).filter(|_| area.height >= 5)
}

/// Reparto vertical de la matriz en `area`: cabecera, tabla, leyenda, celda del cursor y nota.
fn matrix_areas(area: Rect, with_note: bool) -> [Rect; 5] {
    Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(u16::from(with_note)),
    ])
    .areas(area)
}

/// Ancho de la columna de nombres y de cada columna de tema, para un área de `width` columnas.
fn column_widths(width: u16) -> (usize, Vec<usize>) {
    let widths: Vec<usize> = COLUMNS
        .iter()
        .map(|(name, _)| name.chars().count() + GAP)
        .collect();
    let name_width = usize::from(width).saturating_sub(widths.iter().sum());
    (name_width, widths)
}

/// Qué hay bajo un punto de la tabla: el nombre de una fila o una celda.
enum Hit {
    Name(usize),
    Cell(usize, usize),
}

/// Fila y columna de la matriz bajo (`column`, `row`), con el mismo reparto y el mismo
/// desplazamiento vertical que `draw_matrix`. `None` si el punto no cae en una fila pintada.
fn table_hit(area: Rect, show: &Showcase, column: u16, row: u16) -> Option<Hit> {
    let matrix = show.matrix();
    if !matrix_fits(area) || matrix.rows.is_empty() {
        return None;
    }
    let [_, table, ..] = matrix_areas(area, note_for(area, matrix).is_some());
    if !table.contains(Position::new(column, row)) {
        return None;
    }
    let (cursor_row, _) = show.matrix_cursor();
    let offset = (cursor_row + 1).saturating_sub(usize::from(table.height));
    let index = offset + usize::from(row - table.y);
    if index >= matrix.rows.len() {
        return None;
    }
    let (name_width, widths) = column_widths(area.width);
    let x = usize::from(column - area.x);
    if x < name_width {
        return Some(Hit::Name(index));
    }
    let mut start = name_width;
    for (column_index, width) in widths.iter().enumerate() {
        if x < start + *width {
            return Some(Hit::Cell(index, column_index));
        }
        start += *width;
    }
    None
}

/// Celda (fila, columna) de la matriz bajo el punto, si cae en una celda pintada en `area`.
pub(crate) fn matrix_cell_at(
    area: Rect,
    show: &Showcase,
    column: u16,
    row: u16,
) -> Option<(usize, usize)> {
    match table_hit(area, show, column, row)? {
        Hit::Cell(index, column_index) => Some((index, column_index)),
        Hit::Name(_) => None,
    }
}

/// Fila de la matriz cuyo nombre está bajo el punto, si cae en el nombre de una fila pintada.
pub(crate) fn matrix_name_at(area: Rect, show: &Showcase, column: u16, row: u16) -> Option<usize> {
    match table_hit(area, show, column, row)? {
        Hit::Name(index) => Some(index),
        Hit::Cell(..) => None,
    }
}

/// Dibuja la matriz de componentes por tema dentro de `area`: cabecera de columnas, una fila por
/// componente con su símbolo en cada tema, leyenda, texto de la celda del cursor y, si hay sitio,
/// nota sobre dotmax y los demos sin proveedor.
pub fn draw_matrix(f: &mut Frame, area: Rect, show: &Showcase, theme: &Theme) {
    if !matrix_fits(area) {
        let message = Paragraph::new("Terminal pequeña").style(theme.style(StyleToken::Muted));
        f.render_widget(message, area);
        return;
    }
    let matrix = show.matrix();
    let (cursor_row, cursor_col) = show.matrix_cursor();
    let note = note_for(area, matrix);
    let [header, table, legend, cell, note_area] = matrix_areas(area, note.is_some());

    let (name_width, widths) = column_widths(area.width);
    let muted = theme.style(StyleToken::Muted);

    let mut head = vec![Span::raw(" ".repeat(name_width))];
    for (column, (name, _)) in COLUMNS.iter().enumerate() {
        let style = if column == cursor_col {
            theme.style(StyleToken::Accent).add_modifier(Modifier::BOLD)
        } else {
            muted
        };
        head.push(Span::styled(
            format!("{name:<w$}", w = widths[column]),
            style,
        ));
    }
    f.render_widget(Paragraph::new(Line::from(head)), header);

    let rows: Vec<Line> = if matrix.rows.is_empty() {
        vec![Line::styled("Sin componentes", muted)]
    } else {
        // Desplazamiento mínimo que deja visible la fila del cursor.
        let height = usize::from(table.height);
        let offset = (cursor_row + 1).saturating_sub(height);
        matrix
            .rows
            .iter()
            .enumerate()
            .skip(offset)
            .take(height)
            .map(|(row_index, row)| {
                let on_cursor_row = row_index == cursor_row;
                row_line(row, on_cursor_row, cursor_col, &widths, name_width, theme)
            })
            .collect()
    };
    f.render_widget(Paragraph::new(rows), table);

    let legend_line = Line::from(vec![
        Span::styled("●", theme.style(StyleToken::Success)),
        Span::styled(" propio  ", muted),
        Span::styled("◐", theme.style(StyleToken::Warning)),
        Span::styled(" genérico  ", muted),
        Span::styled("·", muted),
        Span::styled(" no existe", muted),
    ]);
    f.render_widget(Paragraph::new(legend_line), legend);

    let width = usize::from(area.width);
    let cell_line = Line::styled(
        fit(&cell_text(show), width),
        theme.style(StyleToken::Default),
    );
    f.render_widget(Paragraph::new(cell_line), cell);

    if let Some(text) = note {
        f.render_widget(
            Paragraph::new(Line::styled(fit(&text, width), muted)),
            note_area,
        );
    }
}

/// Línea de una fila: el nombre recortado a su ancho y, en cada columna, su símbolo. La celda
/// del cursor va invertida.
fn row_line(
    row: &MatrixRow,
    on_cursor_row: bool,
    cursor_col: usize,
    widths: &[usize],
    name_width: usize,
    theme: &Theme,
) -> Line<'static> {
    let name_style = if on_cursor_row {
        theme
            .style(StyleToken::Heading)
            .add_modifier(Modifier::BOLD)
    } else {
        theme.style(StyleToken::Default)
    };
    let name = fit(row.component, name_width.saturating_sub(GAP));
    let mut spans = vec![Span::styled(
        format!("{name:<w$}", w = name_width),
        name_style,
    )];
    for (column, cell) in row.cells.iter().enumerate() {
        let (symbol, style) = glyph(*cell, theme);
        let slot = format!("{symbol:<w$}", w = widths[column]);
        if on_cursor_row && column == cursor_col {
            spans.push(Span::styled(slot, style.add_modifier(Modifier::REVERSED)));
        } else {
            spans.push(Span::styled(slot, style));
        }
    }
    Line::from(spans)
}

/// Texto de la celda bajo el cursor: componente, columna y estado. Vacío si no hay filas.
fn cell_text(show: &Showcase) -> String {
    let (row_index, column) = show.matrix_cursor();
    let (Some(row), Some((name, _))) = (show.matrix().rows.get(row_index), COLUMNS.get(column))
    else {
        return String::new();
    };
    let head = format!("{} · {name}", row.component);
    match show.matrix_cell() {
        Some(Cell::Own(1)) => format!("{head}: 1 demo propio"),
        Some(Cell::Own(n)) => format!("{head}: {n} demos propios"),
        Some(Cell::Generic) => format!("{head}: sin demo propio, se usa el de Batten"),
        Some(Cell::Missing) | None => format!("{head}: no existe"),
    }
}

/// Nota al pie: componentes de dotmax (librería, no tema) y demos sin proveedor reconocido.
fn note_text(matrix: &Matrix) -> Option<String> {
    let mut parts = Vec::new();
    // El recuento va primero: es corto, y al recortar la línea se pierde antes la lista.
    if !matrix.left_out.is_empty() {
        parts.push(format!(
            "{} demos sin proveedor reconocido",
            matrix.left_out.len()
        ));
    }
    if !matrix.library.is_empty() {
        parts.push(format!(
            "dotmax es una librería, no un tema: {}",
            matrix.library.join(", ")
        ));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// Recorta `text` a `width` caracteres (no bytes), terminando en `…` si hay que cortar.
pub fn fit(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut out: String = text.chars().take(width - 1).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo::Demo;
    use crate::demos;
    use batten::widgets::Theme;
    use ratatui::layout::Rect;
    use ratatui::Frame;

    /// Demo vacía: los tests de derivación no la pintan.
    struct Empty;

    impl Demo for Empty {
        fn title(&self) -> &str {
            ""
        }
        fn hint(&self) -> &str {
            ""
        }
        fn render(&mut self, _f: &mut Frame, _a: Rect, _t: &Theme) {}
    }

    /// Entrada de catálogo con demo vacía, construida igual que `with_demos` en app.rs.
    fn item(component: &'static str, variant: &'static str) -> CatalogItem {
        CatalogItem {
            component,
            variant,
            demo: Box::new(Empty),
        }
    }

    /// Índice de columna por nombre, para no depender de números mágicos en los tests.
    fn col(name: &str) -> usize {
        COLUMNS.iter().position(|(n, _)| *n == name).unwrap()
    }

    fn row<'a>(matrix: &'a Matrix, component: &str) -> &'a MatrixRow {
        matrix
            .rows
            .iter()
            .find(|row| row.component == component)
            .unwrap()
    }

    #[test]
    fn own_when_the_theme_has_a_demo() {
        let items = vec![
            item("Chip", "Batten"),
            item("Chip", "Crush"),
            item("Chip", "Crush · variante"),
        ];
        let matrix = build(&items);
        let chip = row(&matrix, "Chip");
        assert_eq!(chip.cells[col("Crush")], Cell::Own(2));
        assert_eq!(chip.cells[col("Batten")], Cell::Own(1));
    }

    #[test]
    fn generic_needs_a_batten_demo() {
        let items = vec![item("Shell", "Crush")];
        let matrix = build(&items);
        let shell = row(&matrix, "Shell");
        assert_eq!(shell.cells[col("Crush")], Cell::Own(1));
        assert_eq!(shell.cells[col("Pawan")], Cell::Missing);
        assert_eq!(target(&items, "Shell", col("Pawan")), None);
    }

    #[test]
    fn generic_when_only_batten_has_it() {
        let items = vec![item("Chat", "Batten")];
        let matrix = build(&items);
        let chat = row(&matrix, "Chat");
        assert_eq!(chat.cells[col("Pawan")], Cell::Generic);
        assert_eq!(chat.cells[col("CodeLoom")], Cell::Generic);
        assert_eq!(chat.cells[col("Batten")], Cell::Own(1));
    }

    #[test]
    fn an_unknown_provider_is_left_out_not_counted_as_batten() {
        let items = vec![
            item("Chip", "Batten"),
            item("Chip", "Cross-reference comparison"),
            item("Tool activity", "Cross-reference comparison"),
        ];
        let matrix = build(&items);
        assert_eq!(
            matrix.left_out,
            vec![
                ("Chip", "Cross-reference comparison"),
                ("Tool activity", "Cross-reference comparison"),
            ]
        );
        // Un demo mal nombrado no infla la columna Batten.
        assert_eq!(row(&matrix, "Chip").cells[col("Batten")], Cell::Own(1));
        // Un componente solo con demos sin proveedor reconocido no tiene fila.
        assert!(matrix
            .rows
            .iter()
            .all(|row| row.component != "Tool activity"));
    }

    #[test]
    fn dotmax_components_go_to_the_library_list() {
        let items = vec![
            item("Progress catalog", "dotmax · 60 temas"),
            item("Chip", "Batten"),
            item("Chip", "dotmax · prueba"),
        ];
        let matrix = build(&items);
        assert_eq!(matrix.library, vec!["Progress catalog"]);
        assert!(matrix
            .rows
            .iter()
            .all(|row| row.component != "Progress catalog"));
        // Un componente con demos de dotmax y de otro proveedor va a rows; dotmax no cuenta.
        let chip = row(&matrix, "Chip");
        assert_eq!(chip.cells[col("Batten")], Cell::Own(1));
        assert!(matrix.library.iter().all(|c| *c != "Chip"));
    }

    #[test]
    fn target_prefers_the_column_provider_then_batten() {
        let items = vec![
            item("Chat", "Batten"),
            item("Chat", "Crush"),
            item("Chat", "Crush · b"),
        ];
        assert_eq!(target(&items, "Chat", col("Crush")), Some(1));
        assert_eq!(target(&items, "Chat", col("Pawan")), Some(0));
        assert_eq!(target(&items, "Chat", col("MastraCode")), Some(0));
        assert_eq!(target(&items, "Chat", col("Batten")), Some(0));
        assert_eq!(target(&items, "Inexistente", col("Batten")), None);
    }

    #[test]
    fn mastracode_has_no_own_cells() {
        let matrix = build(&demos::catalog());
        let mastra = col("MastraCode");
        assert!(matrix
            .rows
            .iter()
            .all(|row| !matches!(row.cells[mastra], Cell::Own(_))));
    }

    #[test]
    fn crush_composer_is_own() {
        let matrix = build(&demos::catalog());
        let composer = row(&matrix, "Composer");
        assert!(matches!(composer.cells[col("Crush")], Cell::Own(n) if n >= 1));
        assert!(matches!(composer.cells[col("Batten")], Cell::Own(n) if n >= 1));
    }

    #[test]
    fn pawan_composer_is_generic() {
        let matrix = build(&demos::catalog());
        let composer = row(&matrix, "Composer");
        assert_eq!(composer.cells[col("Pawan")], Cell::Generic);
    }

    #[test]
    fn status_note_names_partial_implementation_for_generic() {
        assert_eq!(
            status_note("Composer", col("Pawan"), Cell::Generic),
            // ADAPTADO: texto abreviado para que quepa en el pie sin recortarse
            "◐ Composer · Pawan: parcialmente implementado (genérico de Batten con la paleta de Pawan)"
        );
    }

    #[test]
    fn status_note_for_own_mentions_variants_only_when_several() {
        assert_eq!(
            status_note("Chip", col("Crush"), Cell::Own(1)),
            "● Chip · Crush: demo propio"
        );
        assert_eq!(
            status_note("Chip", col("Crush"), Cell::Own(3)),
            "● Chip · Crush: 3 variantes propias"
        );
        assert_eq!(
            status_note("Chip", col("Pawan"), Cell::Missing),
            "· Chip · Pawan: no existe"
        );
    }

    // ADAPTADO: CodeLoom ya tiene preset
    #[test]
    fn codeloom_note_has_no_special_suffix() {
        assert_eq!(
            status_note("Chip", col("CodeLoom"), Cell::Own(1)),
            "● Chip · CodeLoom: demo propio"
        );
        assert_eq!(
            status_note("Chip", col("CodeLoom"), Cell::Missing),
            "· Chip · CodeLoom: no existe"
        );
        for cell in [Cell::Own(1), Cell::Generic, Cell::Missing] {
            assert!(!status_note("Chip", col("CodeLoom"), cell).contains("sin paleta"));
        }
        assert!(!status_note("Chip", col("Crush"), Cell::Own(1)).contains("sin paleta"));
    }

    #[test]
    fn every_target_points_to_an_existing_demo() {
        let items = demos::catalog();
        let matrix = build(&items);
        for row in &matrix.rows {
            for (column, cell) in row.cells.iter().enumerate() {
                match cell {
                    Cell::Missing => assert_eq!(target(&items, row.component, column), None),
                    Cell::Own(_) | Cell::Generic => {
                        let index = target(&items, row.component, column)
                            .unwrap_or_else(|| panic!("sin destino: {} #{column}", row.component));
                        assert!(index < items.len());
                        assert_eq!(items[index].component, row.component);
                    }
                }
            }
        }
    }
}
