use super::{ExecutionMode, Tool, ToolCtx, ToolOutput};
use crate::error::{CoreError, Result};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

pub struct ViewTool;

#[async_trait]
impl Tool for ViewTool {
    fn name(&self) -> &str {
        "view"
    }
    fn description(&self) -> &str {
        "Read a file with line numbers. Params: path (string), offset (int), limit (int)."
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "offset": { "type": "integer" },
                "limit": { "type": "integer" }
            },
            "required": ["path"]
        })
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
        let path = input["path"].as_str().unwrap_or_default();
        let offset = input["offset"].as_u64().unwrap_or(0) as usize;
        let limit = input["limit"].as_u64().unwrap_or(200) as usize;
        let full = ctx.cwd.join(path);
        let text = tokio::fs::read_to_string(&full)
            .await
            .map_err(|e| CoreError::Tool(format!("view {path}: {e}")))?;
        let out = text
            .lines()
            .enumerate()
            .skip(offset)
            .take(limit)
            .map(|(i, l)| format!("{}\t{}", i + 1, l))
            .collect::<Vec<_>>()
            .join("\n");
        Ok(ToolOutput::ok(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::PermissionService;
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

    fn ctx(dir: &std::path::Path) -> ToolCtx {
        ToolCtx {
            cwd: dir.to_path_buf(),
            permission: Arc::new(PermissionService::allow_all()),
        }
    }

    #[tokio::test]
    async fn view_prints_line_numbers() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "l1\nl2\nl3\n").unwrap();
        let out = ViewTool
            .run(
                ctx(dir.path()),
                serde_json::json!({"path":"a.txt"}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(out.content.contains("1\tl1"));
        assert!(out.content.contains("3\tl3"));
    }

    #[tokio::test]
    async fn view_respects_offset_limit() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "a\nb\nc\nd\n").unwrap();
        let out = ViewTool
            .run(
                ctx(dir.path()),
                serde_json::json!({"path":"a.txt","offset":1,"limit":2}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(out.content.contains("2\tb"));
        assert!(out.content.contains("3\tc"));
        assert!(!out.content.contains("4\td"));
    }
}
