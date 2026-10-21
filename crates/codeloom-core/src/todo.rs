//! Lista de tareas de la sesión, que el modelo reemplaza entera con `todo_write`.

use serde_json::Value;

/// Número máximo de tareas admitidas en una sola lista.
pub const MAX_TODOS: usize = 50;

/// Estado de una tarea.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

/// Una tarea de la lista.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoItem {
    pub content: String,
    pub status: TodoStatus,
}

/// Interpreta la entrada `{"todos": [...]}` de `todo_write`.
///
/// Los errores son mensajes legibles para el modelo; el contenido se recorta
/// antes de validarlo, de modo que un contenido en blanco se rechaza.
pub fn parse_todos(input: &Value) -> Result<Vec<TodoItem>, String> {
    let todos = input["todos"]
        .as_array()
        .ok_or_else(|| "todos must be an array".to_string())?;
    if todos.len() > MAX_TODOS {
        return Err(format!("too many todos (max {MAX_TODOS})"));
    }
    todos
        .iter()
        .enumerate()
        .map(|(i, raw)| {
            let n = i + 1;
            let content = raw["content"].as_str().unwrap_or_default().trim();
            if content.is_empty() {
                return Err(format!("todo {n}: empty content"));
            }
            let status = match raw["status"].as_str().unwrap_or_default() {
                "pending" => TodoStatus::Pending,
                "in_progress" => TodoStatus::InProgress,
                "completed" => TodoStatus::Completed,
                other => return Err(format!("todo {n}: invalid status '{other}'")),
            };
            Ok(TodoItem {
                content: content.to_string(),
                status,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_valid_list_and_trims() {
        let items = parse_todos(&json!({"todos": [
            {"content": "  Primera  ", "status": "completed"},
            {"content": "Segunda", "status": "in_progress"},
            {"content": "Tercera", "status": "pending"}
        ]}))
        .unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].content, "Primera");
        assert_eq!(items[0].status, TodoStatus::Completed);
        assert_eq!(items[1].status, TodoStatus::InProgress);
        assert_eq!(items[2].status, TodoStatus::Pending);
    }

    #[test]
    fn empty_list_is_valid() {
        let items = parse_todos(&json!({"todos": []})).unwrap();
        assert!(items.is_empty());
    }

    #[test]
    fn rejects_non_array() {
        let err = parse_todos(&json!({"todos": "nope"})).unwrap_err();
        assert_eq!(err, "todos must be an array");
    }

    #[test]
    fn rejects_more_than_fifty() {
        let many: Vec<_> = (0..51)
            .map(|i| json!({"content": format!("t{i}"), "status": "pending"}))
            .collect();
        let err = parse_todos(&json!({ "todos": many })).unwrap_err();
        assert_eq!(err, "too many todos (max 50)");
    }

    #[test]
    fn rejects_empty_content() {
        let err = parse_todos(&json!({"todos": [
            {"content": "ok", "status": "pending"},
            {"content": "   ", "status": "pending"}
        ]}))
        .unwrap_err();
        assert_eq!(err, "todo 2: empty content");
    }

    #[test]
    fn rejects_unknown_status() {
        let err = parse_todos(&json!({"todos": [
            {"content": "algo", "status": "wip"}
        ]}))
        .unwrap_err();
        assert_eq!(err, "todo 1: invalid status 'wip'");
    }
}
