use super::{ExecutionMode, Tool, ToolCtx, ToolOutput};
use crate::error::{CoreError, Result};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

pub struct GrepTool;

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }
    fn description(&self) -> &str {
        "Search file contents by regex. Params: pattern (string), glob (optional string)."
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type":"object",
            "properties":{"pattern":{"type":"string"},"glob":{"type":"string"}},
            "required":["pattern"]
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
        let pattern = input["pattern"].as_str().unwrap_or_default();
        let re = regex::Regex::new(pattern).map_err(|e| CoreError::Tool(e.to_string()))?;
        let cwd = ctx.cwd.clone();
        let hits = tokio::task::spawn_blocking(move || {
            let mut out = Vec::new();
            for entry in ignore::WalkBuilder::new(&cwd).build().flatten() {
                if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                    continue;
                }
                let Ok(content) = std::fs::read_to_string(entry.path()) else {
                    continue;
                };
                for (i, line) in content.lines().enumerate() {
                    if re.is_match(line) {
                        out.push(format!("{}:{}: {}", entry.path().display(), i + 1, line));
                    }
                }
            }
            out
        })
        .await
        .map_err(|e| CoreError::Tool(e.to_string()))?;
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
        }
    }

    #[tokio::test]
    async fn grep_finds_matching_lines() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "foo\nbar\nfoobar\n").unwrap();
        let out = GrepTool
            .run(
                ctx(dir.path()),
                serde_json::json!({"pattern":"foo"}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(out.content.contains("a.txt:1: foo"));
        assert!(out.content.contains("a.txt:3: foobar"));
    }
}
