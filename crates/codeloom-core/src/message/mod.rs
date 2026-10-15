pub mod codec;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    ToolCalls,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text {
        text: String,
    },
    Reasoning {
        text: String,
        signature: Option<String>,
    },
    ImageUrl {
        url: String,
    },
    ToolCall {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        call_id: String,
        content: String,
        is_error: bool,
    },
    Finish {
        reason: FinishReason,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub id: u64,
    pub role: Role,
    pub parts: Vec<ContentPart>,
    #[serde(default)]
    pub usage: Option<Usage>,
    pub created_at_ms: i64,
}

impl Message {
    pub fn new(role: Role, parts: Vec<ContentPart>) -> Self {
        Self {
            id: 0,
            role,
            parts,
            usage: None,
            created_at_ms: now_ms(),
        }
    }
    pub fn user(text: impl Into<String>) -> Self {
        Self::new(Role::User, vec![ContentPart::Text { text: text.into() }])
    }
    pub fn assistant(parts: Vec<ContentPart>) -> Self {
        Self::new(Role::Assistant, parts)
    }
    pub fn text(&self) -> String {
        self.parts
            .iter()
            .filter_map(|p| match p {
                ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }
    pub fn tool_calls(&self) -> Vec<(&str, &str, &serde_json::Value)> {
        self.parts
            .iter()
            .filter_map(|p| match p {
                ContentPart::ToolCall { id, name, input } => {
                    Some((id.as_str(), name.as_str(), input))
                }
                _ => None,
            })
            .collect()
    }
}

pub(crate) fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn text_collects_only_text_parts() {
        let m = Message::assistant(vec![
            ContentPart::Reasoning {
                text: "hmm".into(),
                signature: None,
            },
            ContentPart::Text {
                text: "hello ".into(),
            },
            ContentPart::Text {
                text: "world".into(),
            },
        ]);
        assert_eq!(m.text(), "hello world");
    }

    #[test]
    fn tool_calls_exposed() {
        let m = Message::assistant(vec![ContentPart::ToolCall {
            id: "c1".into(),
            name: "view".into(),
            input: json!({"path":"a.rs"}),
        }]);
        let calls = m.tool_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "c1");
        assert_eq!(calls[0].1, "view");
    }
}
