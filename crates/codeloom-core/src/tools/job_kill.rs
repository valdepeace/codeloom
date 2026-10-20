use super::{ExecutionMode, Tool, ToolCtx, ToolOutput};
use crate::error::{CoreError, Result};
use crate::job::JobId;
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

pub struct JobKillTool;

#[async_trait]
impl Tool for JobKillTool {
    fn name(&self) -> &str {
        "job_kill"
    }

    fn description(&self) -> &str {
        "Request cancellation of a running job owned by the current session. Params: id."
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": { "id": { "type": "string" } },
            "required": ["id"]
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
        let id = input["id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| CoreError::Tool("job id is required".into()))?;
        ctx.jobs.kill(ctx.session, &JobId(id.into()))?;
        Ok(ToolOutput::ok(format!(
            "Cancellation requested for job {id}."
        )))
    }
}
