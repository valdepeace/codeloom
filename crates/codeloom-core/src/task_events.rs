//! Eventos de tarea de A2A derivados del bus (A9).
//!
//! Cabe señalar que no existe un canal paralelo: los eventos salen de los `SubagentChanged` que el
//! bus ya publica, comparando cada descriptor con el anterior. Los campos son los del proto
//! `lf.a2a.v1` (sin indicador `final`: la v1.0 no lo tiene). Los nombres del cable los pone el
//! adaptador (`codeloom-a2a`).

use crate::a2a::{Artifact, Message};
use crate::event::LiveEvent;
use crate::session::{SessionId, SubagentDescriptor};
use crate::task::TaskStatus;
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use std::collections::VecDeque;
use tokio::sync::broadcast;

/// `TaskStatusUpdateEvent`: la tarea cambió de estado (o de mensaje de estado).
#[derive(Debug, Clone, PartialEq)]
pub struct TaskStatusUpdateEvent {
    pub task_id: String,
    pub context_id: String,
    pub status: TaskStatus,
    pub metadata: Option<serde_json::Value>,
}

/// `TaskArtifactUpdateEvent`: apareció un artefacto. Los nuestros llegan enteros, así que
/// `append` es falso y `last_chunk` verdadero.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskArtifactUpdateEvent {
    pub task_id: String,
    pub context_id: String,
    pub artifact: Artifact,
    pub append: bool,
    pub last_chunk: bool,
    pub metadata: Option<serde_json::Value>,
}

/// `StreamResponse`: exactamente una de sus cuatro cargas. La tarea es un `SubagentDescriptor`,
/// porque el núcleo no tiene un tipo `Task` propio.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamResponse {
    Task(SubagentDescriptor),
    Message(Message),
    StatusUpdate(TaskStatusUpdateEvent),
    ArtifactUpdate(TaskArtifactUpdateEvent),
}

/// Eventos que lleva el paso de `anterior` a `nuevo`: primero el de estado, si cambió el estado o
/// su mensaje, y luego uno por cada artefacto nuevo. No se inventan transiciones.
pub fn eventos_de_cambio(
    anterior: &SubagentDescriptor,
    nuevo: &SubagentDescriptor,
) -> Vec<StreamResponse> {
    let task_id = nuevo.child.0.to_string();
    let context_id = nuevo.context_id();
    let mut eventos = Vec::new();
    if anterior.status.state != nuevo.status.state
        || anterior.status.message != nuevo.status.message
    {
        eventos.push(StreamResponse::StatusUpdate(TaskStatusUpdateEvent {
            task_id: task_id.clone(),
            context_id: context_id.clone(),
            status: nuevo.status.clone(),
            metadata: None,
        }));
    }
    for artefacto in &nuevo.artifacts {
        if anterior
            .artifacts
            .iter()
            .all(|visto| visto.artifact_id != artefacto.artifact_id)
        {
            eventos.push(StreamResponse::ArtifactUpdate(TaskArtifactUpdateEvent {
                task_id: task_id.clone(),
                context_id: context_id.clone(),
                artifact: artefacto.clone(),
                append: false,
                last_chunk: true,
                metadata: None,
            }));
        }
    }
    eventos
}

/// Cómo releer la tarea cuando el suscriptor se atrasa y el bus le descarta eventos.
pub type Refrescar = Box<dyn Fn() -> BoxFuture<'static, Option<SubagentDescriptor>> + Send + Sync>;

struct Estado {
    rx: broadcast::Receiver<LiveEvent>,
    parent: SessionId,
    child: SessionId,
    ultimo: SubagentDescriptor,
    pendientes: VecDeque<StreamResponse>,
    terminal: bool,
    refrescar: Refrescar,
}

impl Estado {
    /// Fija `ultimo` y marca el cierre si la tarea llegó a un estado terminal.
    fn adoptar(&mut self, descriptor: SubagentDescriptor) {
        self.terminal = descriptor.status.state.is_terminal();
        self.ultimo = descriptor;
    }
}

/// Flujo de eventos de una tarea: empieza con una instantánea (`Task`), sigue con los eventos que
/// derivan de `SubagentChanged` y se cierra tras el estado terminal. En `input-required` y
/// `auth-required` sigue abierto. Si el suscriptor se atrasa y el bus descarta eventos, recibe una
/// instantánea nueva en lugar de un hueco. `rx` debe obtenerse **antes** de leer `actual`, para no
/// perder cambios entre ambos.
pub fn flujo_de_tarea(
    rx: broadcast::Receiver<LiveEvent>,
    actual: SubagentDescriptor,
    refrescar: Refrescar,
) -> BoxStream<'static, StreamResponse> {
    let mut estado = Estado {
        rx,
        parent: actual.parent,
        child: actual.child,
        ultimo: actual.clone(),
        pendientes: VecDeque::from([StreamResponse::Task(actual.clone())]),
        terminal: false,
        refrescar,
    };
    estado.adoptar(actual);
    Box::pin(futures::stream::unfold(estado, |mut estado| async move {
        loop {
            if let Some(evento) = estado.pendientes.pop_front() {
                return Some((evento, estado));
            }
            if estado.terminal {
                return None;
            }
            match estado.rx.recv().await {
                Ok(LiveEvent::SubagentChanged(nuevo))
                    if nuevo.parent == estado.parent && nuevo.child == estado.child =>
                {
                    let eventos = eventos_de_cambio(&estado.ultimo, &nuevo);
                    estado.pendientes.extend(eventos);
                    estado.adoptar(nuevo);
                }
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(_)) => match (estado.refrescar)().await {
                    Some(fresco) => {
                        estado
                            .pendientes
                            .push_back(StreamResponse::Task(fresco.clone()));
                        estado.adoptar(fresco);
                    }
                    None => return None,
                },
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventBus;
    use crate::task::TaskState;
    use futures::StreamExt;
    use std::time::Duration;

    fn descriptor(estado: TaskState, artefactos: Vec<Artifact>) -> SubagentDescriptor {
        SubagentDescriptor {
            parent: SessionId(4),
            child: SessionId(9),
            job_id: "subagent-1".into(),
            label: "obrero".into(),
            status: TaskStatus::new(estado),
            artifacts: artefactos,
            notice_delivered: false,
            agent: None,
            agent_origin: None,
            reference_task_ids: Vec::new(),
        }
    }

    fn sin_refresco() -> Refrescar {
        Box::new(|| Box::pin(async { None }))
    }

    #[test]
    fn a_state_change_emits_a_status_update() {
        let antes = descriptor(TaskState::Submitted, Vec::new());
        let despues = descriptor(TaskState::Working, Vec::new());

        let eventos = eventos_de_cambio(&antes, &despues);

        assert_eq!(eventos.len(), 1);
        match &eventos[0] {
            StreamResponse::StatusUpdate(evento) => {
                assert_eq!(evento.task_id, "9");
                assert_eq!(evento.context_id, "4");
                assert_eq!(evento.status.state, TaskState::Working);
            }
            otro => panic!("se esperaba un evento de estado, y salió {otro:?}"),
        }
    }

    #[test]
    fn an_unchanged_state_emits_nothing() {
        let antes = descriptor(TaskState::Working, Vec::new());
        // Solo cambia la marca de tiempo: no es un cambio de estado.
        let mut despues = antes.clone();
        despues.status.timestamp_ms += 5;

        assert!(eventos_de_cambio(&antes, &despues).is_empty());
    }

    #[test]
    fn a_changed_status_message_emits_a_status_update() {
        let antes = descriptor(TaskState::AuthRequired, Vec::new());
        let mut despues = antes.clone();
        despues.status.message = Some(Message::agent_text("necesito permiso"));

        let eventos = eventos_de_cambio(&antes, &despues);

        assert!(matches!(&eventos[..], [StreamResponse::StatusUpdate(_)]));
    }

    #[test]
    fn a_new_artifact_emits_an_artifact_update() {
        let antes = descriptor(TaskState::Working, Vec::new());
        let artefacto = Artifact::text_artifact("listo");
        let despues = descriptor(TaskState::Working, vec![artefacto.clone()]);

        let eventos = eventos_de_cambio(&antes, &despues);

        match &eventos[..] {
            [StreamResponse::ArtifactUpdate(evento)] => {
                assert_eq!(evento.artifact, artefacto);
                assert!(!evento.append);
                assert!(evento.last_chunk);
            }
            otro => panic!("se esperaba un evento de artefacto, y salió {otro:?}"),
        }
        // Un artefacto ya visto no se repite.
        assert!(eventos_de_cambio(&despues, &despues).is_empty());
    }

    #[test]
    fn events_keep_the_order_state_then_artifacts() {
        let antes = descriptor(TaskState::Working, Vec::new());
        let despues = descriptor(
            TaskState::Completed,
            vec![Artifact::text_artifact("a"), Artifact::text_artifact("b")],
        );

        let eventos = eventos_de_cambio(&antes, &despues);

        assert!(matches!(
            &eventos[..],
            [
                StreamResponse::StatusUpdate(_),
                StreamResponse::ArtifactUpdate(_),
                StreamResponse::ArtifactUpdate(_)
            ]
        ));
    }

    async fn siguiente(flujo: &mut BoxStream<'static, StreamResponse>) -> Option<StreamResponse> {
        tokio::time::timeout(Duration::from_secs(2), flujo.next())
            .await
            .expect("el flujo debería producir algo o cerrarse")
    }

    #[tokio::test]
    async fn the_stream_starts_with_a_snapshot() {
        let bus = EventBus::new(16);
        let actual = descriptor(TaskState::Working, Vec::new());
        let mut flujo = flujo_de_tarea(bus.subscribe(), actual.clone(), sin_refresco());

        assert_eq!(
            siguiente(&mut flujo).await,
            Some(StreamResponse::Task(actual))
        );
    }

    #[tokio::test]
    async fn the_stream_closes_after_the_terminal_state() {
        let bus = EventBus::new(16);
        let inicial = descriptor(TaskState::Submitted, Vec::new());
        let mut flujo = flujo_de_tarea(bus.subscribe(), inicial, sin_refresco());
        bus.publish(LiveEvent::SubagentChanged(descriptor(
            TaskState::Working,
            Vec::new(),
        )));
        bus.publish(LiveEvent::SubagentChanged(descriptor(
            TaskState::Completed,
            vec![Artifact::text_artifact("listo")],
        )));

        assert!(matches!(
            siguiente(&mut flujo).await,
            Some(StreamResponse::Task(_))
        ));
        assert!(matches!(
            siguiente(&mut flujo).await,
            Some(StreamResponse::StatusUpdate(e)) if e.status.state == TaskState::Working
        ));
        assert!(matches!(
            siguiente(&mut flujo).await,
            Some(StreamResponse::StatusUpdate(e)) if e.status.state == TaskState::Completed
        ));
        assert!(matches!(
            siguiente(&mut flujo).await,
            Some(StreamResponse::ArtifactUpdate(_))
        ));
        assert_eq!(siguiente(&mut flujo).await, None);
    }

    #[tokio::test]
    async fn input_required_keeps_the_stream_open() {
        let bus = EventBus::new(16);
        let inicial = descriptor(TaskState::Working, Vec::new());
        let mut flujo = flujo_de_tarea(bus.subscribe(), inicial, sin_refresco());
        bus.publish(LiveEvent::SubagentChanged(descriptor(
            TaskState::InputRequired,
            Vec::new(),
        )));
        siguiente(&mut flujo).await; // instantánea
        siguiente(&mut flujo).await; // input-required

        // Sigue abierto: no cierra ni produce nada mientras no haya más cambios.
        assert!(
            tokio::time::timeout(Duration::from_millis(100), flujo.next())
                .await
                .is_err()
        );
        bus.publish(LiveEvent::SubagentChanged(descriptor(
            TaskState::Working,
            Vec::new(),
        )));
        assert!(matches!(
            siguiente(&mut flujo).await,
            Some(StreamResponse::StatusUpdate(e)) if e.status.state == TaskState::Working
        ));
    }

    #[tokio::test]
    async fn events_of_other_tasks_are_ignored() {
        let bus = EventBus::new(16);
        let inicial = descriptor(TaskState::Working, Vec::new());
        let mut flujo = flujo_de_tarea(bus.subscribe(), inicial, sin_refresco());
        let mut ajena = descriptor(TaskState::Completed, Vec::new());
        ajena.child = SessionId(10);
        bus.publish(LiveEvent::SubagentChanged(ajena));
        bus.publish(LiveEvent::TurnStarted);
        siguiente(&mut flujo).await; // instantánea

        assert!(
            tokio::time::timeout(Duration::from_millis(100), flujo.next())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn a_lagging_subscriber_gets_a_fresh_snapshot() {
        let bus = EventBus::new(2);
        let inicial = descriptor(TaskState::Submitted, Vec::new());
        let ultimo = descriptor(TaskState::InputRequired, Vec::new());
        let fresco = ultimo.clone();
        let refrescar: Refrescar = Box::new(move || {
            let fresco = fresco.clone();
            Box::pin(async move { Some(fresco) })
        });
        let mut flujo = flujo_de_tarea(bus.subscribe(), inicial, refrescar);
        siguiente(&mut flujo).await; // instantánea inicial
                                     // El suscriptor no consume: el bus (de 2) descarta los primeros.
        for _ in 0..5 {
            bus.publish(LiveEvent::SubagentChanged(descriptor(
                TaskState::Working,
                Vec::new(),
            )));
        }
        bus.publish(LiveEvent::SubagentChanged(ultimo.clone()));

        match siguiente(&mut flujo).await {
            Some(StreamResponse::Task(tarea)) => {
                assert_eq!(tarea.status.state, TaskState::InputRequired);
            }
            otro => panic!("se esperaba una instantánea nueva, y salió {otro:?}"),
        }
    }

    #[tokio::test]
    async fn two_subscribers_are_independent() {
        let bus = EventBus::new(16);
        let inicial = descriptor(TaskState::Working, Vec::new());
        let mut uno = flujo_de_tarea(bus.subscribe(), inicial.clone(), sin_refresco());
        let mut dos = flujo_de_tarea(bus.subscribe(), inicial, sin_refresco());
        bus.publish(LiveEvent::SubagentChanged(descriptor(
            TaskState::InputRequired,
            Vec::new(),
        )));
        siguiente(&mut uno).await;
        siguiente(&mut dos).await;
        drop(uno);

        assert!(matches!(
            siguiente(&mut dos).await,
            Some(StreamResponse::StatusUpdate(e)) if e.status.state == TaskState::InputRequired
        ));
    }
}
