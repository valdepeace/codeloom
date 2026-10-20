use super::{ExecutionMode, Tool, ToolCtx, ToolOutput};
use crate::error::{CoreError, Result};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

const BANNED: [&str; 3] = ["rm -rf /", "mkfs", ":(){"];

pub struct BashTool;

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }
    fn description(&self) -> &str {
        "Run a shell command in the workspace. Params: command (string)."
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({"type":"object","properties":{"command":{"type":"string"}},"required":["command"]})
    }
    fn execution_mode(&self, _i: &serde_json::Value) -> ExecutionMode {
        ExecutionMode::Exclusive
    }
    async fn run(
        &self,
        ctx: ToolCtx,
        input: serde_json::Value,
        cancel: CancellationToken,
    ) -> Result<ToolOutput> {
        let command = input["command"].as_str().unwrap_or_default();
        if BANNED.iter().any(|b| command.contains(b)) {
            return Ok(ToolOutput::err("command blocked by policy"));
        }
        let (prog, flag) = if cfg!(windows) {
            ("cmd", "/C")
        } else {
            ("sh", "-c")
        };
        let child = tokio::process::Command::new(prog)
            .arg(flag)
            .arg(command)
            .current_dir(&ctx.cwd)
            .kill_on_drop(true)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| CoreError::Tool(e.to_string()))?;
        let out = tokio::select! {
            out = child.wait_with_output() => out.map_err(|e| CoreError::Tool(e.to_string()))?,
            _ = cancel.cancelled() => return Ok(ToolOutput::err("cancelled")),
        };
        let mut text = String::from_utf8_lossy(&out.stdout).to_string();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        if out.status.success() {
            Ok(ToolOutput::ok(text))
        } else {
            Ok(ToolOutput::err(text))
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
    async fn bash_runs_command() {
        let dir = tempfile::tempdir().unwrap();
        let out = BashTool
            .run(
                ctx(dir.path()),
                serde_json::json!({ "command": "echo hi" }),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(out.content.contains("hi"));
    }
}
