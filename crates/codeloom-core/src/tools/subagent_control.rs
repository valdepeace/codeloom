use super::{ExecutionMode, Tool, ToolCtx, ToolOutput};
use crate::error::{CoreError, Result};
use crate::session::SessionId;
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

pub struct SubagentControlTool;

#[async_trait]
impl Tool for SubagentControlTool {
    fn name(&self) -> &str {
        "subagent_control"
    }

    fn description(&self) -> &str {
        "Send a message to or interrupt a child agent. Params: child_id, action, and message for send_message."
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "child_id": { "type": "integer", "minimum": 1 },
                "action": { "type": "string", "enum": ["send_message", "interrupt"] },
                "message": { "type": "string", "maxLength": 32768 }
            },
            "required": ["child_id", "action"]
        })
    }

    fn execution_mode(&self, _: &serde_json::Value) -> ExecutionMode {
        ExecutionMode::Exclusive
    }

    async fn run(
        &self,
        ctx: ToolCtx,
        input: serde_json::Value,
        _: CancellationToken,
    ) -> Result<ToolOutput> {
        let child = input["child_id"]
            .as_i64()
            .filter(|id| *id > 0)
            .ok_or_else(|| CoreError::Tool("child_id must be a positive integer".into()))?;
        let action = input["action"]
            .as_str()
            .ok_or_else(|| CoreError::Tool("action is required".into()))?;
        let launcher = ctx
            .subagents
            .as_ref()
            .ok_or_else(|| CoreError::Tool("subagents are unavailable in this context".into()))?;
        launcher
            .control(
                ctx.session,
                SessionId(child),
                action,
                input["message"].as_str().map(str::to_string),
            )
            .await?;
        Ok(ToolOutput::ok(format!(
            "Subagent {child}: {action} requested."
        )))
    }
}
