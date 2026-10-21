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

/// Apunte de consumo: los tokens de una respuesta del proveedor y el precio vigente
/// (dólares por millón de tokens) en el momento de apuntarla. Los precios son `None`
/// cuando no se conocían; el apunte conserva su precio aunque el catálogo cambie después.
#[derive(Debug, Clone, PartialEq)]
pub struct UsageRecord {
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub input_price: Option<f64>,
    pub output_price: Option<f64>,
    pub created_at_ms: i64,
}

impl UsageRecord {
    /// Coste en dólares del apunte, o `None` si falta alguno de los dos precios.
    pub fn cost(&self) -> Option<f64> {
        let input = self.input_price? * self.input_tokens as f64 / 1_000_000.0;
        let output = self.output_price? * self.output_tokens as f64 / 1_000_000.0;
        Some(input + output)
    }
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
    /// Guarda un apunte de consumo bajo la sesión indicada.
    async fn record_usage(&self, session: SessionId, record: &UsageRecord) -> Result<()>;
    /// Apuntes de la sesión y de sus hijos (`subagents`), en orden de inserción.
    async fn session_usage(&self, session: SessionId) -> Result<Vec<UsageRecord>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_record_cost_none_without_prices() {
        let priced = UsageRecord {
            model: "m".into(),
            input_tokens: 1_000_000,
            output_tokens: 500_000,
            input_price: Some(3.0),
            output_price: Some(15.0),
            created_at_ms: 0,
        };
        let cost = priced.cost().expect("con ambos precios hay coste");
        assert!((cost - 10.5).abs() < 1e-9, "3 + 7.5 = 10.5, y salió {cost}");

        let without_input = UsageRecord {
            input_price: None,
            ..priced.clone()
        };
        assert_eq!(
            without_input.cost(),
            None,
            "sin precio de entrada no hay coste"
        );

        let without_output = UsageRecord {
            output_price: None,
            ..priced
        };
        assert_eq!(
            without_output.cost(),
            None,
            "sin precio de salida no hay coste"
        );
    }
}
