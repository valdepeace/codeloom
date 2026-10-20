use super::{ExecutionMode, Tool, ToolCtx, ToolOutput};
use crate::error::Result;
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

pub struct JobListTool;

#[async_trait]
impl Tool for JobListTool {
    fn name(&self) -> &str {
        "job_list"
    }

    fn description(&self) -> &str {
        "List background jobs owned by the current session."
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({ "type": "object", "properties": {} })
    }

    fn execution_mode(&self, _: &serde_json::Value) -> ExecutionMode {
        ExecutionMode::Parallel
    }

    async fn run(
        &self,
        ctx: ToolCtx,
        _: serde_json::Value,
        _: CancellationToken,
    ) -> Result<ToolOutput> {
        let jobs: Vec<_> = ctx
            .jobs
            .list(ctx.session)
            .into_iter()
            .map(|job| {
                serde_json::json!({
                    "id": job.id.0,
                    "kind": job.kind,
                    "label": job.label,
                    "status": format!("{:?}", job.status).to_lowercase(),
                    "progress": job.progress.map(|p| serde_json::json!({
                        "fraction": p.fraction,
                        "label": p.label
                    })),
                    "output_end": job.output_end,
                    "detail": job.detail,
                })
            })
            .collect();
        Ok(ToolOutput::ok(serde_json::to_string(&jobs)?))
    }
}
