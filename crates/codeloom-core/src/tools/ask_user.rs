//! Herramienta `ask_user`: pregunta al usuario con opciones y espera su respuesta.

use super::{ExecutionMode, Tool, ToolCtx, ToolOutput};
use crate::error::Result;
use crate::question::{format_answers, parse_questions};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

pub struct AskUserTool;

#[async_trait]
impl Tool for AskUserTool {
    fn name(&self) -> &str {
        "ask_user"
    }

    fn description(&self) -> &str {
        "Ask the user 1 to 4 multiple-choice questions and wait for the answers. Each question needs 2 to 6 options; the user can always type an own answer. Use it only when a decision really needs the user. Params: questions (array of {header, question, options: [{label, description}], multi_select})."
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "questions": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 4,
                    "items": {
                        "type": "object",
                        "properties": {
                            "header": { "type": "string" },
                            "question": { "type": "string" },
                            "options": {
                                "type": "array",
                                "minItems": 2,
                                "maxItems": 6,
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "label": { "type": "string" },
                                        "description": { "type": "string" }
                                    },
                                    "required": ["label"]
                                }
                            },
                            "multi_select": { "type": "boolean" }
                        },
                        "required": ["question", "options"]
                    }
                }
            },
            "required": ["questions"]
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
        let questions = match parse_questions(&input) {
            Ok(questions) => questions,
            Err(message) => return Ok(ToolOutput::err(message)),
        };
        let answers = tokio::select! {
            _ = cancel.cancelled() => return Ok(ToolOutput::err("cancelled")),
            answers = ctx.permission.ask_user(&questions) => answers,
        };
        match answers {
            Some(answers) => Ok(ToolOutput::ok(format_answers(&answers))),
            None => Ok(ToolOutput::err(
                "The user did not answer (no interactive user or the question was dismissed).",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::{Decision, PermissionRequest, PermissionResponder, PermissionService};
    use crate::question::{Answer, Question};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    /// Responder de prueba: responde lo indicado, opcionalmente tras una espera, y cuenta las preguntas.
    struct TestResponder {
        answers: Option<Vec<Answer>>,
        delay: Option<Duration>,
        asked: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl PermissionResponder for TestResponder {
        async fn decide(&self, _req: &PermissionRequest) -> Decision {
            Decision::Deny
        }

        async fn ask(&self, _questions: &[Question]) -> Option<Vec<Answer>> {
            self.asked.fetch_add(1, Ordering::SeqCst);
            if let Some(delay) = self.delay {
                tokio::time::sleep(delay).await;
            }
            self.answers.clone()
        }
    }

    fn ctx(dir: &std::path::Path, permission: PermissionService) -> ToolCtx {
        ToolCtx {
            cwd: dir.to_path_buf(),
            permission: Arc::new(permission),
            session: crate::session::SessionId(1),
            jobs: Arc::new(crate::job::LocalJobRegistry::default()),
            bus: crate::event::EventBus::new(8),
            subagents: None,
        }
    }

    fn valid_input() -> serde_json::Value {
        serde_json::json!({"questions": [{
            "question": "Q",
            "options": [{"label": "a"}, {"label": "b"}]
        }]})
    }

    #[tokio::test]
    async fn returns_the_formatted_answers() {
        let dir = tempfile::tempdir().unwrap();
        let responder = TestResponder {
            answers: Some(vec![Answer {
                question: "Q".into(),
                selected: vec!["a".into(), "b".into()],
            }]),
            delay: None,
            asked: Arc::new(AtomicUsize::new(0)),
        };
        let c = ctx(
            dir.path(),
            PermissionService::with_responder(Box::new(responder)),
        );
        let out = AskUserTool
            .run(c, valid_input(), CancellationToken::new())
            .await
            .unwrap();
        assert!(!out.is_error);
        assert_eq!(out.content, "Q: a, b");
    }

    #[tokio::test]
    async fn no_user_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path(), PermissionService::allow_all());
        let out = AskUserTool
            .run(c, valid_input(), CancellationToken::new())
            .await
            .unwrap();
        assert!(out.is_error);
        assert_eq!(
            out.content,
            "The user did not answer (no interactive user or the question was dismissed)."
        );
    }

    #[tokio::test]
    async fn cancellation_wins_while_waiting() {
        let dir = tempfile::tempdir().unwrap();
        let responder = TestResponder {
            answers: Some(vec![]),
            delay: Some(Duration::from_secs(10)),
            asked: Arc::new(AtomicUsize::new(0)),
        };
        let c = ctx(
            dir.path(),
            PermissionService::with_responder(Box::new(responder)),
        );
        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            trigger.cancel();
        });
        let out = AskUserTool.run(c, valid_input(), cancel).await.unwrap();
        assert!(out.is_error);
        assert_eq!(out.content, "cancelled");
    }

    #[tokio::test]
    async fn invalid_input_does_not_ask() {
        let dir = tempfile::tempdir().unwrap();
        let asked = Arc::new(AtomicUsize::new(0));
        let responder = TestResponder {
            answers: Some(vec![]),
            delay: None,
            asked: asked.clone(),
        };
        let c = ctx(
            dir.path(),
            PermissionService::with_responder(Box::new(responder)),
        );
        let out = AskUserTool
            .run(
                c,
                serde_json::json!({"questions": []}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(out.is_error);
        assert_eq!(out.content, "between 1 and 4 questions are required");
        assert_eq!(asked.load(Ordering::SeqCst), 0);
    }
}
