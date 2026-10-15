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
    pub async fn grant_persistent(&self, tool: &str, action: &str) {
        self.persistent.lock().await.insert(Self::key(tool, action));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
