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
        "ask_user" => ask_user(input),
        "subagent" => subagent(input),
        "subagent_control" => subagent_control(input),
        "job_start" => job_start(input),
        "job_output" | "job_kill" => job_ref(input),
        // `subagent_list` y `job_list` no reciben datos: su recuento sale de la salida.
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

/// Entrada de `ask_user`: `questions` no vacío. Los hechos son la primera pregunta
/// y su número de opciones, si `options` está presente (debe ser una lista).
fn ask_user(input: &Value) -> Option<ToolFacts> {
    let first = input.get("questions")?.as_array()?.first()?;
    let count = match first.get("options") {
        None => None,
        Some(options) => Some(options.as_array()?.len()),
    };
    Some(ToolFacts {
        primary: Some(text(first, "question")?),
        count,
        ..ToolFacts::default()
    })
}

/// Entrada de `subagent`: `prompt` obligatorio y `label` opcional. El resumen es
/// la etiqueta si la hay y no está en blanco; si no, el objetivo.
fn subagent(input: &Value) -> Option<ToolFacts> {
    let prompt = text(input, "prompt")?;
    let label = optional_text(input, "label")?.filter(|label| !label.trim().is_empty());
    Some(ToolFacts {
        primary: Some(label.unwrap_or(prompt)),
        ..ToolFacts::default()
    })
}

/// Entrada de `subagent_control`: `child_id` entero positivo y `action` obligatorios.
/// La acción es el primer dato y el id de destino el segundo.
fn subagent_control(input: &Value) -> Option<ToolFacts> {
    let child = input.get("child_id")?.as_u64().filter(|&id| id > 0)?;
    Some(ToolFacts {
        primary: Some(text(input, "action")?),
        secondary: Some(child.to_string()),
        ..ToolFacts::default()
    })
}

/// Entrada de `job_start`: `command` obligatorio; `label` no se muestra.
fn job_start(input: &Value) -> Option<ToolFacts> {
    Some(ToolFacts {
        primary: Some(text(input, "command")?),
        ..ToolFacts::default()
    })
}

/// Entrada de `job_output` y `job_kill`: solo el `id` del job.
fn job_ref(input: &Value) -> Option<ToolFacts> {
    Some(ToolFacts {
        primary: Some(text(input, "id")?),
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

    #[test]
    fn new_tools_extract_the_real_input_shapes() {
        let facts = tool_facts(
            "ask_user",
            &json!({ "questions": [
                {
                    "header": "DB",
                    "question": "¿Qué base de datos?",
                    "options": [
                        { "label": "pg" },
                        { "label": "sqlite", "description": "local" },
                        { "label": "mysql" }
                    ],
                    "multi_select": false
                },
                { "question": "¿Y la caché?", "options": [{ "label": "redis" }, { "label": "no" }] }
            ] }),
        );
        assert_eq!(facts.primary.as_deref(), Some("¿Qué base de datos?"));
        assert_eq!(facts.count, Some(3));

        // La etiqueta del hijo es el resumen; sin ella, el objetivo (prompt).
        let facts = tool_facts(
            "subagent",
            &json!({ "prompt": "revisa los tests", "label": "tests" }),
        );
        assert_eq!(facts.primary.as_deref(), Some("tests"));
        let facts = tool_facts("subagent", &json!({ "prompt": "revisa los tests" }));
        assert_eq!(facts.primary.as_deref(), Some("revisa los tests"));

        let facts = tool_facts(
            "subagent_control",
            &json!({ "child_id": 2, "action": "answer", "answers": ["Sí"] }),
        );
        assert_eq!(facts.primary.as_deref(), Some("answer"));
        assert_eq!(facts.secondary.as_deref(), Some("2"));

        let facts = tool_facts(
            "job_start",
            &json!({ "command": "cargo test", "label": "tests" }),
        );
        assert_eq!(facts.primary.as_deref(), Some("cargo test"));
        let facts = tool_facts("job_output", &json!({ "id": "job-7", "from": 0 }));
        assert_eq!(facts.primary.as_deref(), Some("job-7"));
        let facts = tool_facts("job_kill", &json!({ "id": "job-7" }));
        assert_eq!(facts.primary.as_deref(), Some("job-7"));

        // Las listas no reciben datos de entrada: sus hechos quedan vacíos.
        assert_eq!(
            tool_facts("subagent_list", &json!({})),
            ToolFacts::default()
        );
        assert_eq!(tool_facts("job_list", &json!({})), ToolFacts::default());
    }

    #[test]
    fn malformed_input_for_the_new_tools_yields_default() {
        let shapes = [json!(null), json!(42), json!([]), json!({})];
        let names = [
            "ask_user",
            "subagent",
            "subagent_control",
            "job_start",
            "job_output",
            "job_kill",
        ];
        for name in names {
            for input in &shapes {
                assert_eq!(
                    tool_facts(name, input),
                    ToolFacts::default(),
                    "{name} con {input}"
                );
            }
        }

        let wrong_types = [
            ("ask_user", json!({ "questions": "x" })),
            ("ask_user", json!({ "questions": [] })),
            (
                "ask_user",
                json!({ "questions": [{ "question": 5, "options": [] }] }),
            ),
            (
                "ask_user",
                json!({ "questions": [{ "question": "¿Sí?", "options": "dos" }] }),
            ),
            ("subagent", json!({ "prompt": 3 })),
            ("subagent", json!({ "prompt": "x", "label": 4 })),
            (
                "subagent_control",
                json!({ "child_id": "2", "action": "interrupt" }),
            ),
            (
                "subagent_control",
                json!({ "child_id": 0, "action": "interrupt" }),
            ),
            ("subagent_control", json!({ "child_id": 2 })),
            ("subagent_control", json!({ "child_id": 2, "action": 5 })),
            ("job_start", json!({ "command": ["ls"] })),
            ("job_output", json!({ "id": 7 })),
            ("job_kill", json!({ "id": null })),
        ];
        for (name, input) in wrong_types {
            assert_eq!(
                tool_facts(name, &input),
                ToolFacts::default(),
                "{name} con {input}"
            );
        }
    }
}
