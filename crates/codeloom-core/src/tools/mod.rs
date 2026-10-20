use crate::error::Result;
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub mod bash;
pub mod edit;
pub mod glob;
pub mod grep;
pub mod job_kill;
pub mod job_list;
pub mod job_output;
pub mod job_start;
pub mod subagent;
pub mod subagent_control;
pub mod subagent_list;
pub mod view;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    Parallel,
    Exclusive,
}

#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub content: String,
    pub is_error: bool,
}

impl ToolOutput {
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: false,
        }
    }
    pub fn err(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: true,
        }
    }
}

#[derive(Clone)]
pub struct ToolCtx {
    pub cwd: PathBuf,
    pub permission: Arc<crate::permission::PermissionService>,
    pub session: crate::session::SessionId,
    pub jobs: Arc<dyn crate::job::JobRegistry>,
    pub bus: crate::event::EventBus,
    pub subagents: Option<Arc<dyn crate::subagent::SubagentLauncher>>,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn schema(&self) -> serde_json::Value;
    fn execution_mode(&self, _input: &serde_json::Value) -> ExecutionMode {
        ExecutionMode::Parallel
    }
    async fn run(
        &self,
        ctx: ToolCtx,
        input: serde_json::Value,
        cancel: CancellationToken,
    ) -> Result<ToolOutput>;
}

#[derive(Default, Clone)]
pub struct ToolRegistry {
    tools: BTreeMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }
    pub fn without(&self, name: &str) -> Self {
        let mut registry = self.clone();
        registry.tools.remove(name);
        registry
    }
    pub fn specs(&self) -> Vec<crate::provider::ToolSpec> {
        self.tools
            .values()
            .map(|t| crate::provider::ToolSpec {
                name: t.name().to_string(),
                description: t.description().to_string(),
                input_schema: t.schema(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct Dummy;
    #[async_trait]
    impl Tool for Dummy {
        fn name(&self) -> &str {
            "dummy"
        }
        fn description(&self) -> &str {
            "d"
        }
        fn schema(&self) -> serde_json::Value {
            serde_json::json!({"type":"object"})
        }
        async fn run(
            &self,
            _c: ToolCtx,
            _i: serde_json::Value,
            _x: CancellationToken,
        ) -> Result<ToolOutput> {
            Ok(ToolOutput::ok("ok"))
        }
    }

    #[test]
    fn registers_and_lists_specs() {
        let mut r = ToolRegistry::new();
        r.register(std::sync::Arc::new(Dummy));
        assert!(r.get("dummy").is_some());
        assert_eq!(r.specs()[0].name, "dummy");
    }
}
