use crate::a2a::{artifacts_text, Artifact};
use crate::error::Result;
use crate::message::Message;
use crate::task::TaskStatus;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentDescriptor {
    pub parent: SessionId,
    pub child: SessionId,
    pub job_id: String,
    pub label: String,
    /// Estado de la tarea del subagente, según A2A: el estado es el `TaskState` mismo.
    pub status: TaskStatus,
    /// Resultado de la tarea, como `Artifact`s de A2A. Vacío mientras no ha terminado.
    pub artifacts: Vec<Artifact>,
    pub notice_delivered: bool,
}

impl SubagentDescriptor {
    /// Texto del resultado: el de todos los artefactos, unidos con `"\n"`. `None` si el
    /// subagente no tiene artefactos.
    pub fn result_text(&self) -> Option<String> {
        if self.artifacts.is_empty() {
            None
        } else {
            Some(artifacts_text(&self.artifacts))
        }
    }

    /// Identificador de contexto de A2A: la sesión del padre, en texto.
    pub fn context_id(&self) -> String {
        self.parent.0.to_string()
    }
}

/// Apunte de consumo: los tokens de una respuesta del proveedor y el precio vigente
/// (dólares por millón de tokens) en el momento de apuntarla. Los precios son `None`
/// cuando no se conocían; el apunte conserva su precio aunque el catálogo cambie después.
/// `session` es la sesión que originó el consumo: el padre para su propio turno y el hijo
/// para lo que gasta un subagente.
#[derive(Debug, Clone, PartialEq)]
pub struct UsageRecord {
    pub session: SessionId,
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
    /// Fija el estado de la tarea del hijo. Un estado terminal no admite transiciones
    /// posteriores: el almacén devuelve error si el actual ya lo es.
    async fn update_subagent_status(
        &self,
        parent: SessionId,
        child: SessionId,
        status: TaskStatus,
    ) -> Result<()>;
    async fn list_subagents(&self, parent: SessionId) -> Result<Vec<SubagentDescriptor>>;
    /// Marca el hijo como `Completed` y guarda su resultado como artefactos.
    async fn complete_subagent(
        &self,
        parent: SessionId,
        child: SessionId,
        artifacts: &[Artifact],
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
            session: SessionId(1),
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

    fn descriptor_sin_artefactos() -> SubagentDescriptor {
        use crate::task::TaskState;
        SubagentDescriptor {
            parent: SessionId(7),
            child: SessionId(8),
            job_id: "subagent-8".into(),
            label: "review".into(),
            status: TaskStatus::new(TaskState::Working),
            artifacts: Vec::new(),
            notice_delivered: false,
        }
    }

    #[test]
    fn result_text_is_none_without_artifacts() {
        use crate::a2a::Artifact;
        let sin = descriptor_sin_artefactos();
        assert_eq!(sin.result_text(), None);

        let con = SubagentDescriptor {
            artifacts: vec![
                Artifact::text_artifact("uno"),
                Artifact::text_artifact("dos"),
            ],
            ..sin
        };
        assert_eq!(con.result_text().as_deref(), Some("uno\ndos"));
    }

    #[test]
    fn context_id_is_the_parent_session_id() {
        assert_eq!(descriptor_sin_artefactos().context_id(), "7");
    }
}
