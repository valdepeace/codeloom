//! Limpieza de resultados de herramienta antiguos en la petición al proveedor.
//!
//! Solo afecta a la copia que se envía al modelo: el historial guardado y el almacén no se tocan.

use crate::message::{ContentPart, Message};

/// Número de resultados de herramienta más recientes que se conservan íntegros.
pub const KEEP_TOOL_RESULTS: usize = 6;

/// Por debajo de este tamaño (en bytes) un resultado antiguo no compensa sustituirlo.
const CLEAR_MIN_BYTES: usize = 256;

/// Devuelve una copia del historial en la que los resultados de herramienta anteriores a los
/// `keep` más recientes, y de más de `CLEAR_MIN_BYTES` bytes, se sustituyen por un marcador.
///
/// Se conservan el orden, el número de partes, los `call_id` y el indicador de error.
pub fn clear_old_tool_results(history: &[Message], keep: usize) -> Vec<Message> {
    let total = history
        .iter()
        .flat_map(|m| m.parts.iter())
        .filter(|p| matches!(p, ContentPart::ToolResult { .. }))
        .count();
    // Los resultados con índice inferior a `cutoff`, contando desde el primero, son los antiguos.
    let cutoff = total.saturating_sub(keep);

    let mut ordinal = 0usize;
    let mut out = Vec::with_capacity(history.len());
    for message in history {
        let mut parts = Vec::with_capacity(message.parts.len());
        for part in &message.parts {
            match part {
                ContentPart::ToolResult {
                    call_id,
                    content,
                    is_error,
                } => {
                    let index = ordinal;
                    ordinal += 1;
                    if index < cutoff && content.len() > CLEAR_MIN_BYTES {
                        parts.push(ContentPart::ToolResult {
                            call_id: call_id.clone(),
                            content: format!(
                                "[tool result cleared: {} bytes; run the tool again if you need it]",
                                content.len()
                            ),
                            is_error: *is_error,
                        });
                    } else {
                        parts.push(part.clone());
                    }
                }
                _ => parts.push(part.clone()),
            }
        }
        out.push(Message {
            id: message.id,
            role: message.role,
            parts,
            usage: message.usage.clone(),
            created_at_ms: message.created_at_ms,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::Role;

    fn result(call_id: &str, content: String, is_error: bool) -> ContentPart {
        ContentPart::ToolResult {
            call_id: call_id.into(),
            content,
            is_error,
        }
    }

    fn result_msg(call_id: &str, bytes: usize) -> Message {
        Message::new(Role::Tool, vec![result(call_id, "x".repeat(bytes), false)])
    }

    fn contents(messages: &[Message]) -> Vec<String> {
        messages
            .iter()
            .flat_map(|m| m.parts.iter())
            .filter_map(|p| match p {
                ContentPart::ToolResult { content, .. } => Some(content.clone()),
                _ => None,
            })
            .collect()
    }

    fn marker(bytes: usize) -> String {
        format!(
            "[tool result cleared: {} bytes; run the tool again if you need it]",
            bytes
        )
    }

    #[test]
    fn clears_only_results_older_than_the_last_six() {
        let history: Vec<Message> = (0..10)
            .map(|i| result_msg(&format!("c{i}"), 1000))
            .collect();

        let out = clear_old_tool_results(&history, KEEP_TOOL_RESULTS);
        let got = contents(&out);

        assert_eq!(got.len(), 10);
        for content in &got[..4] {
            assert_eq!(content, &marker(1000));
        }
        for content in &got[4..] {
            assert_eq!(content.len(), 1000);
            assert!(content.chars().all(|c| c == 'x'));
        }
    }

    #[test]
    fn small_old_results_are_kept() {
        let history = vec![
            result_msg("c0", 256),
            result_msg("c1", 257),
            result_msg("c2", 1000),
            result_msg("c3", 1000),
            result_msg("c4", 1000),
            result_msg("c5", 1000),
            result_msg("c6", 1000),
            result_msg("c7", 1000),
        ];

        let out = clear_old_tool_results(&history, KEEP_TOOL_RESULTS);
        let got = contents(&out);

        // El de 256 bytes es antiguo pero no supera el umbral: se queda tal cual.
        assert_eq!(got[0].len(), 256);
        // El de 257 bytes sí supera el umbral: se sustituye.
        assert_eq!(got[1], marker(257));
    }

    #[test]
    fn call_ids_and_order_are_preserved() {
        let history = vec![
            Message::user("hola"),
            Message::new(
                Role::Tool,
                vec![
                    result("c0", "x".repeat(1000), true),
                    result("c1", "y".repeat(1000), false),
                ],
            ),
            Message::assistant(vec![
                ContentPart::Text {
                    text: "t".repeat(1000),
                },
                ContentPart::ToolCall {
                    id: "c2".into(),
                    name: "noop".into(),
                    input: serde_json::json!({}),
                },
            ]),
            Message::new(Role::Tool, vec![result("c2", "z".repeat(1000), false)]),
            result_msg("c3", 1000),
            result_msg("c4", 1000),
            result_msg("c5", 1000),
            result_msg("c6", 1000),
            result_msg("c7", 1000),
        ];

        let out = clear_old_tool_results(&history, KEEP_TOOL_RESULTS);

        assert_eq!(out.len(), history.len());
        for (before, after) in history.iter().zip(out.iter()) {
            assert_eq!(before.role, after.role);
            assert_eq!(before.parts.len(), after.parts.len());
            for (p_before, p_after) in before.parts.iter().zip(after.parts.iter()) {
                match (p_before, p_after) {
                    (
                        ContentPart::ToolResult {
                            call_id: a,
                            is_error: ea,
                            ..
                        },
                        ContentPart::ToolResult {
                            call_id: b,
                            is_error: eb,
                            ..
                        },
                    ) => {
                        assert_eq!(a, b);
                        assert_eq!(ea, eb);
                    }
                    (ContentPart::ToolResult { .. }, _) | (_, ContentPart::ToolResult { .. }) => {
                        panic!("un resultado no puede cambiar de tipo")
                    }
                    _ => assert_eq!(p_before, p_after),
                }
            }
        }
        // El resultado c0 (antiguo, error) se ha limpiado y conserva su indicador de error.
        match &out[1].parts[0] {
            ContentPart::ToolResult {
                call_id,
                content,
                is_error,
            } => {
                assert_eq!(call_id, "c0");
                assert_eq!(content, &marker(1000));
                assert!(*is_error);
            }
            otra => panic!("parte inesperada: {otra:?}"),
        }
    }

    #[test]
    fn original_history_is_not_modified() {
        let history: Vec<Message> = (0..10)
            .map(|i| result_msg(&format!("c{i}"), 1000))
            .collect();
        let snapshot = history.clone();

        let out = clear_old_tool_results(&history, KEEP_TOOL_RESULTS);

        assert_eq!(history, snapshot);
        assert_ne!(
            out, history,
            "la copia debe diferir: hay resultados que limpiar"
        );
    }
}
