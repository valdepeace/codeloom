use crate::message::Message;
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub enum LiveEvent {
    TurnStarted,
    StepStarted {
        step: usize,
        max_steps: usize,
    },
    TurnEnded,
    Delta {
        text: String,
    },
    ReasoningDelta {
        text: String,
    },
    Usage {
        input_tokens: u32,
        output_tokens: u32,
    },
    MessageAdded(Message),
    SessionMessageAdded {
        session: crate::session::SessionId,
        message: Message,
    },
    ToolStarted {
        call_id: String,
        name: String,
    },
    ToolFinished {
        call_id: String,
        is_error: bool,
    },
    FileDiff {
        path: String,
        added: usize,
        removed: usize,
    },
    JobChanged(crate::job::JobSnapshot),
    SubagentChanged(crate::session::SubagentDescriptor),
    /// Un hijo queda a la espera de una respuesta de su padre (`input-required`).
    SubagentInputRequested {
        parent: crate::session::SessionId,
        child: crate::session::SessionId,
        label: String,
    },
    /// La espera del hijo termina: respondida, descartada o cancelada.
    SubagentInputResolved {
        parent: crate::session::SessionId,
        child: crate::session::SessionId,
    },
    TodosChanged {
        session: crate::session::SessionId,
        items: Vec<crate::todo::TodoItem>,
    },
    /// Un despertar del padre no encontró avisos: no hubo turno. Permite a la
    /// interfaz limpiar su marca de despertar en curso.
    WakeIdle,
    /// Un hijo ha llegado a su estado final y el estado ya está persistido. Se
    /// publica tras guardarlo, de modo que el padre puede leer el aviso al recibirlo.
    SubagentSettled {
        parent: crate::session::SessionId,
        child: crate::session::SessionId,
        status: crate::session::SubagentStatus,
    },
    Error {
        message: String,
    },
    /// Evento de un hijo reenviado al bus del padre por el lanzador de subagentes.
    /// Envuelve el evento original para que los manejadores existentes no lo
    /// confundan con uno propio: la interfaz lo despacha aparte.
    ChildEvent {
        parent: crate::session::SessionId,
        child: crate::session::SessionId,
        event: Box<LiveEvent>,
    },
}

impl LiveEvent {
    /// Indica si un evento del bus de un hijo merece reenviarse al padre. Quedan
    /// fuera los deltas de texto (volumen) y el resto de eventos de la sesión.
    pub fn is_child_relevant(&self) -> bool {
        match self {
            LiveEvent::TurnStarted
            | LiveEvent::StepStarted { .. }
            | LiveEvent::ToolStarted { .. }
            | LiveEvent::ToolFinished { .. }
            | LiveEvent::Usage { .. }
            | LiveEvent::FileDiff { .. }
            | LiveEvent::TurnEnded
            | LiveEvent::Error { .. } => true,
            LiveEvent::Delta { .. }
            | LiveEvent::ReasoningDelta { .. }
            | LiveEvent::MessageAdded(_)
            | LiveEvent::SessionMessageAdded { .. }
            | LiveEvent::JobChanged(_)
            | LiveEvent::SubagentChanged(_)
            | LiveEvent::SubagentInputRequested { .. }
            | LiveEvent::SubagentInputResolved { .. }
            | LiveEvent::TodosChanged { .. }
            | LiveEvent::WakeIdle
            | LiveEvent::SubagentSettled { .. }
            | LiveEvent::ChildEvent { .. } => false,
        }
    }
}

#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<LiveEvent>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(capacity);
        Self { tx }
    }
    pub fn subscribe(&self) -> broadcast::Receiver<LiveEvent> {
        self.tx.subscribe()
    }
    pub fn publish(&self, event: LiveEvent) {
        let _ = self.tx.send(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn publishes_to_subscriber() {
        let bus = EventBus::new(8);
        let mut rx = bus.subscribe();
        bus.publish(LiveEvent::TurnStarted);
        match rx.recv().await.unwrap() {
            LiveEvent::TurnStarted => {}
            other => panic!("unexpected {other:?}"),
        }
    }

    #[tokio::test]
    async fn publish_without_subscriber_is_ok() {
        let bus = EventBus::new(8);
        bus.publish(LiveEvent::Delta { text: "x".into() });
    }

    #[test]
    fn file_diff_is_forwarded_from_children() {
        let event = LiveEvent::FileDiff {
            path: "src/lib.rs".into(),
            added: 2,
            removed: 1,
        };
        assert!(event.is_child_relevant(), "debería reenviarse: {event:?}");
    }

    #[test]
    fn child_relevance_filter_excludes_deltas() {
        let session = crate::session::SessionId(1);
        let job = crate::job::JobSnapshot {
            id: crate::job::JobId("subagent-1".into()),
            kind: "subagent".into(),
            label: "obrero".into(),
            owner: session,
            status: crate::job::JobStatus::Running,
            progress: None,
            output: String::new(),
            output_start: 0,
            output_end: 0,
            detail: None,
            result: None,
        };
        let excluded = [
            LiveEvent::Delta { text: "x".into() },
            LiveEvent::ReasoningDelta { text: "x".into() },
            LiveEvent::MessageAdded(Message::user("hola")),
            LiveEvent::JobChanged(job),
        ];
        for event in &excluded {
            assert!(!event.is_child_relevant(), "debería filtrarse: {event:?}");
        }

        let included = [
            LiveEvent::TurnStarted,
            LiveEvent::StepStarted {
                step: 1,
                max_steps: 4,
            },
            LiveEvent::ToolStarted {
                call_id: "c1".into(),
                name: "glob".into(),
            },
            LiveEvent::ToolFinished {
                call_id: "c1".into(),
                is_error: false,
            },
            LiveEvent::Usage {
                input_tokens: 10,
                output_tokens: 2,
            },
            LiveEvent::TurnEnded,
            LiveEvent::Error {
                message: "fallo".into(),
            },
        ];
        for event in &included {
            assert!(event.is_child_relevant(), "debería reenviarse: {event:?}");
        }
    }
}
