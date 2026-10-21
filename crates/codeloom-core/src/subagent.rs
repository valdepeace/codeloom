use crate::agent::AgentLoop;
use crate::config::CodeLoomConfig;
use crate::error::{CoreError, Result};
use crate::event::EventBus;
use crate::job::{JobId, JobRegistry, JobResult, JobSpec};
use crate::permission::PermissionService;
use crate::provider::Provider;
use crate::session::{SessionId, SessionStore, SubagentDescriptor, SubagentStatus};
use crate::tools::ToolRegistry;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

#[async_trait]
pub trait SubagentLauncher: Send + Sync {
    async fn start(
        &self,
        parent: SessionId,
        label: String,
        prompt: String,
        cancel: CancellationToken,
    ) -> Result<(SessionId, JobId)>;

    async fn control(
        &self,
        parent: SessionId,
        child: SessionId,
        action: &str,
        message: Option<String>,
    ) -> Result<()>;

    async fn list(&self, parent: SessionId) -> Result<Vec<SubagentDescriptor>>;

    async fn deliver_notices(&self, parent: SessionId) -> Result<Vec<crate::message::Message>>;
}

pub struct InProcessSubagentLauncher {
    provider: Arc<RwLock<Arc<dyn Provider>>>,
    store: Arc<dyn SessionStore>,
    tools: Arc<ToolRegistry>,
    permission: Arc<PermissionService>,
    config: Arc<RwLock<CodeLoomConfig>>,
    cwd: PathBuf,
    jobs: Arc<dyn JobRegistry>,
    active: Arc<Semaphore>,
}

impl InProcessSubagentLauncher {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        provider: Arc<RwLock<Arc<dyn Provider>>>,
        store: Arc<dyn SessionStore>,
        tools: Arc<ToolRegistry>,
        permission: Arc<PermissionService>,
        config: Arc<RwLock<CodeLoomConfig>>,
        cwd: PathBuf,
        jobs: Arc<dyn JobRegistry>,
    ) -> Self {
        Self {
            provider,
            store,
            tools: Arc::new(
                tools
                    .without("subagent")
                    .without("subagent_control")
                    .without("subagent_list")
                    .without("ask_user"),
            ),
            permission,
            config,
            cwd,
            jobs,
            active: Arc::new(Semaphore::new(8)),
        }
    }
}

#[async_trait]
impl SubagentLauncher for InProcessSubagentLauncher {
    async fn start(
        &self,
        parent: SessionId,
        label: String,
        prompt: String,
        parent_cancel: CancellationToken,
    ) -> Result<(SessionId, JobId)> {
        if prompt.trim().is_empty() || prompt.len() > 32_768 || label.len() > 128 {
            return Err(CoreError::Tool("invalid subagent prompt or label".into()));
        }
        let permit = self
            .active
            .clone()
            .try_acquire_owned()
            .map_err(|_| CoreError::Tool("maximum of 8 active subagents reached".into()))?;
        let project = self.cwd.to_string_lossy().into_owned();
        let child = self.store.create_session(&project).await?;
        let child_store = self.store.clone();
        let child_tools = self.tools.clone();
        let child_provider = self.provider.read().unwrap().clone();
        let child_permission = self.permission.clone();
        let child_config = self.config.read().unwrap().clone();
        let child_cwd = self.cwd.clone();
        let child_jobs = self.jobs.clone();
        let descriptor_label = label.clone();
        let job = self.jobs.start(JobSpec {
            kind: "subagent".into(),
            label,
            owner: parent,
            output_limit: 64 * 1024,
            run: Box::new(move |handle, job_cancel| {
                Box::pin(async move {
                    let _permit = permit;
                    let child_agent = AgentLoop::new(
                        child_provider,
                        child_store.clone(),
                        child_tools,
                        EventBus::new(64),
                        child_permission,
                        child_config,
                        child_cwd,
                    )
                    .with_jobs(child_jobs);
                    let child_cancel = CancellationToken::new();
                    let outcome = tokio::select! {
                        result = child_agent.run(child, prompt, child_cancel.clone()) => result?,
                        _ = job_cancel.cancelled() => {
                            child_cancel.cancel();
                            return Ok(None);
                        }
                        _ = parent_cancel.cancelled() => {
                            child_cancel.cancel();
                            return Ok(None);
                        }
                    };
                    let text = outcome.final_text;
                    child_store.complete_subagent(parent, child, &text).await?;
                    handle.append(&text);
                    Ok(Some(JobResult { content: text }))
                })
            }),
        })?;
        let descriptor = SubagentDescriptor {
            parent,
            child,
            job_id: job.id().0.clone(),
            label: descriptor_label,
            status: SubagentStatus::Running,
            result: None,
            notice_delivered: false,
        };
        if let Err(error) = self.store.save_subagent(&descriptor).await {
            let _ = self.jobs.kill(parent, job.id());
            return Err(error);
        }
        let mut updates = self.jobs.subscribe(parent, job.id())?;
        let store = self.store.clone();
        tokio::spawn(async move {
            loop {
                let snapshot = updates.borrow_and_update().clone();
                let status = match snapshot.status {
                    crate::job::JobStatus::Completed => Some(SubagentStatus::Completed),
                    crate::job::JobStatus::Killed => Some(SubagentStatus::Killed),
                    crate::job::JobStatus::Failed => Some(SubagentStatus::Failed),
                    _ => None,
                };
                if let Some(status) = status {
                    if let Err(error) = store.update_subagent_status(parent, child, status).await {
                        tracing::warn!("persist subagent status: {error}");
                    }
                    break;
                }
                if updates.changed().await.is_err() {
                    break;
                }
            }
        });
        Ok((child, job.id().clone()))
    }

    async fn control(
        &self,
        parent: SessionId,
        child: SessionId,
        action: &str,
        message: Option<String>,
    ) -> Result<()> {
        let descriptor = self
            .store
            .list_subagents(parent)
            .await?
            .into_iter()
            .find(|descriptor| descriptor.child == child)
            .ok_or_else(|| CoreError::Tool("unknown subagent".into()))?;
        let job = JobId(descriptor.job_id);
        if descriptor.status != SubagentStatus::Running {
            return Err(CoreError::Tool("subagent has already finished".into()));
        }
        let snapshot = self
            .jobs
            .list(parent)
            .into_iter()
            .find(|snapshot| snapshot.id == job)
            .ok_or_else(|| CoreError::Tool("unknown subagent".into()))?;
        if !matches!(
            snapshot.status,
            crate::job::JobStatus::Running | crate::job::JobStatus::Stopping
        ) {
            return Err(CoreError::Tool("subagent has already finished".into()));
        }
        match action {
            "interrupt" => self.jobs.kill(parent, &job),
            "send_message" => {
                let message = message
                    .filter(|text| !text.trim().is_empty() && text.len() <= 32_768)
                    .ok_or_else(|| CoreError::Tool("message must be 1 to 32768 bytes".into()))?;
                self.store
                    .append_message(child, &crate::message::Message::user(message))
                    .await
            }
            _ => Err(CoreError::Tool(
                "action must be send_message or interrupt".into(),
            )),
        }
    }

    async fn list(&self, parent: SessionId) -> Result<Vec<SubagentDescriptor>> {
        self.store.list_subagents(parent).await
    }

    async fn deliver_notices(&self, parent: SessionId) -> Result<Vec<crate::message::Message>> {
        let mut messages = Vec::new();
        for descriptor in self.store.list_subagents(parent).await? {
            if descriptor.status != SubagentStatus::Completed || descriptor.notice_delivered {
                continue;
            }
            if let Some(result) = descriptor.result {
                let message = crate::message::Message::user(format!(
                    "[Subagent {} completed]\n{result}",
                    descriptor.label
                ));
                if self
                    .store
                    .append_subagent_notice(parent, descriptor.child, &message)
                    .await?
                {
                    messages.push(message);
                }
            }
        }
        Ok(messages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ProviderConfig, ProviderKind};
    use crate::job::{JobRegistry, JobStatus, LocalJobRegistry};
    use crate::message::FinishReason;
    use crate::permission::PermissionService;
    use crate::provider::testutil::ScriptedProvider;
    use crate::provider::StreamChunk;
    use crate::session::sqlite::SqliteStore;
    use crate::tools::subagent::SubagentTool;

    #[tokio::test]
    async fn runs_a_child_and_queues_its_parent_notice_without_recursive_tool() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("subagents.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let parent = store.create_session("/project").await.unwrap();
        let provider: Arc<dyn Provider> = Arc::new(ScriptedProvider {
            script: vec![
                StreamChunk::TextDelta("child result".into()),
                StreamChunk::Done(FinishReason::Stop),
            ],
        });
        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(SubagentTool));
        let jobs = Arc::new(LocalJobRegistry::default());
        let runtime = InProcessSubagentLauncher::new(
            Arc::new(RwLock::new(provider)),
            store.clone(),
            Arc::new(tools),
            Arc::new(PermissionService::allow_all()),
            Arc::new(RwLock::new(CodeLoomConfig {
                provider: ProviderConfig {
                    kind: ProviderKind::OpenAiCompat,
                    base_url: "http://localhost".into(),
                    api_key_env: "KEY".into(),
                },
                model: "test".into(),
                max_steps: 2,
            })),
            dir.path().to_path_buf(),
            jobs.clone(),
        );

        assert!(runtime.tools.get("subagent").is_none());
        assert!(runtime.tools.get("subagent_control").is_none());
        assert!(runtime.tools.get("subagent_list").is_none());
        let (child, job) = runtime
            .start(
                parent,
                "research".into(),
                "do work".into(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(runtime
            .control(SessionId(999), child, "interrupt", None)
            .await
            .is_err());
        let outcome = jobs.wait(parent, &job).await.unwrap();

        assert_eq!(outcome.status, JobStatus::Completed);
        assert!(store.messages(child).await.unwrap().iter().any(|message| {
            message.role == crate::message::Role::Assistant
                && message.text().contains("child result")
        }));
        assert!(runtime
            .deliver_notices(parent)
            .await
            .unwrap()
            .iter()
            .any(|message| message.text().contains("child result")));
        assert!(runtime.deliver_notices(parent).await.unwrap().is_empty());
        assert!(runtime
            .control(parent, child, "interrupt", None)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn child_registry_hides_ask_user() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("subagents.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let provider: Arc<dyn Provider> = Arc::new(ScriptedProvider { script: vec![] });
        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(crate::tools::ask_user::AskUserTool));
        assert!(tools.get("ask_user").is_some());
        let runtime = InProcessSubagentLauncher::new(
            Arc::new(RwLock::new(provider)),
            store,
            Arc::new(tools),
            Arc::new(PermissionService::allow_all()),
            Arc::new(RwLock::new(CodeLoomConfig {
                provider: ProviderConfig {
                    kind: ProviderKind::OpenAiCompat,
                    base_url: "http://localhost".into(),
                    api_key_env: "KEY".into(),
                },
                model: "test".into(),
                max_steps: 2,
            })),
            dir.path().to_path_buf(),
            Arc::new(LocalJobRegistry::default()),
        );
        assert!(runtime.tools.get("ask_user").is_none());
    }
}
