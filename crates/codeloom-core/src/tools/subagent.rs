use super::{ExecutionMode, Tool, ToolCtx, ToolOutput};
use crate::error::{CoreError, Result};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

pub struct SubagentTool;

#[async_trait]
impl Tool for SubagentTool {
    fn name(&self) -> &str {
        "subagent"
    }

    fn description(&self) -> &str {
        "Start an in-process child agent to work on a task independently. Returns child_id and job_id; use job_list/job_output/job_kill to manage it. Params: prompt and optional label."
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "prompt": { "type": "string", "minLength": 1, "maxLength": 32768 },
                "label": { "type": "string", "maxLength": 128 }
            },
            "required": ["prompt"]
        })
    }

    fn execution_mode(&self, _: &serde_json::Value) -> ExecutionMode {
        ExecutionMode::Exclusive
    }

    async fn run(
        &self,
        ctx: ToolCtx,
        input: serde_json::Value,
        cancel: CancellationToken,
    ) -> Result<ToolOutput> {
        let prompt = input["prompt"]
            .as_str()
            .filter(|s| !s.trim().is_empty() && s.len() <= 32_768)
            .ok_or_else(|| CoreError::Tool("prompt must be 1 to 32768 bytes".into()))?
            .to_string();
        let label = input["label"]
            .as_str()
            .filter(|s| s.len() <= 128 && !s.trim().is_empty())
            .unwrap_or("subagent")
            .to_string();
        let launcher = ctx
            .subagents
            .as_ref()
            .ok_or_else(|| CoreError::Tool("subagents are unavailable in this context".into()))?;
        // La herramienta todavía no elige agente: `None` es el agente `default` (tarea 6b lo cablea).
        let (child, job) = launcher
            .start(ctx.session, label, prompt, None, cancel)
            .await?;
        if let Some(descriptor) = launcher
            .list(ctx.session)
            .await?
            .into_iter()
            .find(|descriptor| descriptor.child == child)
        {
            ctx.bus
                .publish(crate::event::LiveEvent::SubagentChanged(descriptor));
        }
        super::job_start::track_job(&ctx, &job)?;
        Ok(ToolOutput::ok(
            serde_json::json!({
                "child_id": child.0,
                "job_id": job.0,
            })
            .to_string(),
        ))
    }
}
