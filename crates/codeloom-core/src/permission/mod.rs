use crate::question::{Answer, Question};
use async_trait::async_trait;
use std::collections::HashSet;
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct PermissionRequest {
    pub tool: String,
    pub action: String,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    /// Permitir y no volver a preguntar por este `tool:action` en la sesión.
    AllowSession,
    Deny,
}

#[async_trait]
pub trait PermissionResponder: Send + Sync {
    async fn decide(&self, req: &PermissionRequest) -> Decision;

    /// Pregunta al usuario y devuelve sus respuestas; `None` si no hay usuario interactivo
    /// o si descarta la pregunta. Por defecto no hay usuario.
    async fn ask(&self, _questions: &[Question]) -> Option<Vec<Answer>> {
        None
    }

    /// Indica si este responder puede pedir una aprobación humana al decidir. Un responder
    /// que siempre decide sin preguntar (modo yolo, `AllowAll`) devuelve `false`: así no se
    /// anuncia una aprobación que no existe. No recibe la petición: quien necesite saber si
    /// esta petición concreta se preguntará debe usar `PermissionService::would_prompt`.
    fn requires_user(&self) -> bool {
        false
    }
}

pub struct AllowAll;

#[async_trait]
impl PermissionResponder for AllowAll {
    async fn decide(&self, _req: &PermissionRequest) -> Decision {
        Decision::Allow
    }
}

pub struct DenyAll;

#[async_trait]
impl PermissionResponder for DenyAll {
    async fn decide(&self, _req: &PermissionRequest) -> Decision {
        Decision::Deny
    }
}

pub struct PermissionService {
    responder: Box<dyn PermissionResponder>,
    persistent: Mutex<HashSet<String>>,
}

impl PermissionService {
    pub fn allow_all() -> Self {
        Self {
            responder: Box::new(AllowAll),
            persistent: Mutex::new(HashSet::new()),
        }
    }
    pub fn with_responder(responder: Box<dyn PermissionResponder>) -> Self {
        Self {
            responder,
            persistent: Mutex::new(HashSet::new()),
        }
    }
    pub fn key(tool: &str, action: &str) -> String {
        format!("{tool}:{action}")
    }
    pub async fn check(&self, req: PermissionRequest) -> Decision {
        if self
            .persistent
            .lock()
            .await
            .contains(&Self::key(&req.tool, &req.action))
        {
            return Decision::Allow;
        }
        let decision = self.responder.decide(&req).await;
        if decision == Decision::AllowSession {
            self.grant_persistent(&req.tool, &req.action).await;
            return Decision::Allow;
        }
        decision
    }
    /// Indica si `check` de esta petición pediría una aprobación humana. Es `false` si el
    /// `tool:action` ya está concedido en la sesión; si no, depende del responder.
    pub async fn would_prompt(&self, req: &PermissionRequest) -> bool {
        if self
            .persistent
            .lock()
            .await
            .contains(&Self::key(&req.tool, &req.action))
        {
            return false;
        }
        self.responder.requires_user()
    }
    pub async fn grant_persistent(&self, tool: &str, action: &str) {
        self.persistent.lock().await.insert(Self::key(tool, action));
    }
    /// Delegue la pregunta en el responder; no pasa por la caché de permisos.
    pub async fn ask_user(&self, questions: &[Question]) -> Option<Vec<Answer>> {
        self.responder.ask(questions).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::question::{Answer, Question};

    fn req(t: &str, a: &str) -> PermissionRequest {
        PermissionRequest {
            tool: t.into(),
            action: a.into(),
            detail: String::new(),
        }
    }

    #[tokio::test]
    async fn allow_all_allows() {
        let svc = PermissionService::allow_all();
        assert_eq!(svc.check(req("bash", "run")).await, Decision::Allow);
    }

    #[tokio::test]
    async fn deny_all_denies() {
        let svc = PermissionService::with_responder(Box::new(DenyAll));
        assert_eq!(svc.check(req("bash", "run")).await, Decision::Deny);
    }

    #[tokio::test]
    async fn persistent_grant_bypasses_responder() {
        let svc = PermissionService::with_responder(Box::new(DenyAll));
        svc.grant_persistent("bash", "run").await;
        assert_eq!(svc.check(req("bash", "run")).await, Decision::Allow);
        assert_eq!(svc.check(req("bash", "other")).await, Decision::Deny);
    }

    struct SessionOnce(std::sync::Arc<std::sync::atomic::AtomicUsize>);

    #[async_trait]
    impl PermissionResponder for SessionOnce {
        async fn decide(&self, _req: &PermissionRequest) -> Decision {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Decision::AllowSession
        }
    }

    #[tokio::test]
    async fn allow_session_persists_for_the_same_tool_action() {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let svc = PermissionService::with_responder(Box::new(SessionOnce(calls.clone())));
        assert_eq!(svc.check(req("bash", "run")).await, Decision::Allow);
        assert_eq!(svc.check(req("bash", "run")).await, Decision::Allow);
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    struct Asker;

    #[async_trait]
    impl PermissionResponder for Asker {
        async fn decide(&self, _req: &PermissionRequest) -> Decision {
            Decision::Deny
        }

        async fn ask(&self, _questions: &[Question]) -> Option<Vec<Answer>> {
            Some(vec![Answer {
                question: "Q".into(),
                selected: vec!["a".into()],
            }])
        }
    }

    #[tokio::test]
    async fn ask_defaults_to_no_user() {
        let svc = PermissionService::with_responder(Box::new(DenyAll));
        assert_eq!(svc.ask_user(&[]).await, None);
        assert_eq!(PermissionService::allow_all().ask_user(&[]).await, None);
    }

    /// Responder que dice pedir al usuario, para comprobar `would_prompt` sin tocar `decide`.
    struct PreguntaUsuario;

    #[async_trait]
    impl PermissionResponder for PreguntaUsuario {
        async fn decide(&self, _req: &PermissionRequest) -> Decision {
            Decision::Allow
        }

        fn requires_user(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn would_prompt_is_false_for_persistent_grants() {
        let svc = PermissionService::with_responder(Box::new(PreguntaUsuario));
        assert!(svc.would_prompt(&req("bash", "run")).await);
        svc.grant_persistent("bash", "run").await;
        assert!(!svc.would_prompt(&req("bash", "run")).await);
        assert!(svc.would_prompt(&req("bash", "other")).await);
    }

    #[tokio::test]
    async fn would_prompt_follows_the_responder_when_nothing_is_granted() {
        assert!(
            !PermissionService::allow_all()
                .would_prompt(&req("bash", "run"))
                .await
        );
        assert!(
            !PermissionService::with_responder(Box::new(DenyAll))
                .would_prompt(&req("bash", "run"))
                .await
        );
    }

    #[tokio::test]
    async fn ask_delegates_to_the_responder() {
        let svc = PermissionService::with_responder(Box::new(Asker));
        assert_eq!(
            svc.ask_user(&[]).await,
            Some(vec![Answer {
                question: "Q".into(),
                selected: vec!["a".into()],
            }])
        );
    }
}
