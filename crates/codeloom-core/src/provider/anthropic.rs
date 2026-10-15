use super::sse::SseBuffer;
use super::{Capabilities, Provider, ProviderRequest, ProviderStream, StreamChunk};
use crate::error::{CoreError, Result};
use crate::message::{FinishReason, Usage};
use async_trait::async_trait;
use futures::StreamExt;
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub struct AnthropicAcc {
    blocks: Vec<(String, String, String)>, // (id, name, json) por index
}

impl AnthropicAcc {
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }
    pub fn finish(&mut self) -> Vec<StreamChunk> {
        std::mem::take(&mut self.blocks)
            .into_iter()
            .filter(|(_, name, _)| !name.is_empty())
            .map(|(id, name, json)| StreamChunk::ToolCallComplete {
                id,
                name,
                input: serde_json::from_str(&json).unwrap_or(serde_json::Value::Null),
            })
            .collect()
    }
}

pub fn map_anthropic_event(event: &str, data: &str, acc: &mut AnthropicAcc) -> Vec<StreamChunk> {
    let mut out = Vec::new();
    let v: serde_json::Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(_) => return out,
    };
    match event {
        "content_block_start" => {
            let idx = v["index"].as_u64().unwrap_or(0) as usize;
            while acc.blocks.len() <= idx {
                acc.blocks
                    .push((String::new(), String::new(), String::new()));
            }
            let cb = &v["content_block"];
            if cb["type"] == "tool_use" {
                acc.blocks[idx].0 = cb["id"].as_str().unwrap_or_default().to_string();
                acc.blocks[idx].1 = cb["name"].as_str().unwrap_or_default().to_string();
            }
        }
        "content_block_delta" => {
            let idx = v["index"].as_u64().unwrap_or(0) as usize;
            let d = &v["delta"];
            match d["type"].as_str().unwrap_or("") {
                "text_delta" => out.push(StreamChunk::TextDelta(
                    d["text"].as_str().unwrap_or_default().to_string(),
                )),
                "thinking_delta" => out.push(StreamChunk::ReasoningDelta(
                    d["thinking"].as_str().unwrap_or_default().to_string(),
                )),
                "input_json_delta" => {
                    while acc.blocks.len() <= idx {
                        acc.blocks
                            .push((String::new(), String::new(), String::new()));
                    }
                    acc.blocks[idx]
                        .2
                        .push_str(d["partial_json"].as_str().unwrap_or_default());
                }
                _ => {}
            }
        }
        "message_start" => {
            if let Some(u) = v.pointer("/message/usage") {
                out.push(StreamChunk::Usage(Usage {
                    input_tokens: u["input_tokens"].as_u64().unwrap_or(0) as u32,
                    output_tokens: u["output_tokens"].as_u64().unwrap_or(0) as u32,
                }));
            }
        }
        "message_delta" => {
            if let Some(u) = v.get("usage").filter(|u| !u.is_null()) {
                out.push(StreamChunk::Usage(Usage {
                    input_tokens: 0,
                    output_tokens: u["output_tokens"].as_u64().unwrap_or(0) as u32,
                }));
            }
            out.extend(acc.finish());
            let reason = match v
                .pointer("/delta/stop_reason")
                .and_then(|x| x.as_str())
                .unwrap_or("")
            {
                "tool_use" => FinishReason::ToolCalls,
                "max_tokens" => FinishReason::Length,
                _ => FinishReason::Stop,
            };
            out.push(StreamChunk::Done(reason));
        }
        _ => {}
    }
    out
}

pub struct AnthropicProvider {
    base_url: String,
    api_key: String,
    client: reqwest::Client,
}

impl AnthropicProvider {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
            client: reqwest::Client::new(),
        }
    }

    fn build_body(&self, req: &ProviderRequest) -> serde_json::Value {
        let messages = wire_messages(&req.messages);
        let tools: Vec<serde_json::Value> = req
            .tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema
                })
            })
            .collect();
        serde_json::json!({
            "model": req.model,
            "max_tokens": req.max_tokens,
            "system": req.system,
            "messages": messages,
            "tools": tools,
            "stream": true,
            "temperature": req.temperature
        })
    }
}

fn role_str(r: crate::message::Role) -> &'static str {
    use crate::message::Role::{Assistant, System, Tool, User};
    match r {
        System => "system",
        User => "user",
        Assistant => "assistant",
        Tool => "user",
    }
}

/// Traduce los mensajes al formato Messages: bloques `tool_use` en el
/// asistente y `tool_result` en un mensaje de usuario (uno por ToolResult).
fn wire_messages(messages: &[crate::message::Message]) -> Vec<serde_json::Value> {
    use crate::message::{ContentPart, Role};
    let mut out = Vec::new();
    for m in messages {
        match m.role {
            Role::Assistant => {
                let mut blocks: Vec<serde_json::Value> = Vec::new();
                let text = m.text();
                if !text.is_empty() {
                    blocks.push(serde_json::json!({ "type": "text", "text": text }));
                }
                for p in &m.parts {
                    if let ContentPart::ToolCall { id, name, input } = p {
                        blocks.push(serde_json::json!({
                            "type": "tool_use",
                            "id": id,
                            "name": name,
                            "input": input
                        }));
                    }
                }
                if blocks.is_empty() {
                    blocks.push(serde_json::json!({ "type": "text", "text": "" }));
                }
                out.push(serde_json::json!({ "role": "assistant", "content": blocks }));
            }
            Role::Tool => {
                let mut blocks: Vec<serde_json::Value> = Vec::new();
                for p in &m.parts {
                    if let ContentPart::ToolResult {
                        call_id, content, ..
                    } = p
                    {
                        blocks.push(serde_json::json!({
                            "type": "tool_result",
                            "tool_use_id": call_id,
                            "content": content
                        }));
                    }
                }
                if !blocks.is_empty() {
                    out.push(serde_json::json!({ "role": "user", "content": blocks }));
                }
            }
            other => {
                out.push(serde_json::json!({ "role": role_str(other), "content": m.text() }));
            }
        }
    }
    out
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            tool_use: true,
            reasoning: true,
            streaming: true,
        }
    }

    async fn stream(
        &self,
        req: ProviderRequest,
        cancel: CancellationToken,
    ) -> Result<ProviderStream> {
        let url = format!("{}/v1/messages", self.base_url);
        let resp = self
            .client
            .post(url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&self.build_body(&req))
            .send()
            .await
            .map_err(|e| CoreError::Provider(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(CoreError::Provider(format!("http {status}: {text}")));
        }

        struct State<S> {
            stream: S,
            sse: SseBuffer,
            acc: AnthropicAcc,
            queue: std::collections::VecDeque<StreamChunk>,
            cancel: CancellationToken,
            done: bool,
        }

        let init = State {
            stream: resp.bytes_stream(),
            sse: SseBuffer::default(),
            acc: AnthropicAcc::default(),
            queue: std::collections::VecDeque::new(),
            cancel,
            done: false,
        };

        let s = futures::stream::unfold(init, |mut st| async move {
            loop {
                if st.cancel.is_cancelled() {
                    return None;
                }
                if let Some(chunk) = st.queue.pop_front() {
                    if matches!(chunk, StreamChunk::Done(_)) {
                        st.done = true;
                    }
                    return Some((chunk, st));
                }
                if st.done {
                    return None;
                }
                match st.stream.next().await {
                    Some(Ok(bytes)) => {
                        let text = String::from_utf8_lossy(&bytes);
                        for (name, data) in st.sse.push_events(&text) {
                            st.queue
                                .extend(map_anthropic_event(&name, &data, &mut st.acc));
                        }
                    }
                    _ => {
                        st.queue.extend(st.acc.finish());
                        if !st.queue.iter().any(|c| matches!(c, StreamChunk::Done(_))) {
                            st.queue.push_back(StreamChunk::Done(FinishReason::Stop));
                        }
                    }
                }
            }
        });

        Ok(Box::pin(s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ev(name: &str, v: serde_json::Value) -> Vec<StreamChunk> {
        let mut acc = AnthropicAcc::default();
        map_anthropic_event(name, &v.to_string(), &mut acc)
    }

    #[test]
    fn maps_text_delta() {
        let chunks = ev(
            "content_block_delta",
            json!({"index":0,"delta":{"type":"text_delta","text":"hi"}}),
        );
        assert!(matches!(chunks.as_slice(), [StreamChunk::TextDelta(t)] if t == "hi"));
    }

    #[test]
    fn assembles_tool_use() {
        let mut acc = AnthropicAcc::default();
        let start = json!({"index":0,"content_block":{"type":"tool_use","id":"t1","name":"view"}});
        let d1 = json!({"index":0,"delta":{"type":"input_json_delta","partial_json":"{\"path\":"}});
        let d2 = json!({"index":0,"delta":{"type":"input_json_delta","partial_json":"\"a.rs\"}"}});
        map_anthropic_event("content_block_start", &start.to_string(), &mut acc);
        map_anthropic_event("content_block_delta", &d1.to_string(), &mut acc);
        map_anthropic_event("content_block_delta", &d2.to_string(), &mut acc);
        let stop = json!({"delta":{"stop_reason":"tool_use"}});
        let chunks = map_anthropic_event("message_delta", &stop.to_string(), &mut acc);
        assert!(
            matches!(chunks.first(), Some(StreamChunk::ToolCallComplete { name, input, .. })
            if name == "view" && input["path"] == "a.rs")
        );
        assert!(matches!(
            chunks.last(),
            Some(StreamChunk::Done(FinishReason::ToolCalls))
        ));
    }

    #[test]
    fn maps_message_start_usage() {
        let chunks = ev(
            "message_start",
            json!({"message":{"usage":{"input_tokens":12,"output_tokens":0}}}),
        );
        assert!(matches!(chunks.as_slice(), [StreamChunk::Usage(u)] if u.input_tokens == 12));
    }
}

#[cfg(test)]
mod wire_tests {
    use super::*;
    use crate::message::{ContentPart, Message, Role};
    use serde_json::json;

    #[test]
    fn serializes_tool_use_and_tool_result_blocks() {
        let p = AnthropicProvider::new("http://x", "k");
        let req = ProviderRequest {
            model: "m".into(),
            system: "sys".into(),
            messages: vec![
                Message::assistant(vec![ContentPart::ToolCall {
                    id: "t1".into(),
                    name: "view".into(),
                    input: json!({"path":"a"}),
                }]),
                Message::new(
                    Role::Tool,
                    vec![ContentPart::ToolResult {
                        call_id: "t1".into(),
                        content: "ok".into(),
                        is_error: false,
                    }],
                ),
            ],
            tools: vec![],
            temperature: 0.0,
            max_tokens: 16,
        };
        let body = p.build_body(&req);
        let msgs = body["messages"].as_array().unwrap();
        let assistant = msgs.iter().find(|m| m["role"] == "assistant").unwrap();
        let blocks = assistant["content"].as_array().unwrap();
        assert!(blocks
            .iter()
            .any(|b| b["type"] == "tool_use" && b["id"] == "t1" && b["name"] == "view"));
        let user = msgs
            .iter()
            .find(|m| {
                m["role"] == "user"
                    && m["content"]
                        .as_array()
                        .map(|a| a.iter().any(|b| b["type"] == "tool_result"))
                        .unwrap_or(false)
            })
            .unwrap();
        let result = user["content"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["type"] == "tool_result")
            .unwrap();
        assert_eq!(result["tool_use_id"], "t1");
        assert_eq!(result["content"], "ok");
    }
}
