use super::{ExecutionMode, Tool, ToolCtx, ToolOutput};
use crate::error::{CoreError, Result};
use crate::session::SessionId;
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

pub struct SubagentControlTool;

/// Valida las respuestas de la acción `answer`: una cadena por pregunta, en orden.
fn parse_answers(value: &serde_json::Value) -> Result<Vec<String>> {
    let answers: Vec<String> = value
        .as_array()
        .filter(|items| !items.is_empty())
        .ok_or_else(|| CoreError::Tool("answers is required for answer".into()))?
        .iter()
        .map(|item| {
            item.as_str()
                .filter(|text| !text.is_empty() && text.len() <= 4096)
                .map(str::to_string)
                .ok_or_else(|| {
                    CoreError::Tool("each answer must be 1 to 4096 bytes of text".into())
                })
        })
        .collect::<Result<_>>()?;
    if answers.len() > 4 {
        return Err(CoreError::Tool("at most 4 answers are allowed".into()));
    }
    Ok(answers)
}

#[async_trait]
impl Tool for SubagentControlTool {
    fn name(&self) -> &str {
        "subagent_control"
    }

    fn description(&self) -> &str {
        "Send a message to, interrupt, or answer a question from a child agent. Params: child_id, action, message for send_message, and answers (one string per question, in order) for answer."
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "child_id": { "type": "integer", "minimum": 1 },
                "action": { "type": "string", "enum": ["send_message", "interrupt", "answer"] },
                "message": { "type": "string", "maxLength": 32768 },
                "answers": {
                    "type": "array",
                    "maxItems": 4,
                    "items": { "type": "string", "minLength": 1, "maxLength": 4096 }
                }
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
        if action == "answer" {
            let answers = parse_answers(&input["answers"])?;
            launcher
                .answer(ctx.session, SessionId(child), answers)
                .await?;
            return Ok(ToolOutput::ok(format!("Subagent {child}: answer sent.")));
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::JobId;
    use crate::message::Message;
    use crate::permission::PermissionService;
    use crate::session::SubagentDescriptor;
    use crate::subagent::SubagentLauncher;
    use std::sync::{Arc, Mutex};

    /// Lanzador de prueba: registra las respuestas recibidas por `answer`.
    #[derive(Default)]
    struct RecordingLauncher {
        answers: Mutex<Vec<(SessionId, SessionId, Vec<String>)>>,
    }

    #[async_trait]
    impl SubagentLauncher for RecordingLauncher {
        async fn start(
            &self,
            _parent: SessionId,
            _label: String,
            _prompt: String,
            _agent: Option<String>,
            _cancel: CancellationToken,
        ) -> Result<(SessionId, JobId)> {
            Err(CoreError::Tool("not used".into()))
        }

        async fn control(
            &self,
            _parent: SessionId,
            _child: SessionId,
            _action: &str,
            _message: Option<String>,
        ) -> Result<()> {
            Err(CoreError::Tool("not used".into()))
        }

        async fn list(&self, _parent: SessionId) -> Result<Vec<SubagentDescriptor>> {
            Ok(Vec::new())
        }

        async fn deliver_notices(&self, _parent: SessionId) -> Result<Vec<Message>> {
            Ok(Vec::new())
        }

        async fn answer(
            &self,
            parent: SessionId,
            child: SessionId,
            answers: Vec<String>,
        ) -> Result<()> {
            self.answers.lock().unwrap().push((parent, child, answers));
            Ok(())
        }
    }

    fn ctx(dir: &std::path::Path, launcher: Arc<RecordingLauncher>) -> ToolCtx {
        ToolCtx {
            cwd: dir.to_path_buf(),
            permission: Arc::new(PermissionService::allow_all()),
            session: SessionId(1),
            jobs: Arc::new(crate::job::LocalJobRegistry::default()),
            bus: crate::event::EventBus::new(8),
            subagents: Some(launcher),
        }
    }

    #[tokio::test]
    async fn subagent_control_answer_requires_answers() {
        let dir = tempfile::tempdir().unwrap();
        let launcher = Arc::new(RecordingLauncher::default());
        let tool = SubagentControlTool;

        for input in [
            serde_json::json!({"child_id": 2, "action": "answer"}),
            serde_json::json!({"child_id": 2, "action": "answer", "answers": []}),
        ] {
            let error = tool
                .run(
                    ctx(dir.path(), launcher.clone()),
                    input,
                    CancellationToken::new(),
                )
                .await
                .unwrap_err();
            assert!(
                matches!(&error, CoreError::Tool(message) if message == "answers is required for answer"),
                "error inesperado: {error:?}"
            );
        }
        assert!(launcher.answers.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn subagent_control_answer_calls_the_launcher() {
        let dir = tempfile::tempdir().unwrap();
        let launcher = Arc::new(RecordingLauncher::default());
        let tool = SubagentControlTool;

        let output = tool
            .run(
                ctx(dir.path(), launcher.clone()),
                serde_json::json!({
                    "child_id": 2,
                    "action": "answer",
                    "answers": ["Sí", "tarde"]
                }),
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert!(!output.is_error);
        assert_eq!(
            *launcher.answers.lock().unwrap(),
            vec![(
                SessionId(1),
                SessionId(2),
                vec!["Sí".to_string(), "tarde".to_string()]
            )]
        );
    }
}
