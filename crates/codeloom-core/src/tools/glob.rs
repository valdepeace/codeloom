use super::{ExecutionMode, Tool, ToolCtx, ToolOutput};
use crate::error::{CoreError, Result};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

pub struct GlobTool;

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }
    fn description(&self) -> &str {
        "Find files by glob pattern. Params: pattern (string)."
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({"type":"object","properties":{"pattern":{"type":"string"}},"required":["pattern"]})
    }
    fn execution_mode(&self, _i: &serde_json::Value) -> ExecutionMode {
        ExecutionMode::Parallel
    }
    async fn run(
        &self,
        ctx: ToolCtx,
        input: serde_json::Value,
        _c: CancellationToken,
    ) -> Result<ToolOutput> {
        let pattern = input["pattern"].as_str().unwrap_or_default();
        let pat = ctx.cwd.join(pattern).to_string_lossy().to_string();
        let mut hits = Vec::new();
        for p in glob::glob(&pat)
            .map_err(|e| CoreError::Tool(e.to_string()))?
            .flatten()
        {
            hits.push(p.display().to_string());
        }
        hits.sort();
        Ok(ToolOutput::ok(hits.join("\n")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::PermissionService;
    use std::sync::Arc;

    fn ctx(dir: &std::path::Path) -> ToolCtx {
        ToolCtx {
            cwd: dir.to_path_buf(),
            permission: Arc::new(PermissionService::allow_all()),
            session: crate::session::SessionId(0),
            jobs: Arc::new(crate::job::LocalJobRegistry::default()),
            bus: crate::event::EventBus::new(8),
            subagents: None,
        }
    }

    #[tokio::test]
    async fn glob_finds_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "").unwrap();
        std::fs::write(dir.path().join("b.txt"), "").unwrap();
        let out = GlobTool
            .run(
                ctx(dir.path()),
                serde_json::json!({"pattern":"*.rs"}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(out.content.contains("a.rs"));
        assert!(!out.content.contains("b.txt"));
    }
}
