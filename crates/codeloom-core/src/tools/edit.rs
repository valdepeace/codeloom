use super::{ExecutionMode, Tool, ToolCtx, ToolOutput};
use crate::error::{CoreError, Result};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

pub struct EditTool;

#[async_trait]
impl Tool for EditTool {
    fn name(&self) -> &str {
        "edit"
    }
    fn description(&self) -> &str {
        "Exact find/replace in a file. Params: path, old, new. Fails if old is missing or not unique."
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type":"object",
            "properties":{"path":{"type":"string"},"old":{"type":"string"},"new":{"type":"string"}},
            "required":["path","old","new"]
        })
    }
    fn execution_mode(&self, _i: &serde_json::Value) -> ExecutionMode {
        ExecutionMode::Exclusive
    }
    async fn run(
        &self,
        ctx: ToolCtx,
        input: serde_json::Value,
        _c: CancellationToken,
    ) -> Result<ToolOutput> {
        let path = input["path"].as_str().unwrap_or_default();
        let old = input["old"].as_str().unwrap_or_default();
        let new = input["new"].as_str().unwrap_or_default();
        let full = ctx.cwd.join(path);
        let content = tokio::fs::read_to_string(&full)
            .await
            .map_err(|e| CoreError::Tool(format!("edit {path}: {e}")))?;
        match content.matches(old).count() {
            0 => Ok(ToolOutput::err(format!("old text not found in {path}"))),
            1 => {
                let updated = content.replacen(old, new, 1);
                tokio::fs::write(&full, updated)
                    .await
                    .map_err(|e| CoreError::Tool(format!("edit {path}: {e}")))?;
                Ok(ToolOutput::ok(format!("edited {path}")))
            }
            n => Ok(ToolOutput::err(format!(
                "old text not unique in {path} ({n} matches)"
            ))),
        }
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
    async fn edit_replaces_unique_match() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "hello world").unwrap();
        let out = EditTool
            .run(
                ctx(dir.path()),
                serde_json::json!({"path":"a.txt","old":"world","new":"rust"}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(!out.is_error);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "hello rust"
        );
    }

    #[tokio::test]
    async fn edit_rejects_missing_or_ambiguous() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "x x").unwrap();
        let out = EditTool
            .run(
                ctx(dir.path()),
                serde_json::json!({"path":"a.txt","old":"x","new":"y"}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(out.is_error);
    }
}
