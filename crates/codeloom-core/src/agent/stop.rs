use crate::message::{ContentPart, Message, Role};

fn signature(m: &Message) -> Option<Vec<String>> {
    let calls: Vec<String> = m
        .parts
        .iter()
        .filter_map(|p| match p {
            ContentPart::ToolCall { name, input, .. } => Some(format!("{name}:{input}")),
            _ => None,
        })
        .collect();
    if calls.is_empty() {
        None
    } else {
        Some(calls)
    }
}

/// Para si se supero `max_steps` o si los ultimos dos mensajes del asistente
/// repiten las mismas tool calls (bucle).
pub fn should_stop(history: &[Message], steps: usize, max_steps: usize) -> bool {
    if steps >= max_steps {
        return true;
    }
    let sigs: Vec<Vec<String>> = history
        .iter()
        .filter(|m| m.role == Role::Assistant)
        .filter_map(signature)
        .collect();
    if sigs.len() >= 2 && sigs[sigs.len() - 1] == sigs[sigs.len() - 2] {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn with_call(name: &str, input: serde_json::Value) -> Message {
        Message::assistant(vec![ContentPart::ToolCall {
            id: "x".into(),
            name: name.into(),
            input,
        }])
    }

    #[test]
    fn stops_on_step_budget() {
        assert!(should_stop(&[], 5, 5));
    }

    #[test]
    fn stops_on_repeated_tool_calls() {
        let h = vec![
            with_call("view", json!({"path":"a"})),
            with_call("view", json!({"path":"a"})),
        ];
        assert!(should_stop(&h, 1, 10));
    }

    #[test]
    fn continues_when_calls_differ() {
        let h = vec![
            with_call("view", json!({"path":"a"})),
            with_call("view", json!({"path":"b"})),
        ];
        assert!(!should_stop(&h, 1, 10));
    }
}
