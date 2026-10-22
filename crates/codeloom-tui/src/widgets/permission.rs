use async_trait::async_trait;
use codeloom_core::permission::{Decision, PermissionRequest, PermissionResponder};
use codeloom_core::question::{Answer, Question};
use tokio::sync::{mpsc, oneshot};

use crate::permission_flow::QuestionReply;

pub struct UiPermissionResponder {
    tx: mpsc::Sender<(PermissionRequest, oneshot::Sender<Decision>)>,
    auto_allow: bool,
    /// Canal de preguntas (`ask_user`); sin él, `ask` responde `None`.
    questions: Option<mpsc::Sender<(Vec<Question>, QuestionReply)>>,
}

impl UiPermissionResponder {
    pub fn new(
        tx: mpsc::Sender<(PermissionRequest, oneshot::Sender<Decision>)>,
        auto_allow: bool,
    ) -> Self {
        Self {
            tx,
            auto_allow,
            questions: None,
        }
    }

    /// Activa las preguntas al usuario por `tx`.
    pub fn with_questions(mut self, tx: mpsc::Sender<(Vec<Question>, QuestionReply)>) -> Self {
        self.questions = Some(tx);
        self
    }
}

#[async_trait]
impl PermissionResponder for UiPermissionResponder {
    async fn decide(&self, req: &PermissionRequest) -> Decision {
        if self.auto_allow {
            return Decision::Allow;
        }
        let (reply_tx, reply_rx) = oneshot::channel();
        if self.tx.send((req.clone(), reply_tx)).await.is_err() {
            return Decision::Deny;
        }
        reply_rx.await.unwrap_or(Decision::Deny)
    }

    /// Pide aprobación al usuario salvo en modo yolo (`auto_allow`), que decide sin preguntar.
    fn requires_user(&self) -> bool {
        !self.auto_allow
    }

    async fn ask(&self, questions: &[Question]) -> Option<Vec<Answer>> {
        let tx = self.questions.as_ref()?;
        let (reply_tx, reply_rx) = oneshot::channel();
        if tx.send((questions.to_vec(), reply_tx)).await.is_err() {
            return None;
        }
        reply_rx.await.unwrap_or(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codeloom_core::question::QuestionOption;

    #[tokio::test]
    async fn forwards_and_returns_reply() {
        let (tx, mut rx) = mpsc::channel(4);
        let resp = UiPermissionResponder::new(tx, false);
        let req = PermissionRequest {
            tool: "bash".into(),
            action: "run".into(),
            detail: "ls".into(),
        };
        let handle = tokio::spawn(async move { resp.decide(&req).await });
        let (r, reply) = rx.recv().await.unwrap();
        assert_eq!(r.tool, "bash");
        reply.send(Decision::Allow).unwrap();
        assert_eq!(handle.await.unwrap(), Decision::Allow);
    }

    #[tokio::test]
    async fn yolo_allows_without_asking() {
        let (tx, _rx) = mpsc::channel(1);
        let resp = UiPermissionResponder::new(tx, true);
        let req = PermissionRequest {
            tool: "bash".into(),
            action: "run".into(),
            detail: String::new(),
        };
        assert_eq!(resp.decide(&req).await, Decision::Allow);
    }

    fn one_question() -> Vec<Question> {
        vec![Question {
            header: "Stack".into(),
            question: "Which?".into(),
            options: vec![QuestionOption {
                label: "a".into(),
                description: String::new(),
            }],
            multi_select: false,
        }]
    }

    #[tokio::test]
    async fn question_roundtrip_returns_answers() {
        let (tx, _perm_rx) = mpsc::channel(1);
        let (ask_tx, mut ask_rx) = mpsc::channel(4);
        let resp = UiPermissionResponder::new(tx, false).with_questions(ask_tx);
        let questions = one_question();
        let handle = tokio::spawn(async move { resp.ask(&questions).await });
        let (received, reply) = ask_rx.recv().await.unwrap();
        assert_eq!(received, one_question());
        let answers = vec![Answer {
            question: "Which?".into(),
            selected: vec!["a".into()],
        }];
        reply.send(Some(answers.clone())).unwrap();
        assert_eq!(handle.await.unwrap(), Some(answers));
    }

    #[test]
    fn ui_permission_responder_requires_user() {
        let (tx, _rx) = mpsc::channel(1);
        assert!(UiPermissionResponder::new(tx, false).requires_user());
        // En modo yolo nunca se pregunta: no hay aprobación humana que anunciar.
        let (tx, _rx) = mpsc::channel(1);
        assert!(!UiPermissionResponder::new(tx, true).requires_user());
    }

    #[tokio::test]
    async fn without_channel_ask_is_none() {
        let (tx, _perm_rx) = mpsc::channel(1);
        let resp = UiPermissionResponder::new(tx, false);
        assert_eq!(resp.ask(&one_question()).await, None);
    }
}
