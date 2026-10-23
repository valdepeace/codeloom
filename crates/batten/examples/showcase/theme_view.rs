//! Lista de componentes de un tema para la barra lateral del showcase.
//!
//! Deriva de la matriz (`matrix::build`) los componentes que el tema tiene con reproducción propia
//! (`Own`) o solo con el genérico de Batten (`Generic`), y cuenta los que no existen (`Missing`).
//! No depende de la terminal: se prueba directamente sobre el catálogo.

use crate::demos::{strict_provider, CatalogItem};
use crate::matrix::{self, Cell, BATTEN};

/// Componente de un tema, con sus ejemplos en ese tema.
pub struct ThemeEntry {
    /// Nombre del componente, tal como aparece en la matriz.
    pub component: &'static str,
    /// `true` si el tema no tiene demo propio y solo se muestra el genérico de Batten (◐).
    pub generic: bool,
    /// Ejemplos del componente en este tema; siempre al menos uno.
    pub examples: Vec<ThemeExample>,
}

/// Un ejemplo de un componente dentro de un tema.
pub struct ThemeExample {
    /// Índice del demo en el catálogo (`items`).
    pub catalog_index: usize,
    /// Variante sin el prefijo del proveedor; `principal` si no queda texto.
    pub label: String,
}

/// Vista de un tema: sus componentes, en el orden de la matriz, y cuántos no existen en él.
pub struct ThemeView {
    /// Componentes con demo propio o genérico en el tema, en el orden de las filas de la matriz.
    pub entries: Vec<ThemeEntry>,
    /// Componentes que no existen en el tema; no se listan, pero se cuentan.
    // Sin lectura en el binario hasta la Tarea 3 (pie con el recuento); lo leen los tests.
    #[allow(dead_code)]
    pub missing: usize,
}

/// Deriva la vista del tema de la columna `column` (índice en `matrix::COLUMNS`).
///
/// Una columna fuera de rango produce una vista vacía.
pub fn theme_view(items: &[CatalogItem], column: usize) -> ThemeView {
    let Some(provider) = matrix::provider_of_column(column) else {
        return ThemeView {
            entries: Vec::new(),
            missing: 0,
        };
    };
    let matrix = matrix::build(items);
    let mut entries = Vec::new();
    let mut missing = 0;
    for row in &matrix.rows {
        // Con reproducción propia se listan los demos del proveedor de la columna; con solo el
        // genérico, los de Batten. Lo que no existe no entra en la lista.
        let (source, generic) = match row.cells[column] {
            Cell::Own(_) => (provider, false),
            Cell::Generic => (BATTEN, true),
            Cell::Missing => {
                missing += 1;
                continue;
            }
        };
        let examples = items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                item.component == row.component && strict_provider(item.variant) == Some(source)
            })
            .map(|(catalog_index, item)| ThemeExample {
                catalog_index,
                label: label(item.variant),
            })
            .collect();
        entries.push(ThemeEntry {
            component: row.component,
            generic,
            examples,
        });
    }
    ThemeView { entries, missing }
}

/// Texto de un ejemplo: la variante sin el prefijo del proveedor que la inicia y sin separadores
/// iniciales. Si no queda texto, `principal`.
fn label(variant: &str) -> String {
    let prefix = strict_provider(variant).unwrap_or("");
    let rest = variant[prefix.len()..]
        .trim_start_matches([' ', '·', '-', ':'])
        .trim_end();
    if rest.is_empty() {
        "principal".to_string()
    } else {
        rest.to_string()
    }
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
        matrix::COLUMNS
            .iter()
            .position(|(n, _)| *n == name)
            .unwrap()
    }

    /// Entrada de un componente en la vista, si el tema lo lista.
    fn entry<'a>(view: &'a ThemeView, component: &str) -> Option<&'a ThemeEntry> {
        view.entries.iter().find(|e| e.component == component)
    }

    #[test]
    fn crush_lists_its_own_components() {
        let items = demos::catalog();
        let view = theme_view(&items, col("Crush"));
        let composer = entry(&view, "Composer").expect("Composer en Crush");
        assert!(!composer.generic);
        for example in &composer.examples {
            assert_eq!(
                strict_provider(items[example.catalog_index].variant),
                Some("Crush")
            );
        }
    }

    #[test]
    fn pawan_lists_composer_as_generic_with_the_batten_demo() {
        let items = demos::catalog();
        let view = theme_view(&items, col("Pawan"));
        let composer = entry(&view, "Composer").expect("Composer en Pawan");
        assert!(composer.generic);
        assert!(!composer.examples.is_empty());
        for example in &composer.examples {
            assert_eq!(
                strict_provider(items[example.catalog_index].variant),
                Some("Batten")
            );
        }
    }

    #[test]
    fn missing_components_are_not_listed_but_counted() {
        // Chip tiene Batten y por tanto genérico en Pawan; Shell solo Crush, así que falta.
        let items = vec![item("Chip", "Batten"), item("Shell", "Crush")];
        let view = theme_view(&items, col("Pawan"));
        assert_eq!(view.entries.len(), 1);
        assert_eq!(view.missing, 1);
        assert!(entry(&view, "Shell").is_none());
    }

    #[test]
    fn label_strips_the_provider_prefix() {
        let items = vec![
            item("Chip", "Crush · pending"),
            item("Chip", "Crush"),
            item("Chip", "Crush:compacto"),
        ];
        let view = theme_view(&items, col("Crush"));
        let chip = entry(&view, "Chip").expect("Chip en Crush");
        let labels: Vec<&str> = chip.examples.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(labels, ["pending", "principal", "compacto"]);
    }

    #[test]
    fn mastracode_has_only_generic_entries() {
        let items = demos::catalog();
        let view = theme_view(&items, col("MastraCode"));
        assert!(!view.entries.is_empty());
        assert!(view.entries.iter().all(|e| e.generic));
    }

    #[test]
    fn batten_column_has_no_generic_entries() {
        let items = demos::catalog();
        let view = theme_view(&items, col("Batten"));
        assert!(!view.entries.is_empty());
        assert!(view.entries.iter().all(|e| !e.generic));
    }

    #[test]
    fn every_example_points_to_an_existing_demo() {
        let items = demos::catalog();
        for (column, (provider, _)) in matrix::COLUMNS.iter().enumerate() {
            let view = theme_view(&items, column);
            for entry in &view.entries {
                assert!(
                    !entry.examples.is_empty(),
                    "{} sin ejemplos",
                    entry.component
                );
                let source = if entry.generic { BATTEN } else { provider };
                for example in &entry.examples {
                    assert!(example.catalog_index < items.len());
                    let demo = &items[example.catalog_index];
                    assert_eq!(demo.component, entry.component);
                    assert_eq!(strict_provider(demo.variant), Some(source));
                    assert!(!example.label.is_empty());
                }
            }
        }
    }

    #[test]
    fn entries_plus_missing_equal_the_matrix_rows() {
        let items = demos::catalog();
        let rows = matrix::build(&items).rows.len();
        for column in 0..matrix::COLUMNS.len() {
            let view = theme_view(&items, column);
            assert_eq!(view.entries.len() + view.missing, rows, "columna {column}");
        }
    }
}
