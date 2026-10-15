use crate::tools::{ExecutionMode, ToolCtx, ToolOutput, ToolRegistry};
use futures::StreamExt;

/// Ejecuta tool calls respetando barreras:
/// las `Exclusive` se ejecutan solas y en orden; las `Parallel` en un lote comun.
pub async fn execute_tools(
    registry: &ToolRegistry,
    ctx: ToolCtx,
    calls: Vec<(String, String, serde_json::Value)>,
    cancel: tokio_util::sync::CancellationToken,
) -> Vec<(String, ToolOutput)> {
    let mut batches: Vec<Vec<(String, String, serde_json::Value)>> = Vec::new();
    let mut current: Vec<(String, String, serde_json::Value)> = Vec::new();
    for (id, name, input) in calls {
        let mode = registry
            .get(&name)
            .map(|t| t.execution_mode(&input))
            .unwrap_or(ExecutionMode::Exclusive);
        match mode {
            ExecutionMode::Parallel => current.push((id, name, input)),
            ExecutionMode::Exclusive => {
                if !current.is_empty() {
                    batches.push(std::mem::take(&mut current));
                }
                batches.push(vec![(id, name, input)]);
            }
        }
    }
    if !current.is_empty() {
        batches.push(current);
    }

    let mut results = Vec::new();
    for batch in batches {
        let futs = batch.into_iter().map(|(id, name, input)| {
            let tool = registry.get(&name);
            let ctx = ctx.clone();
            let cancel = cancel.clone();
            async move {
                match tool {
                    Some(t) => {
                        let out = t
                            .run(ctx, input, cancel)
                            .await
                            .unwrap_or_else(|e| ToolOutput::err(e.to_string()));
                        (id, out)
                    }
                    None => (id, ToolOutput::err(format!("unknown tool: {name}"))),
                }
            }
        });
        let collected: Vec<_> = futures::stream::iter(futs)
            .buffer_unordered(8)
            .collect()
            .await;
        results.extend(collected);
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::PermissionService;
    use crate::tools::{ExecutionMode, Tool};
    use async_trait::async_trait;
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

    struct Echo;
    #[async_trait]
    impl Tool for Echo {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "e"
        }
        fn schema(&self) -> serde_json::Value {
            serde_json::json!({"type":"object"})
        }
        fn execution_mode(&self, _i: &serde_json::Value) -> ExecutionMode {
            ExecutionMode::Parallel
        }
        async fn run(
            &self,
            _c: ToolCtx,
            i: serde_json::Value,
            _x: CancellationToken,
        ) -> crate::error::Result<ToolOutput> {
            Ok(ToolOutput::ok(i["v"].as_str().unwrap_or_default()))
        }
    }

    #[tokio::test]
    async fn runs_parallel_batch() {
        let mut r = ToolRegistry::new();
        r.register(Arc::new(Echo));
        let ctx = ToolCtx {
            cwd: ".".into(),
            permission: Arc::new(PermissionService::allow_all()),
        };
        let calls = vec![
            (
                "c1".to_string(),
                "echo".to_string(),
                serde_json::json!({"v":"a"}),
            ),
            (
                "c2".to_string(),
                "echo".to_string(),
                serde_json::json!({"v":"b"}),
            ),
            (
                "c3".to_string(),
                "missing".to_string(),
                serde_json::json!({}),
            ),
        ];
        let mut res = execute_tools(&r, ctx, calls, CancellationToken::new()).await;
        res.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(res.len(), 3);
        assert_eq!(res[0].1.content, "a");
        assert!(res[2].1.is_error);
    }
}
