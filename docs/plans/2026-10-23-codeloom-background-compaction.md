# Compactación anticipada en segundo plano: plan de implementación

> Para quien ejecute: una tarea cada vez, con verificación y commit al terminar cada una.

**Meta:** que el resumen del historial se prepare en segundo plano y se active al instante, sin
detener el turno.

**Spec:** `docs/specs/2026-10-23-codeloom-background-compaction-design.md` (léase primero), y
`2026-10-23-codeloom-context-summary-design.md` para lo que ya existe.

## Restricciones globales

Las de `2026-10-23-codeloom-context-summary.md` (entorno de build, LF, `rustfmt --edition 2021
<ruta>` sobre ficheros concretos, no tocar `codeloom.toml`, `git add` de rutas concretas,
comentarios en español formal, `codeloom-core` sin `ratatui`/`crossterm`, sin red real en tests,
clippy sin `--all-targets`). **Fluidez del composer = prioridad máxima:** nada de lo que añade
esta función bloquea el hilo de la interfaz ni el bucle del turno más allá del tope de espera.
Los tests con concurrencia **no usan `sleep` ni tiempos reales**: controlan al proveedor simulado
con `tokio::sync::{Notify, oneshot}`.

---

### Tarea 1: separar «generar» de «activar» y arreglar el consumo en el medidor

**Ficheros:** `crates/codeloom-core/src/agent/mod.rs`, `crates/codeloom-core/src/event/mod.rs`,
`crates/codeloom-tui/src/app.rs` (solo el brazo nuevo).

Sin cambio de comportamiento observable, salvo el evento de consumo.

**Produce:**
- Función libre en `agent/compact.rs` o `agent/mod.rs` (donde quede más natural):

```rust
pub struct GeneratedSummary { pub text: String, pub usage: Option<Usage>, pub covers: usize }
/// Pide el resumen del tramo antiguo. No persiste nada ni publica eventos.
pub async fn generate_summary(
    provider: Arc<dyn Provider>,
    model: String,
    history: &[Message],
    cancel: &CancellationToken,
) -> Result<Option<GeneratedSummary>>;
```
  Es el cuerpo actual de `AgentLoop::compact` hasta tener el texto recortado (`choose_cut`,
  limpieza, petición sin herramientas, flujo con descarte si acaba en error o sin `Done`,
  `limit_bytes` con el hueco del marcador). `Ok(None)` cuando no hay corte, hay cancelación,
  el flujo no cierra bien o el texto queda vacío.
- `AgentLoop::compact` pasa a usar `generate_summary` y a hacer solo: apunte del consumo,
  persistencia, `MessageAdded` y `ContextCompacted`. **Todos los tests actuales de `compact`
  siguen pasando sin tocarlos.**
- `LiveEvent::CompactionUsage { input_tokens: u32, output_tokens: u32 }`: el consumo de la llamada
  de resumen se publica con él y **no** con `LiveEvent::Usage`. Tratado como `Usage` en los
  `match` de `event/mod.rs` (`is_child_relevant` igual que `Usage`). En `app.rs`, su brazo suma a
  los contadores (`input_tokens`, `output_tokens`, `turn_*`, `record_live_usage`) **igual que
  `Usage`** pero **no** toca `context_tokens`.

**Tests:** `generate_summary_does_not_touch_the_store_or_the_bus`;
`compact_usage_goes_through_compaction_usage_not_usage` (el bus recibe `CompactionUsage` y ningún
`Usage`); en `app.rs`, `compaction_usage_counts_but_does_not_move_the_meter`. El test existente
`compact_records_the_usage_of_the_summary_call_even_if_it_is_discarded` se adapta **solo** en el
tipo de evento esperado, con `// ADAPTADO: el consumo del resumen usa CompactionUsage`.

**Commit:** `refactor(core): separar la generación del resumen de su activación`.

---

### Tarea 2: estado del resumen en segundo plano

**Ficheros:** crear `crates/codeloom-core/src/agent/background.rs`; `pub mod background;` en
`agent/mod.rs`.

**Produce (firmas exactas):**

```rust
pub const BUFFER_RATIO: f64 = 0.60;
pub const BACKGROUND_WAIT_SECS: u64 = 60;

pub struct PendingSummary { pub base: Option<usize>, pub summary: GeneratedSummary }

pub enum SlotState { Idle, Running, Ready }

/// Estado por sesión de la compactación en segundo plano.
#[derive(Default)]
pub struct BackgroundSlots { /* Mutex<HashMap<SessionId, Slot>> */ }
impl BackgroundSlots {
    pub fn state(&self, session: SessionId) -> SlotState;
    /// `Idle` → `Running`. Devuelve un `oneshot::Sender`-like `Finisher` que la tarea usa para
    /// entregar su resultado, o `None` si ya hay una tarea o un resumen listo.
    pub fn try_start(&self, session: SessionId, base: Option<usize>) -> Option<Finisher>;
    /// Toma el resumen listo si es vigente (`base` coincide); si no, lo descarta. Deja `Idle`.
    pub fn take_ready(&self, session: SessionId, current_base: Option<usize>) -> Option<PendingSummary>;
    /// Espera a la tarea en curso hasta `timeout`. `Ready(..)`, `Failed` o `TimedOut`.
    pub async fn wait(&self, session: SessionId, timeout: Duration) -> WaitOutcome;
    /// Cancela la tarea en curso y deja `Idle` (sin resultado).
    pub fn abort(&self, session: SessionId);
}
pub fn should_start_background(history: &[Message], system_bytes: usize, window: u64) -> bool;
```

`should_start_background`: `estimate_context_tokens >= window * BUFFER_RATIO` **y**
`choose_cut(history).is_some()`. `Finisher::finish(Result<Option<GeneratedSummary>>)` pasa el slot
a `Ready` (con `base`) o a `Idle` si fue error o `None`. Un `Finisher` descartado sin `finish`
(la tarea murió) deja el slot en `Idle`.

**Tests** (deterministas; `wait` con `tokio::time::pause()` si hace falta, sin `sleep` real):
`try_start_only_succeeds_from_idle`; `finish_ok_makes_the_slot_ready`;
`finish_with_error_or_none_returns_to_idle`; `take_ready_discards_a_stale_summary`
(base distinta); `take_ready_returns_it_once`; `wait_returns_ready_when_the_task_finishes`;
`wait_times_out_and_leaves_the_slot_aborted`; `dropped_finisher_returns_to_idle`;
`slots_are_independent_per_session`; `should_start_background_needs_ratio_and_a_cut`
(59 % no, 60 % sí; 60 % sin corte posible no).

**Commit:** `feat(core): estado por sesión de la compactación en segundo plano`.

---

### Tarea 3: integrarlo en el bucle y la configuración

**Ficheros:** `crates/codeloom-core/src/agent/mod.rs`, `crates/codeloom-core/src/agent/compact.rs`
(config), `crates/codeloom-core/src/event/mod.rs`, `crates/codeloom-cli/src/main.rs`.

**Produce:**
- `CompactionConfig { pub background: bool }` con `from_toml`/`load`/`Default` (`false`), sección
  `[compaction]`, mismo patrón que `BudgetConfig`; sección mal escrita → error y no arranca.
  `AgentLoop::with_compaction(CompactionConfig)`; en `main.rs` se lee junto a `[budget]`.
- `LiveEvent::BackgroundCompactionStarted { tokens_before: u64 }`,
  `LiveEvent::BackgroundCompactionReady { summary_bytes: usize }` y `ContextCompacted` con el
  campo nuevo `activation: Activation` (`Sync`, `Background`, `BackgroundAwaited { waited_ms: u64 }`).
  Adaptar los usos existentes de `ContextCompacted` (`Activation::Sync`) y marcarlos
  `// ADAPTADO: campo nuevo`.
- En `run_turn`, con `background == true`: **(a)** si `should_start_background`, `try_start` y
  `tokio::spawn` de una tarea que llama a `generate_summary` con **copias** (`Arc`) del proveedor,
  el modelo y un historial clonado, entrega con el `Finisher`, apunta su consumo
  (`CompactionUsage` + `record_usage`) y publica `BackgroundCompactionReady`; la tarea **no** lleva
  el token de cancelación del turno (cancelar el turno no mata el resumen) sino uno propio que
  `abort` cancela. **(b)** al llegar a `COMPACT_RATIO`: `take_ready` → si hay, `activate`
  (persistir + eventos, `Activation::Background`, sin llamada); si el estado es `Running`, `wait`
  con `BACKGROUND_WAIT_SECS` y, si da `Ready`, activar con `BackgroundAwaited`; si `TimedOut` o
  `Failed`, `abort` y la vía síncrona de hoy. Con `Idle`, la vía síncrona de hoy.
- `activate(session, pending, tokens_before)`: persiste `summary_message(covers, text)`, publica
  `MessageAdded` y `ContextCompacted`. Es la misma parte final que ya usa `compact`: reutilizarla.
- Con `background == false` el comportamiento es **idéntico** al actual.

**Tests** (módulo de pruebas de `agent/mod.rs`; un proveedor simulado controlable con
`Notify`/`oneshot` que distingue la petición de resumen como `PlanProvider`):
`background_summary_starts_at_the_buffer_ratio_without_blocking_the_turn` (el turno avanza
mientras la tarea sigue pendiente);
`a_ready_summary_is_activated_without_a_model_call_at_the_compact_ratio` (una sola petición de
resumen en total);
`a_running_summary_is_awaited_instead_of_starting_a_second_one`;
`a_timed_out_summary_falls_back_to_the_synchronous_path` (con `tokio::time::pause()`);
`a_stale_summary_is_discarded_when_another_was_persisted`;
`background_off_behaves_exactly_as_before` (ninguna petición de resumen antes del 80 %);
`cancelling_the_turn_does_not_kill_the_background_summary`;
`the_background_call_usage_is_recorded_and_counts_for_the_budget`;
`compaction_section_parses_and_rejects_garbage`.

**Commit:** `feat(core): compactación anticipada en segundo plano`.

---

### Tarea 4: mostrarlo

**Ficheros:** `crates/batten/src/widgets/context_meter.rs`, `crates/codeloom-tui/src/app.rs`,
`crates/codeloom-tui/src/render.rs`, demo del showcase (`demos/context_meter.rs`).

- `ContextMeter` gana `pub ready: bool` (por defecto `false`; `with_ready`). Con `ready`, el texto
  del `Segments` añade ` ↓` al final (flecha de mastra para «ahorro pendiente») y `meter_width`
  lo cuenta. Sin `ready`, la salida no cambia.
- `App`: `background_summary: BackgroundSummary { None, Running{since}, Ready }`. Started →
  `Running`; Ready → `Ready`; `ContextCompacted` → `None`. Mientras `Running`, la línea de
  estado muestra `resumiendo… {N}s` con el mismo mecanismo que otras fases (`set_phase` o el
  campo de estado existente; mira cómo se pinta «working»). Al pasar a `Ready`, el medidor lleva
  `ready: true`.
- El demo del showcase añade a la fila «ciclo» el estado «resumiendo…» y la flecha antes de caer.

**Tests:** en batten, `ready_adds_the_arrow_and_counts_in_the_width` y
`not_ready_keeps_the_previous_text`; en la app, `background_events_drive_the_status_and_the_arrow`
y `compacted_clears_the_ready_mark`.

**Commit:** `feat(tui): estado visible de la compactación en segundo plano`.

---

### Tarea 5: documentación

«Estado» de la spec, regla 5 del decálogo (compactación anticipada: hecha, opt-in) y la idea 2 de
`memoria-observacional-mastra.md` (marcarla como adoptada, con su diferencia de mecanismo).
Prosa en español formal. Nada se da por visto en terminal real.

**Commit:** `docs: estado real de la compactación en segundo plano`.
