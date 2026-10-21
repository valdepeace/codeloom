//! Adaptador entre la entrada JSON de una llamada a herramienta y los hechos
//! tipados (`ToolFacts`) que consumen las piezas de `batten`.
//!
//! Vive aquí y no en `batten` para que el crate de widgets no dependa de
//! `serde_json`. Solo se ejecuta al fijar la entrada de la herramienta, nunca
//! en el camino de los deltas ni al pintar.

use batten::state::ToolFacts;
use serde_json::Value;

/// Hechos de la llamada `name` a partir de su entrada `input`. Las herramientas
/// sin adaptador propio, y las entradas mal formadas (campo obligatorio ausente
/// o campo presente con un tipo inesperado), devuelven `ToolFacts::default()`.
pub fn tool_facts(name: &str, input: &Value) -> ToolFacts {
    let facts = match name {
        "bash" => bash(input),
        "view" => view(input),
        "edit" => edit(input),
        "grep" => grep(input),
        "glob" => glob(input),
        _ => None,
    };
    facts.unwrap_or_default()
}

/// Campo de cadena obligatorio. `None` si falta o no es una cadena.
fn text(input: &Value, key: &str) -> Option<String> {
    input.get(key)?.as_str().map(str::to_owned)
}

/// Campo de cadena opcional. Ausente produce `Some(None)`; de otro tipo, `None`
/// (entrada mal formada).
fn optional_text(input: &Value, key: &str) -> Option<Option<String>> {
    match input.get(key) {
        None => Some(None),
        Some(value) => value.as_str().map(|s| Some(s.to_owned())),
    }
}

/// Campo entero opcional, con el mismo criterio que `optional_text`.
fn optional_index(input: &Value, key: &str) -> Option<Option<usize>> {
    match input.get(key) {
        None => Some(None),
        Some(value) => {
            let number = usize::try_from(value.as_u64()?).ok()?;
            Some(Some(number))
        }
    }
}

fn bash(input: &Value) -> Option<ToolFacts> {
    Some(ToolFacts {
        primary: Some(text(input, "command")?),
        ..ToolFacts::default()
    })
}

/// Entrada de `view`: `path` obligatorio, `offset` y `limit` opcionales. Las
/// líneas se numeran desde `offset + 1`, como en `codeloom-core`. Sin `limit`
/// no hay fin conocido y no se informa rango.
fn view(input: &Value) -> Option<ToolFacts> {
    let path = text(input, "path")?;
    let offset = optional_index(input, "offset")?;
    let limit = optional_index(input, "limit")?;
    let range = limit.filter(|&limit| limit > 0).map(|limit| {
        let start = offset.unwrap_or(0).saturating_add(1);
        (start, start.saturating_add(limit).saturating_sub(1))
    });
    Some(ToolFacts {
        primary: Some(path),
        range,
        ..ToolFacts::default()
    })
}

fn edit(input: &Value) -> Option<ToolFacts> {
    Some(ToolFacts {
        primary: Some(text(input, "path")?),
        before: Some(text(input, "old")?),
        after: Some(text(input, "new")?),
        ..ToolFacts::default()
    })
}

/// Entrada de `grep`: `pattern` obligatorio y `glob` opcional como filtro.
fn grep(input: &Value) -> Option<ToolFacts> {
    Some(ToolFacts {
        primary: Some(text(input, "pattern")?),
        secondary: optional_text(input, "glob")?,
        ..ToolFacts::default()
    })
}

fn glob(input: &Value) -> Option<ToolFacts> {
    Some(ToolFacts {
        primary: Some(text(input, "pattern")?),
        ..ToolFacts::default()
    })
}

#[cfg(test)]
mod tests {
    use batten::state::ToolFacts;
    use serde_json::json;

    use super::tool_facts;

    #[test]
    fn malformed_input_yields_default_facts() {
        let shapes = [json!(null), json!(42), json!([]), json!({})];
        for name in ["bash", "view", "edit", "grep", "glob"] {
            for input in &shapes {
                assert_eq!(
                    tool_facts(name, input),
                    ToolFacts::default(),
                    "{name} con {input}"
                );
            }
        }

        // Campos obligatorios ausentes o de tipo inesperado, por herramienta.
        let wrong_types = [
            ("bash", json!({ "command": 5 })),
            ("view", json!({ "path": ["a.rs"] })),
            ("view", json!({ "path": "a.rs", "offset": "tres" })),
            ("edit", json!({ "path": "a.rs", "old": "x" })),
            ("grep", json!({ "pattern": "x", "glob": false })),
            ("glob", json!({ "pattern": null })),
        ];
        for (name, input) in wrong_types {
            assert_eq!(
                tool_facts(name, &input),
                ToolFacts::default(),
                "{name} con {input}"
            );
        }
    }

    #[test]
    fn unknown_tool_yields_default_facts() {
        let input = json!({ "title": "x", "body": "y" });
        assert_eq!(
            tool_facts("mcp__github__create_issue", &input),
            ToolFacts::default()
        );
        assert_eq!(tool_facts("todo_write", &input), ToolFacts::default());
    }

    #[test]
    fn bash_input_extracts_command() {
        let facts = tool_facts("bash", &json!({ "command": "cargo test -p batten" }));
        assert_eq!(facts.primary.as_deref(), Some("cargo test -p batten"));
        assert_eq!(facts.secondary, None);
    }

    #[test]
    fn view_input_extracts_path_and_range() {
        let facts = tool_facts(
            "view",
            &json!({ "path": "src/lib.rs", "offset": 9, "limit": 20 }),
        );
        assert_eq!(facts.primary.as_deref(), Some("src/lib.rs"));
        assert_eq!(facts.range, Some((10, 29)));

        // Sin paginación la vista empieza en la primera línea y no hay rango.
        let facts = tool_facts("view", &json!({ "path": "a.txt" }));
        assert_eq!(facts.primary.as_deref(), Some("a.txt"));
        assert_eq!(facts.range, None);
    }

    #[test]
    fn edit_input_extracts_path_and_before_after() {
        let facts = tool_facts(
            "edit",
            &json!({ "path": "a.txt", "old": "world", "new": "rust" }),
        );
        assert_eq!(facts.primary.as_deref(), Some("a.txt"));
        assert_eq!(facts.before.as_deref(), Some("world"));
        assert_eq!(facts.after.as_deref(), Some("rust"));
    }

    #[test]
    fn grep_input_extracts_pattern_and_glob() {
        // La entrada real de `grep` no tiene `path`: su segundo dato es el filtro `glob`.
        let facts = tool_facts("grep", &json!({ "pattern": "fn \\w+", "glob": "*.rs" }));
        assert_eq!(facts.primary.as_deref(), Some("fn \\w+"));
        assert_eq!(facts.secondary.as_deref(), Some("*.rs"));

        let facts = tool_facts("grep", &json!({ "pattern": "TODO" }));
        assert_eq!(facts.primary.as_deref(), Some("TODO"));
        assert_eq!(facts.secondary, None);
    }

    #[test]
    fn glob_input_extracts_pattern() {
        let facts = tool_facts("glob", &json!({ "pattern": "crates/**/*.rs" }));
        assert_eq!(facts.primary.as_deref(), Some("crates/**/*.rs"));
        assert_eq!(facts.secondary, None);
    }
}
