//! Herramienta `todo_write`: reemplaza la lista de tareas de la sesión.

use super::{ExecutionMode, Tool, ToolCtx, ToolOutput};
use crate::error::Result;
use crate::event::LiveEvent;
use crate::todo::{parse_todos, TodoStatus};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

pub struct TodoWriteTool;

#[async_trait]
impl Tool for TodoWriteTool {
    fn name(&self) -> &str {
        "todo_write"
    }

    fn description(&self) -> &str {
        "Replace the whole todo list of this session. Always send every task, each with status pending, in_progress or completed. Keep at most one task in_progress. Params: todos (array of {content, status})."
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "todos": {
                    "type": "array",
                    "maxItems": 50,
                    "items": {
                        "type": "object",
                        "properties": {
                            "content": { "type": "string" },
                            "status": {
                                "type": "string",
                                "enum": ["pending", "in_progress", "completed"]
                            }
                        },
                        "required": ["content", "status"]
                    }
                }
            },
            "required": ["todos"]
        })
    }

    fn execution_mode(&self, _: &serde_json::Value) -> ExecutionMode {
        ExecutionMode::Exclusive
    }

    async fn run(
        &self,
        ctx: ToolCtx,
        input: serde_json::Value,
        _c: CancellationToken,
    ) -> Result<ToolOutput> {
        let items = match parse_todos(&input) {
            Ok(items) => items,
            Err(message) => return Ok(ToolOutput::err(message)),
        };
        let total = items.len();
        let done = items
            .iter()
            .filter(|t| t.status == TodoStatus::Completed)
            .count();
        let summary = match items.iter().find(|t| t.status == TodoStatus::InProgress) {
            Some(current) => format!(
                "Todo list updated: {done}/{total} completed · now: {}",
                current.content
            ),
            None => format!("Todo list updated: {done}/{total} completed"),
        };
        ctx.bus.publish(LiveEvent::TodosChanged {
            session: ctx.session,
            items,
        });
        Ok(ToolOutput::ok(summary))
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
            session: crate::session::SessionId(7),
            jobs: Arc::new(crate::job::LocalJobRegistry::default()),
            bus: crate::event::EventBus::new(8),
            subagents: None,
        }
    }

    #[tokio::test]
    async fn publishes_and_reports_progress() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let mut rx = c.bus.subscribe();
        let out = TodoWriteTool
            .run(
                c.clone(),
                serde_json::json!({"todos": [
                    {"content": "Primera", "status": "completed"},
                    {"content": "Segunda", "status": "in_progress"}
                ]}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(!out.is_error);
        assert_eq!(
            out.content,
            "Todo list updated: 1/2 completed · now: Segunda"
        );
        match rx.try_recv().unwrap() {
            LiveEvent::TodosChanged { session, items } => {
                assert_eq!(session, crate::session::SessionId(7));
                assert_eq!(items.len(), 2);
                assert_eq!(items[1].content, "Segunda");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[tokio::test]
    async fn invalid_input_is_an_error_and_publishes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let mut rx = c.bus.subscribe();
        let out = TodoWriteTool
            .run(
                c.clone(),
                serde_json::json!({"todos": [{"content": "", "status": "pending"}]}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(out.is_error);
        assert_eq!(out.content, "todo 1: empty content");
        assert!(rx.try_recv().is_err());
    }
}
