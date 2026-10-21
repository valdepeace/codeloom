use super::sse::SseBuffer;
use super::{Capabilities, Provider, ProviderRequest, ProviderStream, StreamChunk};
use crate::error::{CoreError, Result};
use crate::message::{FinishReason, Usage};
use async_trait::async_trait;
use futures::StreamExt;
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub struct ToolCallAcc {
    slots: Vec<(String, String, String)>,
}

impl ToolCallAcc {
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    pub fn apply(&mut self, tc: &serde_json::Value) {
        let idx = tc.get("index").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
        while self.slots.len() <= idx {
            self.slots
                .push((String::new(), String::new(), String::new()));
        }
        let slot = &mut self.slots[idx];
        if let Some(id) = tc.get("id").and_then(|x| x.as_str()) {
            if !id.is_empty() {
                slot.0 = id.to_string();
            }
        }
        if let Some(name) = tc.pointer("/function/name").and_then(|x| x.as_str()) {
            if !name.is_empty() {
                slot.1 = name.to_string();
            }
        }
        if let Some(args) = tc.pointer("/function/arguments").and_then(|x| x.as_str()) {
            slot.2.push_str(args);
        }
    }

    pub fn finish(&mut self) -> Vec<StreamChunk> {
        std::mem::take(&mut self.slots)
            .into_iter()
            .filter(|(id, name, _)| !id.is_empty() || !name.is_empty())
            .map(|(id, name, args)| {
                let input = serde_json::from_str(&args).unwrap_or(serde_json::Value::Null);
                StreamChunk::ToolCallComplete { id, name, input }
            })
            .collect()
    }
}

pub fn map_openai_event(data: &str, acc: &mut ToolCallAcc) -> Vec<StreamChunk> {
    let mut out = Vec::new();
    let v: serde_json::Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(_) => return out,
    };
    if let Some(u) = v.get("usage").filter(|u| !u.is_null()) {
        out.push(StreamChunk::Usage(Usage {
            input_tokens: u["prompt_tokens"].as_u64().unwrap_or(0) as u32,
            output_tokens: u["completion_tokens"].as_u64().unwrap_or(0) as u32,
        }));
    }
    let Some(choices) = v.get("choices").and_then(|c| c.as_array()) else {
        return out;
    };
    for c in choices {
        if let Some(delta) = c.get("delta") {
            if let Some(t) = delta.get("content").and_then(|x| x.as_str()) {
                if !t.is_empty() {
                    out.push(StreamChunk::TextDelta(t.to_string()));
                }
            }
            if let Some(r) = delta.get("reasoning_content").and_then(|x| x.as_str()) {
                if !r.is_empty() {
                    out.push(StreamChunk::ReasoningDelta(r.to_string()));
                }
            }
            if let Some(tcs) = delta.get("tool_calls").and_then(|x| x.as_array()) {
                for tc in tcs {
                    acc.apply(tc);
                }
            }
        }
        if let Some(fr) = c.get("finish_reason").and_then(|x| x.as_str()) {
            out.extend(acc.finish());
            let reason = match fr {
                "length" => FinishReason::Length,
                "tool_calls" => FinishReason::ToolCalls,
                _ => FinishReason::Stop,
            };
            out.push(StreamChunk::Done(reason));
        }
    }
    out
}

pub struct OpenAiCompatProvider {
    base_url: String,
    api_key: String,
    client: reqwest::Client,
}

impl OpenAiCompatProvider {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
            client: reqwest::Client::new(),
        }
    }

    fn build_body(&self, req: &ProviderRequest) -> serde_json::Value {
        let mut messages = vec![serde_json::json!({ "role": "system", "content": req.system })];
        messages.extend(wire_messages(&req.messages));
        let tools: Vec<serde_json::Value> = req
            .tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema
                    }
                })
            })
            .collect();
        let mut cuerpo = serde_json::json!({
            "model": req.model,
            "messages": messages,
            "tools": tools,
            "stream": true,
            // Sin esta petición expresa, los servidores de tipo OpenAI no envían el
            // uso de tokens en el flujo y las métricas quedarían siempre a cero.
            "stream_options": { "include_usage": true },
            "temperature": req.temperature,
            "max_tokens": req.max_tokens
        });
        // Sin esfuerzo, el cuerpo queda exactamente como antes de existir este campo.
        if let Some(nivel) = req.effort.as_deref() {
            // OpenRouter no entiende `reasoning_effort`: su forma nativa anida el nivel.
            if self.base_url.contains("openrouter.ai") {
                cuerpo["reasoning"] = serde_json::json!({ "effort": nivel });
            } else {
                cuerpo["reasoning_effort"] = serde_json::json!(nivel);
            }
        }
        cuerpo
    }
}

fn role_str(r: crate::message::Role) -> &'static str {
    use crate::message::Role::{Assistant, System, Tool, User};
    match r {
        System => "system",
        User => "user",
        Assistant => "assistant",
        Tool => "tool",
    }
}

/// Traduce los mensajes al formato Chat Completions, incluyendo `tool_calls`
/// en el asistente y `tool_call_id` en los resultados (uno por ToolResult).
fn wire_messages(messages: &[crate::message::Message]) -> Vec<serde_json::Value> {
    use crate::message::{ContentPart, Role};
    let mut out = Vec::new();
    for m in messages {
        match m.role {
            Role::Assistant => {
                let calls: Vec<serde_json::Value> = m
                    .parts
                    .iter()
                    .filter_map(|p| match p {
                        ContentPart::ToolCall { id, name, input } => Some(serde_json::json!({
                            "id": id,
                            "type": "function",
                            "function": { "name": name, "arguments": input.to_string() }
                        })),
                        _ => None,
                    })
                    .collect();
                let mut obj = serde_json::json!({ "role": "assistant", "content": m.text() });
                if !calls.is_empty() {
                    obj["tool_calls"] = serde_json::Value::Array(calls);
                }
                out.push(obj);
            }
            Role::Tool => {
                for p in &m.parts {
                    if let ContentPart::ToolResult {
                        call_id, content, ..
                    } = p
                    {
                        out.push(serde_json::json!({
                            "role": "tool",
                            "tool_call_id": call_id,
                            "content": content
                        }));
                    }
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
impl Provider for OpenAiCompatProvider {
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
        let url = format!("{}/chat/completions", self.base_url);
        let resp = self
            .client
            .post(url)
            .bearer_auth(&self.api_key)
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
            acc: ToolCallAcc,
            queue: std::collections::VecDeque<StreamChunk>,
            cancel: CancellationToken,
            done: bool,
        }

        let init = State {
            stream: resp.bytes_stream(),
            sse: SseBuffer::default(),
            acc: ToolCallAcc::default(),
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
                        for data in st.sse.push(&text) {
                            if data == "[DONE]" {
                                st.queue.extend(st.acc.finish());
                                st.queue.push_back(StreamChunk::Done(FinishReason::Stop));
                            } else {
                                st.queue.extend(map_openai_event(&data, &mut st.acc));
                            }
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

    #[test]
    fn maps_text_delta() {
        let mut acc = ToolCallAcc::default();
        let data = json!({"choices":[{"delta":{"content":"hi"}}]}).to_string();
        let chunks = map_openai_event(&data, &mut acc);
        assert!(matches!(chunks.as_slice(), [StreamChunk::TextDelta(t)] if t == "hi"));
    }

    #[test]
    fn assembles_tool_call_across_events() {
        let mut acc = ToolCallAcc::default();
        let args1 = r#"{"path":"#;
        let args2 = r#""a.rs"}"#;
        let e1 = json!({"choices":[{"delta":{"tool_calls":[
            {"index":0,"id":"c1","function":{"name":"view","arguments":args1}}]}}]})
        .to_string();
        let e2 = json!({"choices":[{"delta":{"tool_calls":[
            {"index":0,"function":{"arguments":args2}}]}}]})
        .to_string();
        let e3 = json!({"choices":[{"finish_reason":"tool_calls"}]}).to_string();
        assert!(map_openai_event(&e1, &mut acc).is_empty());
        assert!(map_openai_event(&e2, &mut acc).is_empty());
        let chunks = map_openai_event(&e3, &mut acc);
        assert!(
            matches!(chunks.first(), Some(StreamChunk::ToolCallComplete { name, input, .. })
            if name == "view" && input["path"] == "a.rs")
        );
        assert!(matches!(
            chunks.last(),
            Some(StreamChunk::Done(FinishReason::ToolCalls))
        ));
    }
}

#[cfg(test)]
mod http_tests {
    use super::*;
    use futures::StreamExt;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn streams_text_then_done() {
        let server = MockServer::start().await;
        let body = "data: {\"choices\":[{\"delta\":{\"content\":\"hello\"}}]}\n\ndata: [DONE]\n\n";
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(body, "text/event-stream"))
            .mount(&server)
            .await;

        let p = OpenAiCompatProvider::new(server.uri(), "sk-test");
        let req = ProviderRequest {
            model: "m".into(),
            system: String::new(),
            messages: vec![],
            tools: vec![],
            temperature: 0.0,
            max_tokens: 16,
            effort: None,
        };
        let mut s = p.stream(req, CancellationToken::new()).await.unwrap();
        let mut texts = String::new();
        let mut saw_done = false;
        while let Some(c) = s.next().await {
            match c {
                StreamChunk::TextDelta(t) => texts.push_str(&t),
                StreamChunk::Done(_) => saw_done = true,
                _ => {}
            }
        }
        assert_eq!(texts, "hello");
        assert!(saw_done);
    }
}

#[cfg(test)]
mod wire_tests {
    use super::*;
    use crate::message::{ContentPart, Message, Role};

    #[test]
    fn serializes_tool_calls_and_tool_results() {
        let p = OpenAiCompatProvider::new("http://x", "k");
        let req = ProviderRequest {
            model: "m".into(),
            system: "sys".into(),
            messages: vec![
                Message::assistant(vec![ContentPart::ToolCall {
                    id: "c1".into(),
                    name: "view".into(),
                    input: serde_json::json!({"path":"a"}),
                }]),
                Message::new(
                    Role::Tool,
                    vec![ContentPart::ToolResult {
                        call_id: "c1".into(),
                        content: "ok".into(),
                        is_error: false,
                    }],
                ),
            ],
            tools: vec![],
            temperature: 0.0,
            max_tokens: 16,
            effort: None,
        };
        let body = p.build_body(&req);
        let msgs = body["messages"].as_array().unwrap();
        let assistant = msgs.iter().find(|m| m["role"] == "assistant").unwrap();
        assert_eq!(assistant["tool_calls"][0]["id"], "c1");
        assert_eq!(assistant["tool_calls"][0]["function"]["name"], "view");
        assert!(assistant["tool_calls"][0]["function"]["arguments"]
            .as_str()
            .unwrap()
            .contains("path"));
        let tool = msgs.iter().find(|m| m["role"] == "tool").unwrap();
        assert_eq!(tool["tool_call_id"], "c1");
        assert_eq!(tool["content"], "ok");
    }

    #[test]
    fn body_requests_usage_in_the_stream() {
        let p = OpenAiCompatProvider::new("http://x", "k");
        let req = ProviderRequest {
            model: "m".into(),
            system: String::new(),
            messages: vec![],
            tools: vec![],
            temperature: 0.0,
            max_tokens: 16,
            effort: None,
        };
        let body = p.build_body(&req);
        assert_eq!(body["stream_options"]["include_usage"], true);
    }
}

#[cfg(test)]
mod effort_tests {
    use super::*;
    use crate::provider::ToolSpec;
    use serde_json::json;

    /// Petición de prueba con una herramienta, para que el cuerpo sea representativo.
    fn peticion(effort: Option<&str>) -> ProviderRequest {
        ProviderRequest {
            model: "m".into(),
            system: "sys".into(),
            messages: vec![],
            tools: vec![ToolSpec {
                name: "view".into(),
                description: "Ve un fichero".into(),
                input_schema: json!({ "type": "object" }),
            }],
            temperature: 0.5,
            max_tokens: 16,
            effort: effort.map(String::from),
        }
    }

    #[test]
    fn openai_body_without_effort_is_unchanged() {
        // Cuerpo exacto que enviaba `build_body` antes de existir el esfuerzo.
        let previo = json!({
            "model": "m",
            "messages": [{ "role": "system", "content": "sys" }],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "view",
                    "description": "Ve un fichero",
                    "parameters": { "type": "object" }
                }
            }],
            "stream": true,
            "stream_options": { "include_usage": true },
            "temperature": 0.5,
            "max_tokens": 16
        });
        let p = OpenAiCompatProvider::new("http://x", "k");
        assert_eq!(p.build_body(&peticion(None)), previo);
    }

    #[test]
    fn openai_body_sends_reasoning_effort() {
        let p = OpenAiCompatProvider::new("http://x", "k");
        let con = p.build_body(&peticion(Some("high")));
        assert_eq!(con["reasoning_effort"], "high");
        assert!(con.get("reasoning").is_none());
        // Salvo el campo nuevo, el cuerpo es idéntico al de «default».
        let mut sin_campo = con.clone();
        sin_campo
            .as_object_mut()
            .unwrap()
            .remove("reasoning_effort");
        assert_eq!(sin_campo, p.build_body(&peticion(None)));
    }

    #[test]
    fn openrouter_body_nests_effort_under_reasoning() {
        let p = OpenAiCompatProvider::new("https://openrouter.ai/api/v1", "k");
        let con = p.build_body(&peticion(Some("high")));
        assert_eq!(con["reasoning"]["effort"], "high");
        assert!(con.get("reasoning_effort").is_none());
    }
}
