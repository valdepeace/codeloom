//! Derivación pura de la matriz de componentes por tema del showcase.
//!
//! Filas = componentes; columnas = temas. Cada celda dice si el tema tiene reproducción propia
//! (`Own`), si solo existe el widget genérico de Batten pintado con su paleta (`Generic`) o si no
//! existe (`Missing`). No depende de la terminal: se prueba directamente sobre el catálogo.

use crate::demos::{strict_provider, CatalogItem};
use batten::widgets::ThemePreset;

/// Estado de una celda de la matriz.
// Puntos de entrada del modo matriz (Tarea 2); hasta que se usen, solo los tests los ejercitan.
#[allow(dead_code)]
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
// Lo usará el modo matriz (Tarea 2); hasta entonces solo lo ejercitan los tests.
#[allow(dead_code)]
pub const COLUMNS: [(&str, Option<ThemePreset>); 7] = [
    ("Batten", Some(ThemePreset::Batten)),
    ("Martty", Some(ThemePreset::Martty)),
    ("Mindfork", Some(ThemePreset::Mindfork)),
    ("Pawan", Some(ThemePreset::Pawan)),
    ("Crush", Some(ThemePreset::Crush)),
    ("MastraCode", Some(ThemePreset::MastraCode)),
    ("CodeLoom", None),
];

/// Proveedor de la columna Batten, usado como respaldo para las celdas `Generic`.
const BATTEN: &str = "Batten";

// Lo usará el modo matriz (Tarea 2); hasta entonces solo lo ejercitan los tests.
#[allow(dead_code)]
pub struct Matrix {
    /// Componentes con al menos un demo de un proveedor de columna, en el orden de primera
    /// aparición en el catálogo.
    pub rows: Vec<MatrixRow>,
    /// (componente, variante) cuyo proveedor no se reconoce; no cuentan en ninguna celda.
    pub left_out: Vec<(&'static str, &'static str)>,
    /// Componentes cuyos demos son solo de dotmax (librería, no tema), sin repetir.
    pub library: Vec<&'static str>,
}

// Lo usará el modo matriz (Tarea 2); hasta entonces solo lo ejercitan los tests.
#[allow(dead_code)]
pub struct MatrixRow {
    pub component: &'static str,
    pub cells: [Cell; 7],
}

/// Deriva la matriz a partir del catálogo.
// Lo usará el modo matriz (Tarea 2); hasta entonces solo lo ejercitan los tests.
#[allow(dead_code)]
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
// Lo usará el modo matriz (Tarea 2); hasta entonces solo lo ejercitan los tests.
#[allow(dead_code)]
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
fn provider_of_column(column: usize) -> Option<&'static str> {
    COLUMNS.get(column).map(|(name, _)| *name)
}

/// Indica si un proveedor corresponde a una de las columnas de la matriz.
fn is_column(provider: &str) -> bool {
    COLUMNS.iter().any(|(name, _)| *name == provider)
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
            provider: "Test",
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
