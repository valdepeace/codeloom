use crate::error::Result;
use crate::message::Message;
use async_trait::async_trait;

pub mod sqlite;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionId(pub i64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInfo {
    pub id: SessionId,
    pub preview: String,
    pub message_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubagentStatus {
    Running,
    Completed,
    Killed,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentDescriptor {
    pub parent: SessionId,
    pub child: SessionId,
    pub job_id: String,
    pub label: String,
    pub status: SubagentStatus,
    pub result: Option<String>,
    pub notice_delivered: bool,
}

#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn create_session(&self, project: &str) -> Result<SessionId>;
    async fn list_sessions(&self, project: &str) -> Result<Vec<SessionInfo>>;
    async fn append_message(&self, sid: SessionId, msg: &Message) -> Result<()>;
    async fn messages(&self, sid: SessionId) -> Result<Vec<Message>>;
    async fn save_subagent(&self, descriptor: &SubagentDescriptor) -> Result<()>;
    async fn update_subagent_status(
        &self,
        parent: SessionId,
        child: SessionId,
        status: SubagentStatus,
    ) -> Result<()>;
    async fn list_subagents(&self, parent: SessionId) -> Result<Vec<SubagentDescriptor>>;
    async fn complete_subagent(
        &self,
        parent: SessionId,
        child: SessionId,
        result: &str,
    ) -> Result<()>;
    async fn append_subagent_notice(
        &self,
        parent: SessionId,
        child: SessionId,
        message: &Message,
    ) -> Result<bool>;
}
