use super::{ExecutionMode, Tool, ToolCtx, ToolOutput};
use crate::error::{CoreError, Result};
use crate::job::JobId;
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

pub struct JobOutputTool;

#[async_trait]
impl Tool for JobOutputTool {
    fn name(&self) -> &str {
        "job_output"
    }

    fn description(&self) -> &str {
        "Read retained output from a job owned by the current session. Params: id, from (byte offset)."
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": { "type": "string" },
                "from": { "type": "integer", "minimum": 0 }
            },
            "required": ["id"]
        })
    }

    fn execution_mode(&self, _: &serde_json::Value) -> ExecutionMode {
        ExecutionMode::Parallel
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
        let from = input["from"].as_u64().unwrap_or(0).min(usize::MAX as u64) as usize;
        let read = ctx.jobs.read(ctx.session, &JobId(id.into()), from)?;
        let output = serde_json::json!({
            "id": read.snapshot.id.0,
            "status": format!("{:?}", read.snapshot.status).to_lowercase(),
            "text": read.text,
            "next": read.next,
            "truncated": read.truncated,
        });
        Ok(ToolOutput::ok(serde_json::to_string(&output)?))
    }
}
