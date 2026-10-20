use super::{ExecutionMode, Tool, ToolCtx, ToolOutput};
use crate::error::Result;
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

pub struct SubagentListTool;

#[async_trait]
impl Tool for SubagentListTool {
    fn name(&self) -> &str {
        "subagent_list"
    }

    fn description(&self) -> &str {
        "List durable child-agent descriptors for the current session, including jobs interrupted by restart."
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
        let agents = match &ctx.subagents {
            Some(runtime) => runtime.list(ctx.session).await?,
            None => Vec::new(),
        };
        let result: Vec<_> = agents
            .into_iter()
            .map(|agent| {
                serde_json::json!({
                    "child_id": agent.child.0,
                    "job_id": agent.job_id,
                    "label": agent.label,
                    "status": format!("{:?}", agent.status).to_lowercase(),
                })
            })
            .collect();
        Ok(ToolOutput::ok(serde_json::to_string(&result)?))
    }
}
