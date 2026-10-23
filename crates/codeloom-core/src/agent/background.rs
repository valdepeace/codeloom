//! Estado por sesión de la compactación en segundo plano.
//!
//! Un resumen puede prepararse mientras el turno sigue avanzando, y activarse en cuanto haga
//! falta sin llamar al modelo. Este módulo guarda ese estado: si hay una tarea en curso, si hay un
//! resumen listo y de qué posición del historial parte. No llama al proveedor ni toca el almacén:
//! la tarea que genera el resumen lo entrega aquí a través de un `Finisher`.
//!
//! Véase `docs/specs/2026-10-23-codeloom-background-compaction-design.md`, sección «Máquina de
//! estados por sesión» y «Concurrencia».

use crate::agent::compact::{choose_cut, estimate_context_tokens, parse_summary};
use crate::agent::GeneratedSummary;
use crate::error::Result;
use crate::message::{Message, Usage};
use crate::session::SessionId;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

/// Fracción de la ventana del modelo a partir de la cual se lanza el resumen en segundo plano.
/// Queda por debajo de `COMPACT_RATIO`, para que haya tiempo de terminarlo antes de necesitarlo.
pub const BUFFER_RATIO: f64 = 0.60;

/// Segundos máximos que el turno espera a una tarea en curso antes de caer a la vía síncrona.
pub const BACKGROUND_WAIT_SECS: u64 = 60;

/// Resumen listo para activar. `base` es la posición del último resumen persistido cuando se
/// lanzó la tarea; si al activar no coincide con la actual, el resumen ya no es vigente.
#[derive(Debug, Clone)]
pub struct PendingSummary {
    pub base: Option<usize>,
    pub summary: GeneratedSummary,
}

/// Estado observable de la compactación en segundo plano de una sesión.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotState {
    Idle,
    Running,
    Ready,
}

/// Resultado de esperar a la tarea en curso.
#[derive(Debug)]
pub enum WaitOutcome {
    /// La tarea terminó con un resumen; el resumen se entrega y el estado vuelve a `Idle`.
    Ready(PendingSummary),
    /// La tarea terminó sin resumen, o no había ninguna en curso.
    Failed,
    /// Venció el plazo. La tarea se ha cancelado y el estado está en `Idle`.
    TimedOut,
}

/// Entrada de una sesión en el mapa. La ausencia de entrada equivale a `Idle`.
enum Slot {
    /// Tarea en curso. `id` distingue esta tarea de otras posteriores de la misma sesión, y
    /// `done` avisa a quien espera al soltarse el emisor, sea cual sea la causa.
    Running {
        id: u64,
        cancel: CancellationToken,
        done: watch::Sender<()>,
    },
    Ready(PendingSummary),
}

/// Contenido compartido. Va detrás de un `Arc` para que los `Finisher` lo conserven de forma
/// independiente del ciclo de vida de quien los creó.
#[derive(Default)]
struct Inner {
    /// La clave es el `SessionId` en bruto (`i64`): `SessionId` no deriva `Hash`.
    slots: Mutex<HashMap<i64, Slot>>,
    next_id: AtomicU64,
}

impl Inner {
    /// Adquiere el cierre. Un cierre envenenado por un pánico ajeno se reutiliza: el estado es
    /// simple y mantenerlo accesible es preferible a propagar el pánico desde un `Drop`.
    fn map(&self) -> MutexGuard<'_, HashMap<i64, Slot>> {
        self.slots.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Estado de la compactación en segundo plano para todas las sesiones.
///
/// Es un contenedor ligero de `Arc<Inner>`: clonarlo produce otra vista del mismo estado. Así,
/// `try_start` no necesita recibir `&Arc<Self>` y el `Finisher` puede conservar su propia copia.
#[derive(Clone, Default)]
pub struct BackgroundSlots {
    inner: Arc<Inner>,
}

impl BackgroundSlots {
    /// Estado actual de la sesión.
    pub fn state(&self, session: SessionId) -> SlotState {
        match self.inner.map().get(&session.0) {
            None => SlotState::Idle,
            Some(Slot::Running { .. }) => SlotState::Running,
            Some(Slot::Ready(_)) => SlotState::Ready,
        }
    }

    /// Pasa de `Idle` a `Running`. Devuelve el `Finisher` con el que la tarea entrega su resultado,
    /// o `None` si la sesión ya tiene una tarea en curso o un resumen listo.
    pub fn try_start(&self, session: SessionId, base: Option<usize>) -> Option<Finisher> {
        let mut map = self.inner.map();
        if map.contains_key(&session.0) {
            return None;
        }
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let cancel = CancellationToken::new();
        let (done, _) = watch::channel(());
        map.insert(
            session.0,
            Slot::Running {
                id,
                cancel: cancel.clone(),
                done,
            },
        );
        Some(Finisher {
            inner: Arc::clone(&self.inner),
            session,
            id,
            base,
            cancel,
            outcome: None,
        })
    }

    /// Toma el resumen listo si es vigente, es decir, si su `base` coincide con `current_base`.
    /// Si no lo es, lo descarta. En cualquier caso, un resumen listo deja la sesión en `Idle`. Una
    /// tarea en curso no se toca.
    pub fn take_ready(
        &self,
        session: SessionId,
        current_base: Option<usize>,
    ) -> Option<PendingSummary> {
        let mut map = self.inner.map();
        take_ready_locked(&mut map, session).filter(|pending| pending.base == current_base)
    }

    /// Espera hasta `timeout` a la tarea en curso.
    ///
    /// Devuelve `Ready` si termina con resumen (el resumen se entrega y el estado vuelve a `Idle`),
    /// `Failed` si termina sin él o si no había ninguna tarea, y `TimedOut` si vence el plazo. En
    /// ese último caso la tarea se cancela y el estado queda en `Idle`.
    ///
    /// El llamador debe comparar `PendingSummary::base` con la posición actual antes de activarlo.
    /// No se mantiene el cierre durante la espera.
    pub async fn wait(&self, session: SessionId, timeout: Duration) -> WaitOutcome {
        let (id, mut done) = {
            let mut map = self.inner.map();
            match map.get(&session.0) {
                None => return WaitOutcome::Failed,
                Some(Slot::Ready(_)) => {
                    return match take_ready_locked(&mut map, session) {
                        Some(pending) => WaitOutcome::Ready(pending),
                        None => WaitOutcome::Failed,
                    };
                }
                Some(Slot::Running { id, done, .. }) => (*id, done.subscribe()),
            }
        };

        // Espera sin cierre. El emisor se suelta al terminar la tarea o al abortarla, y `changed`
        // devuelve error en ambos casos.
        let vencio = tokio::time::timeout(timeout, async { done.changed().await.ok() })
            .await
            .is_err();

        let mut map = self.inner.map();
        if let Some(pending) = take_ready_locked(&mut map, session) {
            return WaitOutcome::Ready(pending);
        }
        if !vencio {
            return WaitOutcome::Failed;
        }
        // Vencido sin resumen: se aborta, solo si la tarea que se esperaba sigue en curso.
        let cancel = match map.get(&session.0) {
            Some(Slot::Running {
                id: actual, cancel, ..
            }) if *actual == id => Some(cancel.clone()),
            _ => None,
        };
        match cancel {
            Some(cancel) => {
                map.remove(&session.0);
                drop(map);
                cancel.cancel();
                WaitOutcome::TimedOut
            }
            None => WaitOutcome::Failed,
        }
    }

    /// Cancela la tarea en curso y deja la sesión en `Idle`, sin resultado. Un resumen listo
    /// también se descarta.
    pub fn abort(&self, session: SessionId) {
        let removed = self.inner.map().remove(&session.0);
        // La cancelación se hace fuera del cierre.
        if let Some(Slot::Running { cancel, .. }) = removed {
            cancel.cancel();
        }
    }
}

/// Extrae el resumen listo de la sesión, si lo hay, y deja la entrada libre. Una tarea en curso
/// se reinserta sin cambios.
fn take_ready_locked(map: &mut HashMap<i64, Slot>, session: SessionId) -> Option<PendingSummary> {
    match map.remove(&session.0) {
        Some(Slot::Ready(pending)) => Some(pending),
        Some(other) => {
            map.insert(session.0, other);
            None
        }
        None => None,
    }
}

/// Entrega del resultado de una tarea de resumen. Se obtiene con `BackgroundSlots::try_start`.
///
/// Si se llama a `finish`, el resumen queda listo (si lo hay) o la sesión vuelve a `Idle`. Si el
/// `Finisher` se descarta sin llamar a `finish` (por ejemplo, porque la tarea murió), la sesión
/// vuelve a `Idle`. En ambos casos quien espera se despierta. Si la tarea fue abortada entre
/// medias, la entrega es un no-op: no pisa una tarea posterior de la misma sesión.
pub struct Finisher {
    inner: Arc<Inner>,
    session: SessionId,
    id: u64,
    base: Option<usize>,
    cancel: CancellationToken,
    outcome: Option<PendingSummary>,
}

impl Finisher {
    /// Token de cancelación de esta tarea. `BackgroundSlots::abort` lo cancela.
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// Entrega el resultado de la tarea. El consumo de la llamada lo apunta quien llama antes de
    /// entregar; aquí no se usa.
    pub fn finish(mut self, result: Result<(Option<GeneratedSummary>, Option<Usage>)>) {
        match result {
            Ok((Some(summary), _usage)) => {
                self.outcome = Some(PendingSummary {
                    base: self.base,
                    summary,
                });
            }
            Ok((None, _usage)) => {}
            Err(error) => {
                tracing::debug!(%error, "resumen en segundo plano descartado");
            }
        }
        // Al terminar esta función se ejecuta `Drop`, que es quien asienta el estado.
    }
}

impl Drop for Finisher {
    fn drop(&mut self) {
        let mut map = self.inner.map();
        let es_la_nuestra = matches!(
            map.get(&self.session.0),
            Some(Slot::Running { id, .. }) if *id == self.id
        );
        if !es_la_nuestra {
            return;
        }
        // Al reemplazar o quitar la entrada `Running`, su emisor se suelta y despierta a quien espera.
        match self.outcome.take() {
            Some(pending) => {
                map.insert(self.session.0, Slot::Ready(pending));
            }
            None => {
                map.remove(&self.session.0);
            }
        }
    }
}

/// Indica si conviene lanzar el resumen en segundo plano: el contexto estimado alcanza
/// `BUFFER_RATIO` de la ventana y existe un corte posible.
pub fn should_start_background(history: &[Message], system_bytes: usize, window: u64) -> bool {
    let umbral = window as f64 * BUFFER_RATIO;
    estimate_context_tokens(history, system_bytes) as f64 >= umbral && choose_cut(history).is_some()
}

/// Posición del último resumen del historial, que hace de `base` de una tarea en curso. `None` si
/// no hay ningún resumen.
pub fn current_base(history: &[Message]) -> Option<usize> {
    history
        .iter()
        .rposition(|message| parse_summary(message).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::CoreError;

    fn sesion(n: i64) -> SessionId {
        SessionId(n)
    }

    fn resumen(texto: &str) -> GeneratedSummary {
        GeneratedSummary {
            text: texto.to_string(),
            usage: None,
            covers: 2,
        }
    }

    /// Historial de `n` mensajes de usuario de `bytes` bytes cada uno.
    fn mensajes(n: usize, bytes: usize) -> Vec<Message> {
        (0..n).map(|_| Message::user("x".repeat(bytes))).collect()
    }

    #[test]
    fn try_start_only_succeeds_from_idle() {
        let slots = BackgroundSlots::default();
        let s = sesion(1);

        let primera = slots.try_start(s, None).expect("desde Idle debe arrancar");
        assert_eq!(slots.state(s), SlotState::Running);
        assert!(
            slots.try_start(s, None).is_none(),
            "ya hay una tarea en curso"
        );

        primera.finish(Ok((None, None)));
        assert_eq!(slots.state(s), SlotState::Idle);
        assert!(
            slots.try_start(s, None).is_some(),
            "vuelto a Idle, debe arrancar"
        );

        // Con un resumen listo tampoco se arranca otra.
        let t = sesion(2);
        let finisher = slots.try_start(t, Some(0)).expect("sesión libre");
        finisher.finish(Ok((Some(resumen("r")), None)));
        assert_eq!(slots.state(t), SlotState::Ready);
        assert!(
            slots.try_start(t, None).is_none(),
            "con resumen listo no se arranca"
        );
    }

    #[test]
    fn finish_ok_makes_the_slot_ready() {
        let slots = BackgroundSlots::default();
        let s = sesion(1);
        let finisher = slots.try_start(s, Some(4)).expect("arranca");

        finisher.finish(Ok((Some(resumen("texto")), None)));

        assert_eq!(slots.state(s), SlotState::Ready);
    }

    #[test]
    fn finish_with_error_or_none_returns_to_idle() {
        let slots = BackgroundSlots::default();
        let s = sesion(1);

        let con_error = slots.try_start(s, None).expect("arranca");
        con_error.finish(Err(CoreError::Provider("caído".into())));
        assert_eq!(slots.state(s), SlotState::Idle);

        let sin_resumen = slots.try_start(s, None).expect("arranca");
        sin_resumen.finish(Ok((None, None)));
        assert_eq!(slots.state(s), SlotState::Idle);
    }

    #[test]
    fn take_ready_discards_a_stale_summary() {
        let slots = BackgroundSlots::default();
        let s = sesion(1);
        let finisher = slots.try_start(s, Some(3)).expect("arranca");
        finisher.finish(Ok((Some(resumen("viejo")), None)));

        assert!(
            slots.take_ready(s, Some(5)).is_none(),
            "base distinta: no vigente"
        );
        assert_eq!(slots.state(s), SlotState::Idle, "se descarta y queda Idle");
    }

    #[test]
    fn take_ready_returns_it_once() {
        let slots = BackgroundSlots::default();
        let s = sesion(1);
        let finisher = slots.try_start(s, Some(3)).expect("arranca");
        finisher.finish(Ok((Some(resumen("vigente")), None)));

        let primera = slots
            .take_ready(s, Some(3))
            .expect("vigente, debe entregarse");
        assert_eq!(primera.base, Some(3));
        assert_eq!(primera.summary.text, "vigente");
        assert!(slots.take_ready(s, Some(3)).is_none(), "ya entregado");
    }

    #[tokio::test(start_paused = true)]
    async fn wait_returns_ready_when_the_task_finishes() {
        let slots = BackgroundSlots::default();
        let s = sesion(1);
        let finisher = slots.try_start(s, Some(4)).expect("arranca");
        let (liberar, esperar) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            // La tarea solo termina cuando el test lo indica.
            let _ = esperar.await;
            finisher.finish(Ok((Some(resumen("listo")), None)));
        });

        let espera = {
            let slots = slots.clone();
            tokio::spawn(async move {
                slots
                    .wait(s, Duration::from_secs(BACKGROUND_WAIT_SECS))
                    .await
            })
        };
        // Cede el turno para que la espera se registre antes de terminar la tarea.
        tokio::task::yield_now().await;
        liberar.send(()).expect("la tarea sigue esperando");

        match espera.await.expect("la espera termina") {
            WaitOutcome::Ready(pending) => {
                assert_eq!(pending.base, Some(4));
                assert_eq!(pending.summary.text, "listo");
            }
            otro => panic!("se esperaba Ready, llegó {otro:?}"),
        }
        assert_eq!(slots.state(s), SlotState::Idle, "el resumen se entregó");
    }

    #[tokio::test(start_paused = true)]
    async fn wait_times_out_and_leaves_the_slot_aborted() {
        let slots = BackgroundSlots::default();
        let s = sesion(1);
        // El `Finisher` se conserva sin terminar: la tarea no acaba nunca.
        let finisher = slots.try_start(s, None).expect("arranca");
        let token = finisher.cancel_token();

        let resultado = slots
            .wait(s, Duration::from_secs(BACKGROUND_WAIT_SECS))
            .await;

        assert!(matches!(resultado, WaitOutcome::TimedOut));
        assert_eq!(slots.state(s), SlotState::Idle);
        assert!(token.is_cancelled(), "la tarea vencida se cancela");
        drop(finisher);
    }

    #[test]
    fn dropped_finisher_returns_to_idle() {
        let slots = BackgroundSlots::default();
        let s = sesion(1);
        let finisher = slots.try_start(s, None).expect("arranca");
        assert_eq!(slots.state(s), SlotState::Running);

        drop(finisher);

        assert_eq!(slots.state(s), SlotState::Idle);
    }

    #[test]
    fn slots_are_independent_per_session() {
        let slots = BackgroundSlots::default();
        let a = sesion(1);
        let b = sesion(2);
        let fa = slots.try_start(a, None).expect("A arranca");
        let fb = slots.try_start(b, None).expect("B arranca");

        fa.finish(Ok((Some(resumen("a")), None)));
        assert_eq!(slots.state(a), SlotState::Ready);
        assert_eq!(slots.state(b), SlotState::Running, "B no se entera de A");

        slots.abort(b);
        assert_eq!(slots.state(b), SlotState::Idle);
        assert_eq!(slots.state(a), SlotState::Ready, "abortar B no toca A");
        drop(fb);
    }

    #[test]
    fn should_start_background_needs_ratio_and_a_cut() {
        // Ventana de 100 000 tokens: el 60 % son 60 000. Cincuenta mensajes de 4000 bytes son
        // 50 000 tokens, con corte posible (la cola ocupa unos 10 000).
        let ventana = 100_000;
        let historia = mensajes(50, 4000);

        // 59 %: 9 000 de prompt de sistema (36 000 bytes) más 50 000 de historia.
        assert!(!should_start_background(&historia, 36_000, ventana));
        // 60 %: 10 000 de prompt de sistema (40 000 bytes) más 50 000 de historia.
        assert!(should_start_background(&historia, 40_000, ventana));

        // 60 % sin corte posible: dos mensajes de 2 000 tokens en total, todos dentro de la cola.
        let corta = mensajes(2, 4000);
        assert!(!should_start_background(&corta, 232_000, ventana));
    }
}
