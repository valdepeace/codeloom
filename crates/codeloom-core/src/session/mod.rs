use crate::error::Result;
use crate::message::Message;
use async_trait::async_trait;

pub mod sqlite;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionId(pub i64);

#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn create_session(&self, project: &str) -> Result<SessionId>;
    async fn append_message(&self, sid: SessionId, msg: &Message) -> Result<()>;
    async fn messages(&self, sid: SessionId) -> Result<Vec<Message>>;
}
