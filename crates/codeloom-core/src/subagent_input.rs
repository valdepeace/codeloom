//! Buzón de preguntas de los subagentes (`input-required`).
//!
//! Cuando un hijo invoca `ask_user`, su petición queda aquí hasta que el padre
//! responde, la descarta el lanzador o el propio hijo deja de esperar. Es un
//! componente puro: no conoce la red ni la interfaz.

use crate::error::{CoreError, Result};
use crate::event::{EventBus, LiveEvent};
use crate::permission::{Decision, PermissionRequest, PermissionResponder, PermissionService};
use crate::question::{Answer, Question};
use crate::session::SessionId;
use crate::subagent::{resumen_de_permiso, TaskTracker};
use crate::task::{TaskState, TaskStatus};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::oneshot;

/// Petición de un hijo que espera respuesta de su padre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingInput {
    pub parent: SessionId,
    pub child: SessionId,
    pub label: String,
    pub questions: Vec<Question>,
}

/// Petición viva en el buzón. La clave del mapa es el `i64` del hijo, porque
/// `SessionId` no implementa `Hash`.
struct Entry {
    /// Identifica esta petición concreta; evita que el guard de otra la borre.
    ticket: u64,
    parent: SessionId,
    label: String,
    questions: Vec<Question>,
    announced: bool,
    tx: oneshot::Sender<Option<Vec<Answer>>>,
}

impl Entry {
    fn view(&self, child: i64) -> PendingInput {
        PendingInput {
            parent: self.parent,
            child: SessionId(child),
            label: self.label.clone(),
            questions: self.questions.clone(),
        }
    }
}

#[derive(Default)]
struct Inner {
    entries: HashMap<i64, Entry>,
    next_ticket: u64,
}

/// Buzón de preguntas pendientes, indexado por hijo.
#[derive(Default)]
pub struct InputBroker {
    inner: Mutex<Inner>,
}

impl InputBroker {
    /// Registra una petición del hijo y espera la respuesta del padre.
    ///
    /// Devuelve `None` si la petición se descarta (`discard`, o sustitución por
    /// una pregunta nueva del mismo hijo). Si el futuro se cancela, la entrada
    /// se retira al soltarlo.
    pub async fn request(
        &self,
        parent: SessionId,
        child: SessionId,
        label: String,
        questions: Vec<Question>,
    ) -> Option<Vec<Answer>> {
        self.request_with(parent, child, label, questions, || {})
            .await
    }

    /// Como `request`, pero llama a `on_registered` justo después de registrar la
    /// petición y sin mantener el cerrojo. Así quien anuncia la petición (p. ej. por
    /// el bus) puede confiar en que ya es visible en `pending`.
    pub async fn request_with(
        &self,
        parent: SessionId,
        child: SessionId,
        label: String,
        questions: Vec<Question>,
        on_registered: impl FnOnce(),
    ) -> Option<Vec<Answer>> {
        let (tx, rx) = oneshot::channel();
        let (ticket, replaced) = {
            let mut inner = self.lock();
            inner.next_ticket += 1;
            let ticket = inner.next_ticket;
            let replaced = inner.entries.insert(
                child.0,
                Entry {
                    ticket,
                    parent,
                    label,
                    questions,
                    announced: false,
                    tx,
                },
            );
            (ticket, replaced)
        };
        on_registered();
        // La petición sustituida se avisa fuera del cerrojo.
        if let Some(previous) = replaced {
            let _ = previous.tx.send(None);
        }
        let _cleanup = Cleanup {
            broker: self,
            child: child.0,
            ticket,
        };
        rx.await.ok().flatten()
    }

    /// Peticiones pendientes de un padre, ordenadas por hijo.
    pub fn pending(&self, parent: SessionId) -> Vec<PendingInput> {
        let inner = self.lock();
        let mut out: Vec<PendingInput> = inner
            .entries
            .iter()
            .filter(|(_, entry)| entry.parent == parent)
            .map(|(child, entry)| entry.view(*child))
            .collect();
        out.sort_by_key(|input| input.child.0);
        out
    }

    /// Devuelve las peticiones de un padre que aún no se han anunciado y las
    /// marca como anunciadas: cada una sale una sola vez. No las responde.
    pub fn take_unannounced(&self, parent: SessionId) -> Vec<PendingInput> {
        let mut inner = self.lock();
        let mut out: Vec<PendingInput> = inner
            .entries
            .iter_mut()
            .filter(|(_, entry)| entry.parent == parent && !entry.announced)
            .map(|(child, entry)| {
                entry.announced = true;
                entry.view(*child)
            })
            .collect();
        out.sort_by_key(|input| input.child.0);
        out
    }

    /// Responde la petición pendiente del hijo: una respuesta por pregunta, en orden.
    ///
    /// Si no hay petición del padre para ese hijo, o el número de respuestas no
    /// coincide, devuelve error y la petición sigue esperando.
    pub fn answer(&self, parent: SessionId, child: SessionId, answers: Vec<String>) -> Result<()> {
        let (tx, built) = {
            let mut inner = self.lock();
            let entry = match inner.entries.get(&child.0) {
                Some(entry) if entry.parent == parent => entry,
                _ => {
                    return Err(CoreError::Tool(format!(
                        "subagent {} has no pending question",
                        child.0
                    )))
                }
            };
            if answers.len() != entry.questions.len() {
                return Err(CoreError::Tool(format!(
                    "subagent {} asked {} question(s) but {} answer(s) were given",
                    child.0,
                    entry.questions.len(),
                    answers.len()
                )));
            }
            let built: Vec<Answer> = entry
                .questions
                .iter()
                .zip(answers)
                .map(|(question, selected)| Answer {
                    question: question.question.clone(),
                    selected: vec![selected],
                })
                .collect();
            let removed = inner.entries.remove(&child.0).ok_or_else(|| {
                CoreError::Tool(format!("subagent {} has no pending question", child.0))
            })?;
            (removed.tx, built)
        };
        // Un receptor ya cancelado no es un error: su guard ya limpió la entrada.
        let _ = tx.send(Some(built));
        Ok(())
    }

    /// Descarta la petición del hijo sin responder; su `request` devuelve `None`.
    pub fn discard(&self, child: SessionId) {
        let removed = self.lock().entries.remove(&child.0);
        if let Some(entry) = removed {
            let _ = entry.tx.send(None);
        }
    }

    /// Bloqueo del buzón. Un cerrojo envenenado no debe tumbar al resto de hijos.
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Retira la entrada de `request` si el futuro se suelta antes de tiempo. Solo
/// borra la entrada si sigue siendo la suya (mismo `ticket`).
struct Cleanup<'a> {
    broker: &'a InputBroker,
    child: i64,
    ticket: u64,
}

impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        let mut inner = self.broker.lock();
        if inner
            .entries
            .get(&self.child)
            .is_some_and(|entry| entry.ticket == self.ticket)
        {
            inner.entries.remove(&self.child);
        }
    }
}

/// Responder de un hijo. Los permisos se delegan en el servicio del padre, de modo
/// que comparten decisiones y caché; las preguntas (`ask_user`) se encaminan al
/// buzón y se anuncian en el bus como `SubagentInputRequested` / `SubagentInputResolved`.
pub struct ChildResponder {
    parent_service: Arc<PermissionService>,
    broker: Arc<InputBroker>,
    parent: SessionId,
    child: SessionId,
    label: String,
    bus: EventBus,
    tracker: Arc<TaskTracker>,
}

impl ChildResponder {
    pub fn new(
        parent_service: Arc<PermissionService>,
        broker: Arc<InputBroker>,
        parent: SessionId,
        child: SessionId,
        label: String,
        bus: EventBus,
        tracker: Arc<TaskTracker>,
    ) -> Self {
        Self {
            parent_service,
            broker,
            parent,
            child,
            label,
            bus,
            tracker,
        }
    }
}

/// Devuelve el hijo a `Working` al cerrarse la espera. Si el futuro que esperaba se suelta
/// antes de tiempo (cancelación o aborto), no hay `await` disponible: la transición se lanza
/// en segundo plano. Si la tarea ya terminó, el almacén la rechaza sin daño.
struct VuelveAWorking {
    tracker: Arc<TaskTracker>,
    parent: SessionId,
    child: SessionId,
    armado: bool,
}

impl VuelveAWorking {
    fn armar(tracker: &Arc<TaskTracker>, parent: SessionId, child: SessionId) -> Self {
        Self {
            tracker: tracker.clone(),
            parent,
            child,
            armado: true,
        }
    }

    /// Cierre normal de la espera: la transición se espera y la guarda deja de actuar.
    async fn resolver(&mut self) {
        self.armado = false;
        if let Err(error) = self
            .tracker
            .transition(self.parent, self.child, TaskStatus::new(TaskState::Working))
            .await
        {
            tracing::warn!("persist subagent status: {error}");
        }
    }
}

impl Drop for VuelveAWorking {
    fn drop(&mut self) {
        if !self.armado {
            return;
        }
        // Fuera de un runtime no hay a quién encargar la transición: se omite.
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let tracker = self.tracker.clone();
        let (parent, child) = (self.parent, self.child);
        handle.spawn(async move {
            if let Err(error) = tracker
                .transition(parent, child, TaskStatus::new(TaskState::Working))
                .await
            {
                tracing::debug!("subagent already settled: {error}");
            }
        });
    }
}

/// Publica `SubagentInputResolved` al soltarse, de forma que la marca de espera
/// desaparezca también cuando la pregunta se cancela a mitad.
struct ResolvedOnDrop {
    bus: EventBus,
    parent: SessionId,
    child: SessionId,
}

impl Drop for ResolvedOnDrop {
    fn drop(&mut self) {
        self.bus.publish(LiveEvent::SubagentInputResolved {
            parent: self.parent,
            child: self.child,
        });
    }
}

#[async_trait]
impl PermissionResponder for ChildResponder {
    async fn decide(&self, req: &PermissionRequest) -> Decision {
        // Sin aprobación humana pendiente (padre que permite sin preguntar, o permiso ya
        // concedido en la sesión) el hijo no pasa por `auth-required`: se delega sin tocar su estado.
        if !self.parent_service.would_prompt(req).await {
            return self.parent_service.check(req.clone()).await;
        }
        // El padre va a preguntar al usuario: mientras espera, el hijo está en `auth-required`.
        let mut espera = VuelveAWorking::armar(&self.tracker, self.parent, self.child);
        let resumen = resumen_de_permiso(&req.tool, &req.action, &req.detail);
        if let Err(error) = self
            .tracker
            .transition(
                self.parent,
                self.child,
                TaskStatus::new(TaskState::AuthRequired).with_message(resumen),
            )
            .await
        {
            tracing::warn!("persist subagent status: {error}");
        }
        let decision = self.parent_service.check(req.clone()).await;
        espera.resolver().await;
        decision
    }

    /// Un hijo puede anidar otro responder que pida al usuario, así que lo declara como `true`.
    /// La decisión por petición la toma `decide` consultando a `parent_service.would_prompt`.
    fn requires_user(&self) -> bool {
        true
    }

    async fn ask(&self, questions: &[Question]) -> Option<Vec<Answer>> {
        let _resolved = ResolvedOnDrop {
            bus: self.bus.clone(),
            parent: self.parent,
            child: self.child,
        };
        // Mientras la pregunta espera respuesta, el hijo está en `input-required` con su texto.
        let texto = questions
            .iter()
            .map(|question| question.question.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let mut espera = VuelveAWorking::armar(&self.tracker, self.parent, self.child);
        if let Err(error) = self
            .tracker
            .transition(
                self.parent,
                self.child,
                TaskStatus::new(TaskState::InputRequired).with_message(texto),
            )
            .await
        {
            tracing::warn!("persist subagent status: {error}");
        }
        // El aviso sale por el bus solo cuando la petición ya está en el buzón, para
        // que quien reaccione al evento (un despertar del padre) la encuentre.
        let bus = self.bus.clone();
        let (parent, child) = (self.parent, self.child);
        let label = self.label.clone();
        let respuesta = self
            .broker
            .request_with(
                self.parent,
                self.child,
                self.label.clone(),
                questions.to_vec(),
                move || {
                    bus.publish(LiveEvent::SubagentInputRequested {
                        parent,
                        child,
                        label,
                    });
                },
            )
            .await;
        espera.resolver().await;
        respuesta
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::question::QuestionOption;
    use crate::session::SessionStore;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    const PARENT: SessionId = SessionId(1);
    const CHILD: SessionId = SessionId(2);

    fn question(text: &str) -> Question {
        Question {
            header: "Cabecera".into(),
            question: text.into(),
            options: vec![
                QuestionOption {
                    label: "a".into(),
                    description: String::new(),
                },
                QuestionOption {
                    label: "b".into(),
                    description: String::new(),
                },
            ],
            multi_select: false,
        }
    }

    /// Espera, con cota de tiempo, a que el padre vea al menos una petición pendiente.
    async fn wait_for_pending(broker: &InputBroker, parent: SessionId) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while broker.pending(parent).is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("la petición debería aparecer en pending");
    }

    #[tokio::test]
    async fn request_waits_until_answered() {
        let broker = Arc::new(InputBroker::default());
        let questions = vec![question("¿Cuál?")];
        let b = broker.clone();
        let q = questions.clone();
        let handle =
            tokio::spawn(async move { b.request(PARENT, CHILD, "obrero".into(), q).await });

        wait_for_pending(&broker, PARENT).await;
        assert_eq!(broker.pending(PARENT).len(), 1);

        broker.answer(PARENT, CHILD, vec!["a".into()]).unwrap();
        let result = handle.await.unwrap();
        assert_eq!(
            result,
            Some(vec![Answer {
                question: "¿Cuál?".into(),
                selected: vec!["a".into()],
            }])
        );
        assert!(broker.pending(PARENT).is_empty());
    }

    #[tokio::test]
    async fn answer_without_pending_is_an_error() {
        let broker = InputBroker::default();
        let err = broker.answer(PARENT, CHILD, vec!["a".into()]).unwrap_err();
        assert!(
            matches!(err, CoreError::Tool(_)),
            "error inesperado: {err:?}"
        );
    }

    #[tokio::test]
    async fn answer_with_wrong_count_is_an_error_and_keeps_waiting() {
        let broker = Arc::new(InputBroker::default());
        let questions = vec![question("Primera"), question("Segunda")];
        let b = broker.clone();
        let q = questions.clone();
        let handle =
            tokio::spawn(async move { b.request(PARENT, CHILD, "obrero".into(), q).await });

        wait_for_pending(&broker, PARENT).await;
        let err = broker
            .answer(PARENT, CHILD, vec!["solo una".into()])
            .unwrap_err();
        assert!(
            matches!(err, CoreError::Tool(_)),
            "error inesperado: {err:?}"
        );
        assert_eq!(broker.pending(PARENT).len(), 1, "la petición debe seguir");

        broker
            .answer(PARENT, CHILD, vec!["x".into(), "y".into()])
            .unwrap();
        let result = handle.await.unwrap();
        assert_eq!(
            result,
            Some(vec![
                Answer {
                    question: "Primera".into(),
                    selected: vec!["x".into()],
                },
                Answer {
                    question: "Segunda".into(),
                    selected: vec!["y".into()],
                },
            ])
        );
    }

    #[tokio::test]
    async fn discard_unblocks_with_none_and_clears_entry() {
        let broker = Arc::new(InputBroker::default());
        let b = broker.clone();
        let handle = tokio::spawn(async move {
            b.request(PARENT, CHILD, "obrero".into(), vec![question("Q")])
                .await
        });

        wait_for_pending(&broker, PARENT).await;
        broker.discard(CHILD);
        assert_eq!(handle.await.unwrap(), None);
        assert!(broker.pending(PARENT).is_empty());
    }

    #[tokio::test]
    async fn dropping_the_request_future_clears_entry() {
        let broker = Arc::new(InputBroker::default());
        let b = broker.clone();
        let handle = tokio::spawn(async move {
            b.request(PARENT, CHILD, "obrero".into(), vec![question("Q")])
                .await
        });

        wait_for_pending(&broker, PARENT).await;
        handle.abort();
        // Al esperar al JoinError, la tarea ya se ha soltado y su guard ha corrido.
        assert!(handle.await.unwrap_err().is_cancelled());
        assert!(broker.pending(PARENT).is_empty());
    }

    #[tokio::test]
    async fn request_with_calls_the_callback_after_registration() {
        let broker = Arc::new(InputBroker::default());
        let observed = Arc::new(Mutex::new(None));
        let b = broker.clone();
        let o = observed.clone();
        let on_registered = move || {
            *o.lock().unwrap() = Some(b.pending(PARENT).len());
        };
        let b = broker.clone();
        let handle = tokio::spawn(async move {
            b.request_with(
                PARENT,
                CHILD,
                "obrero".into(),
                vec![question("Q")],
                on_registered,
            )
            .await
        });

        wait_for_pending(&broker, PARENT).await;
        broker.answer(PARENT, CHILD, vec!["a".into()]).unwrap();
        assert!(handle.await.unwrap().is_some());
        // El callback vio la petición ya registrada (y sin cerrojo: pending no bloquea).
        assert_eq!(*observed.lock().unwrap(), Some(1));
    }

    #[tokio::test]
    async fn take_unannounced_returns_each_request_once() {
        let broker = Arc::new(InputBroker::default());
        let b = broker.clone();
        let handle = tokio::spawn(async move {
            b.request(PARENT, CHILD, "obrero".into(), vec![question("Q")])
                .await
        });

        wait_for_pending(&broker, PARENT).await;
        let first = broker.take_unannounced(PARENT);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].label, "obrero");
        assert!(broker.take_unannounced(PARENT).is_empty());
        // Anunciar no equivale a responder: sigue pendiente.
        assert_eq!(broker.pending(PARENT).len(), 1);

        broker.discard(CHILD);
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn pending_is_scoped_to_the_parent() {
        let broker = Arc::new(InputBroker::default());
        let other_parent = SessionId(9);
        let other_child = SessionId(3);

        let b1 = broker.clone();
        let h1 = tokio::spawn(async move {
            b1.request(PARENT, CHILD, "uno".into(), vec![question("Q1")])
                .await
        });
        let b2 = broker.clone();
        let h2 = tokio::spawn(async move {
            b2.request(
                other_parent,
                other_child,
                "dos".into(),
                vec![question("Q2")],
            )
            .await
        });

        wait_for_pending(&broker, PARENT).await;
        wait_for_pending(&broker, other_parent).await;

        let mine = broker.pending(PARENT);
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0].child, CHILD);
        let theirs = broker.pending(other_parent);
        assert_eq!(theirs.len(), 1);
        assert_eq!(theirs[0].child, other_child);

        broker.discard(CHILD);
        broker.discard(other_child);
        h1.await.unwrap();
        h2.await.unwrap();
    }

    /// Extra (más allá de las 7 del plan): una petición nueva del mismo hijo sustituye
    /// a la pendiente; la anterior recibe `None` y su guard no borra la nueva.
    #[tokio::test]
    async fn new_request_from_same_child_replaces_the_previous_one() {
        let broker = Arc::new(InputBroker::default());

        let b1 = broker.clone();
        let first = tokio::spawn(async move {
            b1.request(PARENT, CHILD, "primera".into(), vec![question("Vieja")])
                .await
        });
        wait_for_pending(&broker, PARENT).await;

        let b2 = broker.clone();
        let second = tokio::spawn(async move {
            b2.request(PARENT, CHILD, "segunda".into(), vec![question("Nueva")])
                .await
        });

        assert_eq!(first.await.unwrap(), None);
        wait_for_pending(&broker, PARENT).await;
        let pending = broker.pending(PARENT);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].label, "segunda");

        broker.answer(PARENT, CHILD, vec!["ok".into()]).unwrap();
        assert_eq!(
            second.await.unwrap(),
            Some(vec![Answer {
                question: "Nueva".into(),
                selected: vec!["ok".into()],
            }])
        );
    }

    /// Tracker sobre un almacén SQLite temporal y vacío, para las pruebas que no miran el
    /// estado del hijo. El directorio se devuelve para que siga vivo durante la prueba.
    async fn tracker_vacio(bus: &EventBus) -> (tempfile::TempDir, Arc<TaskTracker>) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            crate::session::sqlite::SqliteStore::open(
                dir.path().join("vacio.db").to_str().unwrap(),
            )
            .await
            .unwrap(),
        );
        (dir, Arc::new(TaskTracker::new(store, bus.clone())))
    }

    /// Espera, con cota de tiempo, al siguiente evento del bus que no sea un cambio de estado de
    /// la tarea (esos llegan por `SubagentChanged` y tienen sus propias pruebas).
    async fn next_event(rx: &mut tokio::sync::broadcast::Receiver<LiveEvent>) -> LiveEvent {
        loop {
            let evento = tokio::time::timeout(Duration::from_secs(5), rx.recv())
                .await
                .expect("debería llegar un evento")
                .expect("el bus debería seguir abierto");
            if !matches!(evento, LiveEvent::SubagentChanged(_)) {
                return evento;
            }
        }
    }

    /// Responder del padre que cuenta las decisiones y siempre deniega.
    struct CountingDeny(Arc<AtomicUsize>);

    #[async_trait]
    impl PermissionResponder for CountingDeny {
        async fn decide(&self, _req: &PermissionRequest) -> Decision {
            self.0.fetch_add(1, Ordering::SeqCst);
            Decision::Deny
        }
    }

    #[tokio::test]
    async fn child_responder_delegates_permissions_to_the_parent() {
        let calls = Arc::new(AtomicUsize::new(0));
        let parent_service = Arc::new(PermissionService::with_responder(Box::new(CountingDeny(
            calls.clone(),
        ))));
        let (_dir, tracker) = tracker_vacio(&EventBus::new(8)).await;
        let child_service = PermissionService::with_responder(Box::new(ChildResponder::new(
            parent_service,
            Arc::new(InputBroker::default()),
            PARENT,
            CHILD,
            "obrero".into(),
            EventBus::new(8),
            tracker,
        )));
        let req = PermissionRequest {
            tool: "bash".into(),
            action: "run".into(),
            detail: String::new(),
        };
        assert_eq!(child_service.check(req).await, Decision::Deny);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn child_responder_publishes_requested_after_registration() {
        let broker = Arc::new(InputBroker::default());
        let bus = EventBus::new(8);
        let mut rx = bus.subscribe();
        let (_dir, tracker) = tracker_vacio(&bus).await;
        let service = Arc::new(PermissionService::with_responder(Box::new(
            ChildResponder::new(
                Arc::new(PermissionService::allow_all()),
                broker.clone(),
                PARENT,
                CHILD,
                "obrero".into(),
                bus.clone(),
                tracker,
            ),
        )));
        let questions = vec![question("¿Cuál?")];
        let s = service.clone();
        let q = questions.clone();
        let handle = tokio::spawn(async move { s.ask_user(&q).await });

        match next_event(&mut rx).await {
            LiveEvent::SubagentInputRequested { .. } => {}
            other => panic!("evento inesperado: {other:?}"),
        }
        // Al recibir el aviso, la petición ya debe estar registrada en el buzón del padre.
        assert_eq!(broker.pending(PARENT).len(), 1);

        broker.answer(PARENT, CHILD, vec!["a".into()]).unwrap();
        assert!(handle.await.unwrap().is_some());
    }

    #[tokio::test]
    async fn child_responder_ask_goes_through_the_broker_and_emits_events() {
        let broker = Arc::new(InputBroker::default());
        let bus = EventBus::new(8);
        let mut rx = bus.subscribe();
        let (_dir, tracker) = tracker_vacio(&bus).await;
        let service = Arc::new(PermissionService::with_responder(Box::new(
            ChildResponder::new(
                Arc::new(PermissionService::allow_all()),
                broker.clone(),
                PARENT,
                CHILD,
                "obrero".into(),
                bus.clone(),
                tracker,
            ),
        )));
        let questions = vec![question("¿Cuál?")];
        let s = service.clone();
        let q = questions.clone();
        let handle = tokio::spawn(async move { s.ask_user(&q).await });

        match next_event(&mut rx).await {
            LiveEvent::SubagentInputRequested {
                parent,
                child,
                label,
            } => {
                assert_eq!(parent, PARENT);
                assert_eq!(child, CHILD);
                assert_eq!(label, "obrero");
            }
            other => panic!("evento inesperado: {other:?}"),
        }
        wait_for_pending(&broker, PARENT).await;

        broker.answer(PARENT, CHILD, vec!["a".into()]).unwrap();
        assert_eq!(
            handle.await.unwrap(),
            Some(vec![Answer {
                question: "¿Cuál?".into(),
                selected: vec!["a".into()],
            }])
        );
        match next_event(&mut rx).await {
            LiveEvent::SubagentInputResolved { parent, child } => {
                assert_eq!(parent, PARENT);
                assert_eq!(child, CHILD);
            }
            other => panic!("evento inesperado: {other:?}"),
        }
    }

    /// Almacén SQLite con un padre y un hijo ya lanzado en `Working`, para observar sus estados.
    async fn almacen_con_hijo(
        dir: &std::path::Path,
    ) -> (
        Arc<crate::session::sqlite::SqliteStore>,
        SessionId,
        SessionId,
    ) {
        let store = Arc::new(
            crate::session::sqlite::SqliteStore::open(dir.join("hijo.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let parent = store.create_session("/project").await.unwrap();
        let child = store.create_session("/project").await.unwrap();
        store
            .save_subagent(&crate::session::SubagentDescriptor {
                parent,
                child,
                job_id: "subagent-1".into(),
                label: "obrero".into(),
                status: crate::task::TaskStatus::new(crate::task::TaskState::Working),
                result: None,
                notice_delivered: false,
            })
            .await
            .unwrap();
        (store, parent, child)
    }

    /// Estado de tarea del hijo tal como lo guarda el almacén.
    async fn estado_guardado(
        store: &dyn crate::session::SessionStore,
        parent: SessionId,
        child: SessionId,
    ) -> crate::task::TaskStatus {
        store
            .list_subagents(parent)
            .await
            .unwrap()
            .into_iter()
            .find(|descriptor| descriptor.child == child)
            .expect("el hijo debe estar guardado")
            .status
    }

    /// Espera, con cota de tiempo, a que el almacén registre el estado pedido.
    async fn espera_estado(
        store: &dyn crate::session::SessionStore,
        parent: SessionId,
        child: SessionId,
        estado: crate::task::TaskState,
    ) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while estado_guardado(store, parent, child).await.state != estado {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("el hijo debería quedar en {estado:?}"));
    }

    /// Lanzador de permisos del padre que no decide hasta que se le avisa; sirve para
    /// observar el estado `auth-required` del hijo mientras espera.
    struct EsperaDecision(Arc<tokio::sync::Notify>);

    #[async_trait]
    impl PermissionResponder for EsperaDecision {
        async fn decide(&self, _req: &PermissionRequest) -> Decision {
            self.0.notified().await;
            Decision::Allow
        }

        fn requires_user(&self) -> bool {
            true
        }
    }

    /// Recoge los estados `auth-required` que llegan al bus tras `rx`, sin esperar.
    fn auth_required_publicados(
        rx: &mut tokio::sync::broadcast::Receiver<LiveEvent>,
    ) -> Vec<TaskState> {
        let mut vistos = Vec::new();
        while let Ok(evento) = rx.try_recv() {
            if let LiveEvent::SubagentChanged(descriptor) = evento {
                if descriptor.status.state == TaskState::AuthRequired {
                    vistos.push(descriptor.status.state);
                }
            }
        }
        vistos
    }

    #[tokio::test]
    async fn child_decide_does_not_mark_auth_required_when_the_parent_allows_without_asking() {
        let dir = tempfile::tempdir().unwrap();
        let (store, parent, child) = almacen_con_hijo(dir.path()).await;
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();
        let tracker = Arc::new(crate::subagent::TaskTracker::new(
            store.clone(),
            bus.clone(),
        ));
        let service = PermissionService::with_responder(Box::new(ChildResponder::new(
            Arc::new(PermissionService::allow_all()),
            Arc::new(InputBroker::default()),
            parent,
            child,
            "obrero".into(),
            bus,
            tracker,
        )));

        let decision = service
            .check(PermissionRequest {
                tool: "bash".into(),
                action: "run".into(),
                detail: "ls".into(),
            })
            .await;
        assert_eq!(decision, Decision::Allow);
        assert!(
            auth_required_publicados(&mut rx).is_empty(),
            "sin aprobación humana no hay auth-required"
        );
        assert_eq!(
            estado_guardado(store.as_ref(), parent, child).await.state,
            TaskState::Working
        );
    }

    #[tokio::test]
    async fn child_decide_does_not_mark_auth_required_when_the_permission_was_granted_for_the_session(
    ) {
        let dir = tempfile::tempdir().unwrap();
        let (store, parent, child) = almacen_con_hijo(dir.path()).await;
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();
        let tracker = Arc::new(crate::subagent::TaskTracker::new(
            store.clone(),
            bus.clone(),
        ));
        // El padre pediría al usuario, pero ya concedió `bash:run` en la sesión.
        let parent_service = Arc::new(PermissionService::with_responder(Box::new(EsperaDecision(
            Arc::new(tokio::sync::Notify::new()),
        ))));
        parent_service.grant_persistent("bash", "run").await;
        let service = PermissionService::with_responder(Box::new(ChildResponder::new(
            parent_service,
            Arc::new(InputBroker::default()),
            parent,
            child,
            "obrero".into(),
            bus,
            tracker,
        )));

        let decision = service
            .check(PermissionRequest {
                tool: "bash".into(),
                action: "run".into(),
                detail: "ls".into(),
            })
            .await;
        assert_eq!(decision, Decision::Allow);
        assert!(auth_required_publicados(&mut rx).is_empty());
    }

    #[tokio::test]
    async fn child_decide_marks_auth_required_when_the_parent_will_prompt() {
        use crate::task::TaskState;
        let dir = tempfile::tempdir().unwrap();
        let (store, parent, child) = almacen_con_hijo(dir.path()).await;
        let bus = EventBus::new(8);
        let tracker = Arc::new(crate::subagent::TaskTracker::new(
            store.clone(),
            bus.clone(),
        ));
        let liberar = Arc::new(tokio::sync::Notify::new());
        let parent_service = Arc::new(PermissionService::with_responder(Box::new(EsperaDecision(
            liberar.clone(),
        ))));
        let service = Arc::new(PermissionService::with_responder(Box::new(
            ChildResponder::new(
                parent_service,
                Arc::new(InputBroker::default()),
                parent,
                child,
                "obrero".into(),
                bus,
                tracker,
            ),
        )));
        let s = service.clone();
        let handle = tokio::spawn(async move {
            s.check(PermissionRequest {
                tool: "bash".into(),
                action: "run".into(),
                detail: "ls -la".into(),
            })
            .await
        });

        espera_estado(store.as_ref(), parent, child, TaskState::AuthRequired).await;
        liberar.notify_one();
        assert_eq!(handle.await.unwrap(), Decision::Allow);
        espera_estado(store.as_ref(), parent, child, TaskState::Working).await;
    }

    #[tokio::test]
    async fn ask_moves_the_child_to_input_required_and_back() {
        use crate::task::TaskState;
        let dir = tempfile::tempdir().unwrap();
        let (store, parent, child) = almacen_con_hijo(dir.path()).await;
        let bus = EventBus::new(8);
        let tracker = Arc::new(crate::subagent::TaskTracker::new(
            store.clone(),
            bus.clone(),
        ));
        let broker = Arc::new(InputBroker::default());
        let service = Arc::new(PermissionService::with_responder(Box::new(
            ChildResponder::new(
                Arc::new(PermissionService::allow_all()),
                broker.clone(),
                parent,
                child,
                "obrero".into(),
                bus,
                tracker,
            ),
        )));
        let questions = vec![question("¿Cuál?")];
        let s = service.clone();
        let q = questions.clone();
        let handle = tokio::spawn(async move { s.ask_user(&q).await });

        espera_estado(store.as_ref(), parent, child, TaskState::InputRequired).await;
        assert_eq!(
            estado_guardado(store.as_ref(), parent, child).await.message,
            Some("¿Cuál?".into())
        );
        wait_for_pending(&broker, parent).await;

        broker.answer(parent, child, vec!["a".into()]).unwrap();
        assert!(handle.await.unwrap().is_some());
        assert_eq!(
            estado_guardado(store.as_ref(), parent, child).await.state,
            TaskState::Working
        );
    }

    #[tokio::test]
    async fn ask_resolves_input_required_even_if_cancelled() {
        use crate::task::TaskState;
        let dir = tempfile::tempdir().unwrap();
        let (store, parent, child) = almacen_con_hijo(dir.path()).await;
        let bus = EventBus::new(8);
        let tracker = Arc::new(crate::subagent::TaskTracker::new(
            store.clone(),
            bus.clone(),
        ));
        let broker = Arc::new(InputBroker::default());
        let service = Arc::new(PermissionService::with_responder(Box::new(
            ChildResponder::new(
                Arc::new(PermissionService::allow_all()),
                broker.clone(),
                parent,
                child,
                "obrero".into(),
                bus,
                tracker,
            ),
        )));
        let questions = vec![question("¿Cuál?")];
        let s = service.clone();
        let q = questions.clone();
        let handle = tokio::spawn(async move { s.ask_user(&q).await });

        espera_estado(store.as_ref(), parent, child, TaskState::InputRequired).await;
        handle.abort();
        assert!(handle.await.unwrap_err().is_cancelled());
        espera_estado(store.as_ref(), parent, child, TaskState::Working).await;
        assert!(broker.pending(parent).is_empty());
    }

    #[tokio::test]
    async fn decide_moves_the_child_to_auth_required_with_a_summary_and_back() {
        use crate::task::TaskState;
        let dir = tempfile::tempdir().unwrap();
        let (store, parent, child) = almacen_con_hijo(dir.path()).await;
        let bus = EventBus::new(8);
        let tracker = Arc::new(crate::subagent::TaskTracker::new(
            store.clone(),
            bus.clone(),
        ));
        let liberar = Arc::new(tokio::sync::Notify::new());
        let parent_service = Arc::new(PermissionService::with_responder(Box::new(EsperaDecision(
            liberar.clone(),
        ))));
        let service = Arc::new(PermissionService::with_responder(Box::new(
            ChildResponder::new(
                parent_service,
                Arc::new(InputBroker::default()),
                parent,
                child,
                "obrero".into(),
                bus,
                tracker,
            ),
        )));
        let s = service.clone();
        let handle = tokio::spawn(async move {
            s.check(PermissionRequest {
                tool: "bash".into(),
                action: "run".into(),
                detail: "ls -la".into(),
            })
            .await
        });

        espera_estado(store.as_ref(), parent, child, TaskState::AuthRequired).await;
        let resumen = estado_guardado(store.as_ref(), parent, child).await;
        let mensaje = resumen.message.expect("auth-required lleva un resumen");
        assert!(mensaje.contains("bash"), "{mensaje}");

        liberar.notify_one();
        assert_eq!(handle.await.unwrap(), Decision::Allow);
        assert_eq!(
            estado_guardado(store.as_ref(), parent, child).await.state,
            TaskState::Working
        );
    }

    #[tokio::test]
    async fn decide_resolves_auth_required_even_if_cancelled() {
        use crate::task::TaskState;
        let dir = tempfile::tempdir().unwrap();
        let (store, parent, child) = almacen_con_hijo(dir.path()).await;
        let bus = EventBus::new(8);
        let tracker = Arc::new(crate::subagent::TaskTracker::new(
            store.clone(),
            bus.clone(),
        ));
        let liberar = Arc::new(tokio::sync::Notify::new());
        let parent_service = Arc::new(PermissionService::with_responder(Box::new(EsperaDecision(
            liberar,
        ))));
        let service = Arc::new(PermissionService::with_responder(Box::new(
            ChildResponder::new(
                parent_service,
                Arc::new(InputBroker::default()),
                parent,
                child,
                "obrero".into(),
                bus,
                tracker,
            ),
        )));
        let s = service.clone();
        let handle = tokio::spawn(async move {
            s.check(PermissionRequest {
                tool: "bash".into(),
                action: "run".into(),
                detail: "ls".into(),
            })
            .await
        });

        espera_estado(store.as_ref(), parent, child, TaskState::AuthRequired).await;
        handle.abort();
        assert!(handle.await.unwrap_err().is_cancelled());
        espera_estado(store.as_ref(), parent, child, TaskState::Working).await;
    }

    #[test]
    fn auth_summary_names_the_tool_and_trims_the_arguments() {
        let corto = resumen_de_permiso("bash", "run", "ls");
        assert!(corto.contains("bash") && corto.contains("run"), "{corto}");

        let largo = resumen_de_permiso("bash", "run", &"x".repeat(2_000));
        assert!(largo.starts_with("bash"), "{largo}");
        assert!(largo.chars().count() <= 200, "no se recorta: {largo}");
    }
}
