pub mod dispatch;
pub mod stop;

use crate::config::CodeLoomConfig;
use crate::error::Result;
use crate::event::{EventBus, LiveEvent};
use crate::message::{ContentPart, FinishReason, Message, Role};
use crate::permission::PermissionService;
use crate::provider::{Provider, ProviderRequest, StreamChunk};
use crate::session::{SessionId, SessionStore};
use crate::tools::{ToolCtx, ToolRegistry};
use futures::StreamExt;
use std::path::PathBuf;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub struct TurnOutcome {
    pub session: SessionId,
    pub final_text: String,
    pub steps: usize,
}

pub struct AgentLoop {
    provider: Arc<dyn Provider>,
    store: Arc<dyn SessionStore>,
    tools: Arc<ToolRegistry>,
    bus: EventBus,
    permission: Arc<PermissionService>,
    config: CodeLoomConfig,
    cwd: PathBuf,
}

impl AgentLoop {
    pub fn new(
        provider: Arc<dyn Provider>,
        store: Arc<dyn SessionStore>,
        tools: Arc<ToolRegistry>,
        bus: EventBus,
        permission: Arc<PermissionService>,
        config: CodeLoomConfig,
        cwd: PathBuf,
    ) -> Self {
        Self {
            provider,
            store,
            tools,
            bus,
            permission,
            config,
            cwd,
        }
    }

    pub async fn run(
        &self,
        session: SessionId,
        user_prompt: String,
        cancel: CancellationToken,
    ) -> Result<TurnOutcome> {
        self.bus.publish(LiveEvent::TurnStarted);
        let mut history = self.store.messages(session).await?;
        let user = Message::user(user_prompt);
        self.store.append_message(session, &user).await?;
        self.bus.publish(LiveEvent::MessageAdded(user.clone()));
        history.push(user);

        let system = crate::config::context::load_context_files(&self.cwd).join("\n\n");
        let ctx = ToolCtx {
            cwd: self.cwd.clone(),
            permission: self.permission.clone(),
        };
        let mut steps = 0usize;
        let mut final_text = String::new();

        loop {
            steps += 1;
            if crate::agent::stop::should_stop(&history, steps, self.config.max_steps) {
                break;
            }
            let req = ProviderRequest {
                model: self.config.model.clone(),
                system: system.clone(),
                messages: history.clone(),
                tools: self.tools.specs(),
                temperature: 0.0,
                max_tokens: 4096,
            };
            let mut stream = self.provider.stream(req, cancel.clone()).await?;
            let mut text_buf = String::new();
            let mut tool_calls: Vec<(String, String, serde_json::Value)> = Vec::new();
            let mut finish = FinishReason::Stop;

            while let Some(chunk) = stream.next().await {
                match chunk {
                    StreamChunk::TextDelta(t) => {
                        text_buf.push_str(&t);
                        self.bus.publish(LiveEvent::Delta { text: t });
                    }
                    StreamChunk::ReasoningDelta(r) => {
                        self.bus.publish(LiveEvent::ReasoningDelta { text: r });
                    }
                    StreamChunk::ToolCallComplete { id, name, input } => {
                        tool_calls.push((id, name, input));
                    }
                    StreamChunk::Done(r) => finish = r,
                    _ => {}
                }
            }

            let mut parts = Vec::new();
            if !text_buf.is_empty() {
                parts.push(ContentPart::Text {
                    text: text_buf.clone(),
                });
            }
            for (id, name, input) in &tool_calls {
                parts.push(ContentPart::ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                });
            }
            parts.push(ContentPart::Finish { reason: finish });
            let assistant = Message::assistant(parts);
            self.store.append_message(session, &assistant).await?;
            self.bus.publish(LiveEvent::MessageAdded(assistant.clone()));
            history.push(assistant);
            final_text = text_buf;

            if tool_calls.is_empty() {
                break;
            }

            let mut allowed = Vec::new();
            for (id, name, input) in tool_calls {
                let req = crate::permission::PermissionRequest {
                    tool: name.clone(),
                    action: "run".into(),
                    detail: input.to_string(),
                };
                match self.permission.check(req).await {
                    crate::permission::Decision::Allow => allowed.push((id, name, input)),
                    crate::permission::Decision::Deny => {
                        let tr = Message::new(
                            Role::Tool,
                            vec![ContentPart::ToolResult {
                                call_id: id,
                                content: "permission denied".into(),
                                is_error: true,
                            }],
                        );
                        self.store.append_message(session, &tr).await?;
                        self.bus.publish(LiveEvent::MessageAdded(tr.clone()));
                        history.push(tr);
                    }
                }
            }
            let results =
                dispatch::execute_tools(&self.tools, ctx.clone(), allowed, cancel.clone()).await;
            for (id, out) in results {
                let tr = Message::new(
                    Role::Tool,
                    vec![ContentPart::ToolResult {
                        call_id: id,
                        content: out.content,
                        is_error: out.is_error,
                    }],
                );
                self.store.append_message(session, &tr).await?;
                self.bus.publish(LiveEvent::MessageAdded(tr.clone()));
                history.push(tr);
            }
        }

        self.bus.publish(LiveEvent::TurnEnded);
        Ok(TurnOutcome {
            session,
            final_text,
            steps,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{Capabilities, ProviderStream, StreamChunk};
    use crate::session::sqlite::SqliteStore;
    use crate::tools::{ExecutionMode, Tool, ToolCtx, ToolOutput};
    use async_trait::async_trait;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    struct QueueProvider {
        scripts: Mutex<VecDeque<Vec<StreamChunk>>>,
    }

    #[async_trait]
    impl Provider for QueueProvider {
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                tool_use: true,
                reasoning: false,
                streaming: true,
            }
        }
        async fn stream(
            &self,
            _req: ProviderRequest,
            _cancel: CancellationToken,
        ) -> Result<ProviderStream> {
            let script = self.scripts.lock().unwrap().pop_front().unwrap_or_default();
            Ok(Box::pin(futures::stream::iter(script)))
        }
    }

    struct Noop;

    #[async_trait]
    impl Tool for Noop {
        fn name(&self) -> &str {
            "noop"
        }
        fn description(&self) -> &str {
            "n"
        }
        fn schema(&self) -> serde_json::Value {
            serde_json::json!({"type":"object"})
        }
        fn execution_mode(&self, _i: &serde_json::Value) -> ExecutionMode {
            ExecutionMode::Exclusive
        }
        async fn run(
            &self,
            _c: ToolCtx,
            _i: serde_json::Value,
            _x: CancellationToken,
        ) -> Result<ToolOutput> {
            Ok(ToolOutput::ok("ran"))
        }
    }

    #[tokio::test]
    async fn runs_tool_then_finishes() {
        let scripts = vec![
            vec![
                StreamChunk::ToolCallComplete {
                    id: "c1".into(),
                    name: "noop".into(),
                    input: serde_json::json!({}),
                },
                StreamChunk::Done(FinishReason::ToolCalls),
            ],
            vec![
                StreamChunk::TextDelta("done".into()),
                StreamChunk::Done(FinishReason::Stop),
            ],
        ];
        let provider = Arc::new(QueueProvider {
            scripts: Mutex::new(scripts.into()),
        });
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("l.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let sid = store.create_session("/p").await.unwrap();

        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(Noop));

        let cfg = CodeLoomConfig {
            provider: crate::config::ProviderConfig {
                kind: crate::config::ProviderKind::OpenAiCompat,
                base_url: "http://x".into(),
                api_key_env: "K".into(),
            },
            model: "m".into(),
            max_steps: 5,
        };
        let agent = AgentLoop::new(
            provider,
            store.clone(),
            Arc::new(tools),
            EventBus::new(64),
            Arc::new(PermissionService::allow_all()),
            cfg,
            dir.path().to_path_buf(),
        );

        let outcome = agent
            .run(sid, "hi".into(), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(outcome.steps, 2);
        assert_eq!(outcome.final_text, "done");
        let msgs = store.messages(sid).await.unwrap();
        assert!(msgs.iter().any(|m| m.role == Role::Tool));
    }

    #[tokio::test]
    async fn denied_tool_stops_and_is_recorded() {
        let scripts = vec![
            vec![
                StreamChunk::ToolCallComplete {
                    id: "c1".into(),
                    name: "noop".into(),
                    input: serde_json::json!({}),
                },
                StreamChunk::Done(FinishReason::ToolCalls),
            ],
            vec![
                StreamChunk::TextDelta("after".into()),
                StreamChunk::Done(FinishReason::Stop),
            ],
        ];
        let provider = Arc::new(QueueProvider {
            scripts: Mutex::new(scripts.into()),
        });
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("d.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let sid = store.create_session("/p").await.unwrap();
        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(Noop));
        let cfg = CodeLoomConfig {
            provider: crate::config::ProviderConfig {
                kind: crate::config::ProviderKind::OpenAiCompat,
                base_url: "http://x".into(),
                api_key_env: "K".into(),
            },
            model: "m".into(),
            max_steps: 5,
        };
        let agent = AgentLoop::new(
            provider,
            store.clone(),
            Arc::new(tools),
            EventBus::new(64),
            Arc::new(crate::permission::PermissionService::with_responder(
                Box::new(crate::permission::DenyAll),
            )),
            cfg,
            dir.path().to_path_buf(),
        );
        let _ = agent
            .run(sid, "hi".into(), CancellationToken::new())
            .await
            .unwrap();
        let msgs = store.messages(sid).await.unwrap();
        assert!(msgs.iter().any(|m| m
            .parts
            .iter()
            .any(|p| matches!(p, ContentPart::ToolResult { is_error: true, .. }))));
    }
}
