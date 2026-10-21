# Despertar al padre ocioso (Fase A de autonomía) — Plan de implementación

> **Para agentes:** sub-skill requerida: `superpowers:subagent-driven-development` (recomendada) o `superpowers:executing-plans`. Los pasos usan casillas `- [ ]`. Una tarea cada vez, en orden (comparten árbol y `target`). Al terminar cada tarea y verificarla, se commitea (regla del usuario).

**Meta:** que cuando un subagente pregunta (`input-required`) o termina y el padre está ocioso, la aplicación arranque sola un turno del padre para que lea el aviso, con un tope de turnos automáticos por cada mensaje del usuario.

**Arquitectura:** el despertar es una orden `wake` distinta de un prompt: el trabajador (en `codeloom-cli`) llama a `AgentLoop::run_woken`, que vacía los avisos pendientes y **solo si había alguno** ejecuta un turno, sin inventar mensaje de usuario. Si no había nada publica `LiveEvent::WakeIdle`. El lanzador publica un evento nuevo, `SubagentSettled`, **después** de persistir el estado final del hijo (así no hay carrera con `deliver_notices`). La TUI decide cuándo pedir el despertar (estado ocioso, sin diálogos, con presupuesto).

**Stack:** Rust, `tokio`.

**Spec:** `docs/specs/2026-10-21-codeloom-agent-autonomy-design.md` (Fase A); contexto en `2026-10-21-codeloom-subagent-input-required-design.md`.

## Hallazgos del código que fijan el diseño

- Todo turno añade un mensaje de usuario (`run_turn` hace `append_message(user_prompt)`). Un despertar **no** debe fabricar uno: ensuciaría el transcript y la historia. Por eso `run_turn` pasa a aceptar `Option<String>`.
- La TUI traduce `JobChanged` a estado de subagente por su cuenta (`app.rs`), y el lanzador persiste el estado final en una tarea aparte: si la TUI despertase al padre al ver el job terminado, `deliver_notices` podría no ver aún el estado `Completed` y el aviso se perdería. De ahí el evento `SubagentSettled` publicado tras persistir.
- `ChildResponder::ask` publica `SubagentInputRequested` **antes** de registrar la petición en el buzón: mismo riesgo de carrera. Se corrige con un callback `on_registered` en el buzón.
- `turn_in_flight` solo lo activa `TurnStarted`; el hueco entre pedir el despertar y que arranque el turno debe cubrirlo un indicador propio (`wake_in_flight`).

## Restricciones globales

- `codeloom-core` no depende de `ratatui`/`crossterm`. Sin red en tests (proveedor guionizado).
- Comprobaciones obligatorias: `cargo build`, `cargo test`, `cargo fmt --all --check`, `cargo clippy -- -D warnings`.
- Entorno de build (Git Bash): `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe' CARGO_TARGET_DIR='E:/workspace/personal/loom/target' CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`. No crear otros `target*`. Formatear solo con `rustfmt --edition 2021 <ruta>`. Ficheros en LF.
- Comentarios en español «pijo», sin tacos ni andalucismos.
- La fluidez del composer es prioridad máxima: el despertar no puede añadir trabajo al hilo de dibujado ni bloquear el bucle.

## Review Focus

- Dos eventos seguidos del mismo hijo (p. ej. `Completed` publicado dos veces) no deben gastar dos turnos: si no hay avisos, `run_woken` no ejecuta turno y no consume presupuesto.
- Un turno despertado por un hijo no debe disparar otro despertar por sus propios avisos ya entregados (sin bucle infinito); el presupuesto lo acota en cualquier caso.
- Un mensaje que el usuario tenga en cola o un diálogo abierto tienen prioridad sobre el despertar.
- Esc debe cancelar un turno despertado igual que uno normal, y cancelar no debe rearmar el despertar por el mismo evento.
- Un hijo `Killed` por el usuario no despierta al padre.
- `WakeIdle` siempre limpia `wake_in_flight` (si no, la TUI se queda sin despertares para siempre).

---

### Tarea 1: núcleo — `run_woken`, `WakeIdle`, `SubagentSettled` y callback del buzón

**Ficheros:** `crates/codeloom-core/src/agent/mod.rs`, `event/mod.rs`, `subagent.rs`, `subagent_input.rs`; llamantes de `LiveEvent` con `match` exhaustivo (p. ej. `crates/codeloom-tui/src/app.rs`: brazo mínimo que ignore los eventos nuevos hasta la tarea 3).

**Interfaces — produce:**
```rust
// event/mod.rs
LiveEvent::WakeIdle,
LiveEvent::SubagentSettled { parent: SessionId, child: SessionId, status: SubagentStatus },

// agent/mod.rs
pub async fn run_woken(&self, session: SessionId, cancel: CancellationToken)
    -> Result<Option<TurnOutcome>>;           // None si no había avisos (publica WakeIdle)
async fn flush_subagent_notices(&self, session: SessionId) -> Result<usize>; // nº de avisos entregados
async fn run_turn(&self, session, user_prompt: Option<String>, cancel) -> Result<TurnOutcome>;

// subagent_input.rs
pub async fn request_with(&self, parent, child, label, questions,
    on_registered: impl FnOnce()) -> Option<Vec<Answer>>;  // request() delega aquí con |_| {}
```
`run` conserva su firma y llama a `run_turn(Some(prompt))`. En `run_woken`, `TurnStarted` y `TurnEnded` se publican solo si hay turno; el mensaje de usuario solo se añade si `user_prompt` es `Some`.

- [ ] **Paso 1: tests en rojo** (comprobar que fallan por aserción o compilación e indicarlo):
  - `agent/mod.rs`: `run_woken_without_notices_runs_no_turn_and_emits_wake_idle` (`Ok(None)`, ningún `TurnStarted`, un `WakeIdle`, el proveedor guionizado no se consulta); `run_woken_with_a_notice_runs_a_turn_without_inventing_a_user_message` (con un lanzador falso que entrega un aviso, el historial no contiene ningún mensaje de usuario que no sea el aviso, y el turno publica `TurnStarted` y `TurnEnded`).
  - `subagent_input.rs`: `request_with_calls_the_callback_after_registration` (dentro del callback, `pending(parent)` ya contiene la petición); `child_responder_publishes_requested_after_registration` (al recibir `SubagentInputRequested`, `broker.pending(parent)` ya tiene la petición).
  - `subagent.rs`: `settled_event_is_published_after_the_status_is_persisted` (al recibir `SubagentSettled`, `store.list_subagents(parent)` ya muestra el estado final y `deliver_notices(parent)` devuelve el aviso de un hijo `Completed`).
- [ ] **Paso 2: implementar.** En el lanzador, la tarea que persiste el estado final (`update_subagent_status`) publica `SubagentSettled` con el estado **después** de persistir (incluido `Killed`; la TUI decide qué hacer con cada uno). `ChildResponder::ask` publica `SubagentInputRequested` desde el callback `on_registered`.
- [ ] **Paso 3: verificar** `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --all --check`.
- [ ] **Paso 4: commit** en español pijo con el trailer de coautoría.

---

### Tarea 2: trabajador — orden `wake` desde la TUI

**Ficheros:** `crates/codeloom-cli/src/main.rs`, `crates/codeloom-tui/src/run.rs` (firma de `run`), `crates/codeloom-tui/src/lib.rs` si reexporta tipos.

**Interfaces — produce:** un canal nuevo `wake_tx: UnboundedSender<()>` pasado a `codeloom_tui::run::run` (un argumento más, justo después de `prompt_tx`). En el trabajador, una rama `wake_rx.recv()` del `select!` que, igual que la de los prompts, fija `worker_active_turn` con un `CancellationToken` y llama a `agent_task.run_woken(session_id, cancel)`; el informe a herdr (`Working`/`Idle`) solo se emite si se ejecutó turno (`Some`).

- [ ] **Paso 1: test en rojo** (donde sea viable sin red): `wake_while_no_session_is_ignored` o equivalente en el módulo de pruebas del trabajador si existe; si el trabajador no es comprobable por separado, dejar constancia en el informe y cubrirlo con la prueba e2e de `crates/codeloom-cli/tests` si la hay.
- [ ] **Paso 2: implementar** y actualizar todos los llamantes de `run(...)` (incluidos tests y ejemplos que lo construyan).
- [ ] **Paso 3: verificar** suite completa, clippy y fmt. **Paso 4: commit.**

---

### Tarea 3: TUI — decidir cuándo despertar y mostrarlo

**Ficheros:** `crates/codeloom-tui/src/app.rs`, `crates/codeloom-tui/src/run.rs`, `crates/codeloom-tui/src/render.rs` (pie/estado, solo texto).

**Interfaces — produce (en `App`):**
```rust
pub const MAX_AUTO_TURNS: u32 = 8;
pub wake_pending: bool,
pub wake_in_flight: bool,
pub auto_turns_left: u32,            // se rearma a MAX_AUTO_TURNS con cada prompt del usuario
pub fn take_wake(&mut self) -> bool; // true si debe pedirse un despertar ahora
```
`take_wake` devuelve `true` (y pone `wake_in_flight = true`, `wake_pending = false`) solo si: `wake_pending && !wake_in_flight && !turn_in_flight && pending_prompts.is_empty() && permission.is_none() && question.is_none() && !queue_open && !queue_paused && !session_loading && !session_picker_open && !settings_open && !settings_saving && auto_turns_left > 0`. Si `wake_pending` y `auto_turns_left == 0`, deja `status = "auto-turns exhausted — write to continue"` una sola vez y limpia `wake_pending`.

Reglas de `apply`: `SubagentInputRequested` (padre = sesión abierta) → `wake_pending = true`. `SubagentSettled` (padre = sesión abierta) con estado `Completed` o `Failed` → `wake_pending = true` (con `Killed`/`Interrupted`, no). `TurnStarted` con `wake_in_flight` → `auto_turns_left -= 1`, `wake_in_flight = false`, `status = "auto-turn"`. `WakeIdle` → `wake_in_flight = false`. `TurnEnded`/`Error` → `wake_in_flight = false`. En `run.rs`, `send` con prompt del usuario rearma `auto_turns_left`; tras cada rama que hoy llama a `dispatch_next` se llama a `dispatch_wake(&mut app, &wake_tx)`, que hace `if app.take_wake() { let _ = wake_tx.send(()); }`. Cancelar el turno (Esc) limpia `wake_pending`.

- [ ] **Paso 1: tests en rojo** (puros sobre `App`, sin terminal): `wake_requested_on_child_question`, `wake_requested_on_completed_or_failed_but_not_killed`, `no_wake_while_turn_in_flight_or_dialog_open_or_prompt_queued`, `wake_spends_budget_only_when_a_turn_starts`, `wake_idle_clears_in_flight_and_spends_nothing`, `budget_exhausted_stops_waking_until_user_writes` (se rearma al llamar a la rutina de envío con un prompt), `wake_ignores_other_sessions`, `cancel_clears_pending_wake`.
- [ ] **Paso 2: implementar.** Nada de trabajo en el dibujado: el texto de estado ya existe (`app.status`).
- [ ] **Paso 3: verificar** suite completa, clippy, fmt. **Paso 4: commit.**

---

### Tarea 4: documentación

- [ ] Actualizar `docs/specs/2026-10-21-codeloom-agent-autonomy-design.md` (Fase A: implementada, decisiones: despertar sin mensaje de usuario, `SubagentSettled`, presupuesto 8 por mensaje del usuario, `Killed` no despierta) y la sección «Estado» de `2026-10-21-codeloom-subagent-input-required-design.md` (el padre ocioso ya despierta). Anotar como pendiente la validación visual. **Commit.**

## Autoauditoría

- Cobertura de la Fase A: reacción (T1+T3), presupuesto (T3), no arrancar con diálogos (T3), marca de turno automático (T3, `status`), sin bucles (Review Focus).
- Tipos coherentes: `run_woken`, `WakeIdle`, `SubagentSettled`, `request_with`, `wake_tx`, `take_wake`, `wake_pending`, `wake_in_flight`, `auto_turns_left` se nombran igual en todas las tareas.
- Riesgo no resuelto: validación en terminal real (el despertar interactúa con el scheduler de frames y el composer); anotado como pendiente.
