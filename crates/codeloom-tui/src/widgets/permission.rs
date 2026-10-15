use async_trait::async_trait;
use codeloom_core::permission::{Decision, PermissionRequest, PermissionResponder};
use tokio::sync::{mpsc, oneshot};

pub struct UiPermissionResponder {
    tx: mpsc::Sender<(PermissionRequest, oneshot::Sender<Decision>)>,
    auto_allow: bool,
}

impl UiPermissionResponder {
    pub fn new(
        tx: mpsc::Sender<(PermissionRequest, oneshot::Sender<Decision>)>,
        auto_allow: bool,
    ) -> Self {
        Self { tx, auto_allow }
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
