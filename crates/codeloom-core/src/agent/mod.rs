pub mod changes;
pub mod dispatch;
pub mod stop;

use crate::config::CodeLoomConfig;
use crate::error::Result;
use crate::event::{EventBus, LiveEvent};
use crate::message::{ContentPart, FinishReason, Message, Role, Usage};
use crate::permission::PermissionService;
use crate::provider::{Provider, ProviderRequest, StreamChunk};
use crate::session::{SessionId, SessionStore};
use crate::tools::{ToolCtx, ToolRegistry};
use futures::StreamExt;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tokio_util::sync::CancellationToken;

pub struct TurnOutcome {
    pub session: SessionId,
    pub final_text: String,
    pub steps: usize,
}

pub struct AgentLoop {
    provider: Arc<RwLock<Arc<dyn Provider>>>,
    store: Arc<dyn SessionStore>,
    tools: Arc<ToolRegistry>,
    bus: EventBus,
    permission: Arc<PermissionService>,
    config: Arc<RwLock<CodeLoomConfig>>,
    cwd: PathBuf,
    changes: crate::agent::changes::ChangeTracker,
    jobs: Arc<dyn crate::job::JobRegistry>,
    subagents: Option<Arc<dyn crate::subagent::SubagentLauncher>>,
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
        let provider = Arc::new(RwLock::new(provider));
        let config = Arc::new(RwLock::new(config));
        let jobs: Arc<dyn crate::job::JobRegistry> =
            Arc::new(crate::job::LocalJobRegistry::default());
        let subagents = tools.get("subagent").map(|_| {
            Arc::new(crate::subagent::InProcessSubagentLauncher::new(
                provider.clone(),
                store.clone(),
                tools.clone(),
                permission.clone(),
                config.clone(),
                cwd.clone(),
                jobs.clone(),
            )) as Arc<dyn crate::subagent::SubagentLauncher>
        });
        Self {
            provider,
            store,
            tools,
            bus,
            permission,
            config,
            changes: crate::agent::changes::ChangeTracker::detect(cwd.clone()),
            jobs,
            subagents,
            cwd,
        }
    }

    pub fn with_jobs(mut self, jobs: Arc<dyn crate::job::JobRegistry>) -> Self {
        self.jobs = jobs;
        self
    }

    pub fn jobs(&self) -> Arc<dyn crate::job::JobRegistry> {
        self.jobs.clone()
    }

    pub fn update_provider(&self, provider: Arc<dyn Provider>, config: CodeLoomConfig) {
        *self.provider.write().unwrap() = provider;
        *self.config.write().unwrap() = config;
    }

    pub async fn run(
        &self,
        session: SessionId,
        user_prompt: String,
        cancel: CancellationToken,
    ) -> Result<TurnOutcome> {
        self.bus.publish(LiveEvent::TurnStarted);
        let result = self.run_turn(session, user_prompt, cancel).await;
        match &result {
            Ok(_) => self.bus.publish(LiveEvent::TurnEnded),
            Err(error) => self.bus.publish(LiveEvent::Error {
                message: error.to_string(),
            }),
        }
        result
    }

    async fn run_turn(
        &self,
        session: SessionId,
        user_prompt: String,
        cancel: CancellationToken,
    ) -> Result<TurnOutcome> {
        let provider = self.provider.read().unwrap().clone();
        let config = self.config.read().unwrap().clone();
        self.flush_subagent_notices(session).await?;
        let mut history = self.store.messages(session).await?;
        let user = Message::user(user_prompt);
        self.store.append_message(session, &user).await?;
        self.bus.publish(LiveEvent::MessageAdded(user.clone()));
        history.push(user);

        let system = crate::config::context::load_context_files(&self.cwd).join("\n\n");
        let ctx = ToolCtx {
            cwd: self.cwd.clone(),
            permission: self.permission.clone(),
            session,
            jobs: self.jobs.clone(),
            bus: self.bus.clone(),
            subagents: self.subagents.clone(),
        };
        let mut steps = 0usize;
        let mut final_text = String::new();

        loop {
            if cancel.is_cancelled() {
                break;
            }
            self.flush_subagent_notices(session).await?;
            history = self.store.messages(session).await?;
            if crate::agent::stop::should_stop(&history, steps, config.max_steps) {
                break;
            }
            steps += 1;
            self.bus.publish(LiveEvent::StepStarted {
                step: steps,
                max_steps: config.max_steps,
            });
            let req = ProviderRequest {
                model: config.model.clone(),
                system: system.clone(),
                messages: history.clone(),
                tools: self.tools.specs(),
                temperature: 0.0,
                max_tokens: 4096,
            };
            let mut stream = tokio::select! {
                _ = cancel.cancelled() => break,
                result = provider.stream(req, cancel.clone()) => result?,
            };
            let mut text_buf = String::new();
            let mut tool_calls: Vec<(String, String, serde_json::Value)> = Vec::new();
            let mut usage: Option<Usage> = None;
            let mut finish = FinishReason::Stop;

            while let Some(chunk) = tokio::select! {
                _ = cancel.cancelled() => None,
                chunk = stream.next() => chunk,
            } {
                match chunk {
                    StreamChunk::TextDelta(t) => {
                        text_buf.push_str(&t);
                        self.bus.publish(LiveEvent::Delta { text: t });
                    }
                    StreamChunk::ReasoningDelta(r) => {
                        self.bus.publish(LiveEvent::ReasoningDelta { text: r });
                    }
                    StreamChunk::Usage(chunk) => {
                        let total = usage.get_or_insert(Usage {
                            input_tokens: 0,
                            output_tokens: 0,
                        });
                        total.input_tokens = total.input_tokens.saturating_add(chunk.input_tokens);
                        total.output_tokens =
                            total.output_tokens.saturating_add(chunk.output_tokens);
                        self.bus.publish(LiveEvent::Usage {
                            input_tokens: chunk.input_tokens,
                            output_tokens: chunk.output_tokens,
                        });
                    }
                    StreamChunk::ToolCallComplete { id, name, input } => {
                        tool_calls.push((id, name, input));
                    }
                    StreamChunk::Done(r) => finish = r,
                    _ => {}
                }
            }

            if cancel.is_cancelled() {
                finish = FinishReason::Error;
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
            let mut assistant = Message::assistant(parts);
            assistant.usage = usage;
            self.store.append_message(session, &assistant).await?;
            self.bus.publish(LiveEvent::MessageAdded(assistant.clone()));
            history.push(assistant);
            final_text = text_buf;

            if cancel.is_cancelled() {
                break;
            }

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
                    crate::permission::Decision::Allow
                    | crate::permission::Decision::AllowSession => allowed.push((id, name, input)),
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
            let mut by_id: std::collections::HashMap<String, (String, serde_json::Value)> =
                std::collections::HashMap::new();
            for (id, name, input) in &allowed {
                by_id.insert(id.clone(), (name.clone(), input.clone()));
            }

            let results = dispatch::execute_tools(
                &self.tools,
                ctx.clone(),
                allowed,
                cancel.clone(),
                self.bus.clone(),
            )
            .await;
            for (id, out) in results {
                if !out.is_error {
                    if let Some((name, input)) = by_id.get(&id) {
                        if name == "edit" {
                            let path = input["path"].as_str().unwrap_or_default();
                            let old = input["old"].as_str().unwrap_or_default();
                            let new = input["new"].as_str().unwrap_or_default();
                            let (added, removed) = self.changes.numbers(path, old, new);
                            self.bus.publish(LiveEvent::FileDiff {
                                path: path.to_string(),
                                added,
                                removed,
                            });
                        }
                    }
                }
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

        Ok(TurnOutcome {
            session,
            final_text,
            steps,
        })
    }

    async fn flush_subagent_notices(&self, session: SessionId) -> Result<()> {
        for job in self.jobs.pending_settled_notices(session) {
            let status = match job.status {
                crate::job::JobStatus::Completed => "completed",
                crate::job::JobStatus::Killed => "killed",
                crate::job::JobStatus::Failed => "failed",
                _ => continue,
            };
            let detail = job
                .detail
                .or_else(|| job.result.map(|result| result.content));
            let message = Message::user(format!(
                "[Job {} {status}] {}{}",
                job.id.0,
                job.label,
                detail.map(|text| format!("\n{text}")).unwrap_or_default()
            ));
            self.store.append_message(session, &message).await?;
            self.jobs.mark_notice_delivered(session, &job.id)?;
            self.bus
                .publish(LiveEvent::SessionMessageAdded { session, message });
        }
        if let Some(subagents) = &self.subagents {
            for message in subagents.deliver_notices(session).await? {
                self.bus
                    .publish(LiveEvent::SessionMessageAdded { session, message });
            }
        }
        Ok(())
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

    #[tokio::test]
    async fn cancelled_turn_marks_partial_message_as_error() {
        struct CancelMidStream;

        #[async_trait]
        impl Provider for CancelMidStream {
            fn capabilities(&self) -> Capabilities {
                Capabilities {
                    tool_use: false,
                    reasoning: false,
                    streaming: true,
                }
            }

            async fn stream(
                &self,
                _req: ProviderRequest,
                cancel: CancellationToken,
            ) -> Result<ProviderStream> {
                Ok(Box::pin(futures::stream::unfold(
                    (false, cancel),
                    |(sent_delta, cancel)| async move {
                        if sent_delta {
                            tokio::task::yield_now().await;
                            cancel.cancel();
                            None
                        } else {
                            Some((StreamChunk::TextDelta("partial".into()), (true, cancel)))
                        }
                    },
                )))
            }
        }

        let provider = Arc::new(CancelMidStream);
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("c.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let sid = store.create_session("/p").await.unwrap();
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
            Arc::new(ToolRegistry::new()),
            EventBus::new(64),
            Arc::new(PermissionService::allow_all()),
            cfg,
            dir.path().to_path_buf(),
        );
        let _ = agent
            .run(sid, "hi".into(), CancellationToken::new())
            .await
            .unwrap();
        let msgs = store.messages(sid).await.unwrap();
        assert!(msgs.iter().any(|m| m.parts.iter().any(|p| matches!(
            p,
            ContentPart::Finish {
                reason: FinishReason::Error
            }
        ))));
    }

    #[tokio::test]
    async fn edit_publishes_a_file_diff() {
        let scripts = vec![
            vec![
                StreamChunk::ToolCallComplete {
                    id: "c1".into(),
                    name: "edit".into(),
                    input: serde_json::json!({"path":"a.txt","old":"world","new":"rust"}),
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
        std::fs::write(dir.path().join("a.txt"), "hello world").unwrap();

        let store = Arc::new(
            SqliteStore::open(dir.path().join("l.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let sid = store.create_session("/p").await.unwrap();

        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(crate::tools::edit::EditTool));

        let cfg = CodeLoomConfig {
            provider: crate::config::ProviderConfig {
                kind: crate::config::ProviderKind::OpenAiCompat,
                base_url: "http://x".into(),
                api_key_env: "K".into(),
            },
            model: "m".into(),
            max_steps: 5,
        };
        let bus = EventBus::new(64);
        let mut rx = bus.subscribe();
        let agent = AgentLoop::new(
            provider,
            store.clone(),
            Arc::new(tools),
            bus,
            Arc::new(PermissionService::allow_all()),
            cfg,
            dir.path().to_path_buf(),
        );

        agent
            .run(sid, "hi".into(), CancellationToken::new())
            .await
            .unwrap();

        let mut found = None;
        while let Ok(ev) = rx.try_recv() {
            if let LiveEvent::FileDiff {
                path,
                added,
                removed,
            } = ev
            {
                found = Some((path, added, removed));
            }
        }
        assert_eq!(found, Some(("a.txt".to_string(), 1, 1)));
    }
}
