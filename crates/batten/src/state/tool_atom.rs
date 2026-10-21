//! Piezas atómicas por herramienta: cómo se representa una llamada (`ToolItem`)
//! en el transcript a partir de sus hechos tipados (`ToolFacts`).
//!
//! Este módulo no conoce el JSON de entrada: los hechos los extrae el adaptador
//! que vive fuera de `batten` (`codeloom-tui`), de modo que la pieza solo pinta
//! hechos y estado.

use super::{tool_lines, Line, ToolItem};

/// Hechos tipados de una llamada a herramienta, ya extraídos de su entrada.
///
/// Cada campo es opcional: una pieza muestra lo que el adaptador haya podido
/// extraer y degrada con elegancia cuando falte algo. La interpretación exacta
/// de cada campo depende de la herramienta que lo rellena.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolFacts {
    /// Ruta, comando o patrón principal de la llamada.
    pub primary: Option<String>,
    /// Segundo dato corto (p. ej. el filtro `glob` de `grep`).
    pub secondary: Option<String>,
    /// Líneas de inicio y fin (`view`), ambas incluidas.
    pub range: Option<(usize, usize)>,
    /// Texto previo (`edit`).
    pub before: Option<String>,
    /// Texto nuevo (`edit`).
    pub after: Option<String>,
    /// Recuento (coincidencias, tareas…).
    pub count: Option<usize>,
    /// Resto de pares clave-valor que merece la pena mostrar.
    pub items: Vec<(String, String)>,
}

/// Pieza de render de una herramienta: convierte la llamada y sus hechos en
/// líneas del transcript. Las piezas no parsean JSON ni conocen el protocolo.
pub trait ToolAtom: Sync {
    fn lines(&self, item: &ToolItem, facts: &ToolFacts) -> Vec<Line>;
}

/// Pieza de reserva para las herramientas sin pieza propia (y las MCP). Reproduce
/// `tool_lines` exactamente, ignorando los hechos.
struct GenericAtom;

impl ToolAtom for GenericAtom {
    fn lines(&self, item: &ToolItem, _facts: &ToolFacts) -> Vec<Line> {
        tool_lines(item)
    }
}

static GENERIC: GenericAtom = GenericAtom;

/// Pieza registrada para `name`. De momento todas las herramientas usan la
/// pieza genérica; las piezas propias se añadirán aquí por nombre.
pub fn atom_for(_name: &str) -> &'static dyn ToolAtom {
    &GENERIC
}

/// Líneas de la llamada según su pieza y sus hechos.
pub fn tool_lines_with(item: &ToolItem, facts: &ToolFacts) -> Vec<Line> {
    atom_for(&item.name).lines(item, facts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{LineKind, ToolStatus};

    /// Llamadas representativas de cada estado que `tool_lines` distingue.
    fn sample_items() -> Vec<ToolItem> {
        let mut pending = ToolItem::new("c0", "bash");
        pending.set_summary("ls");

        let mut running = ToolItem::new("c1", "bash");
        running.set_summary("cargo test");
        running.start();

        let mut short = ToolItem::new("c2", "bash");
        short.set_summary("echo hi");
        short.set_output("a\nb");

        let mut folded = ToolItem::new("c3", "bash");
        folded.set_result(false);
        folded.set_output("1\n2\n3\n4\n5");

        let mut expanded = ToolItem::new("c4", "bash");
        expanded.set_result(true);
        expanded.set_output("1\n2\n3\n4");
        expanded.toggle_expanded();

        let mut failed = ToolItem::new("c5", "edit");
        failed.set_summary("src/lib.rs");
        failed.set_result(true);
        failed.set_output("old text not found");

        let mut waiting = ToolItem::new("c6", "bash");
        waiting.set_summary("rm -rf build");
        waiting.start();
        waiting.await_permission();

        let mut canceled = ToolItem::new("c7", "view");
        canceled.set_summary("src/main.rs");
        canceled.start();
        canceled.cancel();

        vec![
            pending, running, short, folded, expanded, failed, waiting, canceled,
        ]
    }

    #[test]
    fn generic_atom_matches_the_current_tool_lines() {
        let facts = ToolFacts::default();
        for item in sample_items() {
            assert_eq!(
                tool_lines_with(&item, &facts),
                super::super::tool_lines(&item),
                "distinto para la llamada {} ({})",
                item.call_id,
                item.name
            );
        }
    }

    #[test]
    fn generic_atom_ignores_facts() {
        // Con hechos presentes la pieza genérica sigue pintando lo mismo: el
        // aspecto de las herramientas sin pieza propia no cambia.
        let facts = ToolFacts {
            primary: Some("ls -la".to_string()),
            count: Some(3),
            ..ToolFacts::default()
        };
        for item in sample_items() {
            assert_eq!(
                tool_lines_with(&item, &facts),
                super::super::tool_lines(&item)
            );
        }
    }

    #[test]
    fn unknown_tool_falls_back_to_generic() {
        let mut item = ToolItem::new("c1", "mcp__github__create_issue");
        item.set_summary("{\"title\":\"x\"}");
        item.set_result(false);
        item.set_output("created");
        let lines = atom_for(&item.name).lines(&item, &ToolFacts::default());
        assert_eq!(lines, super::super::tool_lines(&item));
        assert_eq!(lines[0].kind, LineKind::ToolHead(ToolStatus::Success));
    }

    #[test]
    fn facts_default_is_empty() {
        let facts = ToolFacts::default();
        assert_eq!(facts.primary, None);
        assert_eq!(facts.secondary, None);
        assert_eq!(facts.range, None);
        assert_eq!(facts.before, None);
        assert_eq!(facts.after, None);
        assert_eq!(facts.count, None);
        assert!(facts.items.is_empty());
    }
}
