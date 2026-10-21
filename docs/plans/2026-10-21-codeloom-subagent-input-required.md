# `ask_user` en subagentes (`input-required`) — Plan de implementación

> **Para agentes:** sub-skill requerida: `superpowers:subagent-driven-development` (recomendada) o `superpowers:executing-plans`. Los pasos usan casillas `- [ ]`. Una tarea cada vez, en orden (comparten árbol y `target`).

**Meta:** que un subagente pueda llamar a `ask_user`, que la pregunta llegue al LLM del padre como aviso, y que este responda con `subagent_control` (acción `answer`) o escale al usuario con su propio `ask_user`.

**Arquitectura:** un buzón puro `InputBroker` en el core; un `ChildResponder` que da a cada hijo su propio `PermissionService` (permisos delegados en el del padre, preguntas al buzón); un método nuevo `answer` en `SubagentLauncher`; un aviso por el canal existente de avisos; y una marca de espera en la TUI mediante eventos. No cambian `ToolCtx`, `SubagentDescriptor` ni SQLite.

**Stack:** Rust, `tokio` (`oneshot`), `async_trait`.

**Spec:** `docs/specs/2026-10-21-codeloom-subagent-input-required-design.md` (leerla antes). Contexto: `docs/specs/2026-10-15-loom-agent-profile-a2a.md` §5 y §8.

## Restricciones globales

- `codeloom-core` no depende de `ratatui`/`crossterm`, ni de red. Errores de biblioteca con `thiserror` (`CoreError`).
- Los tests no usan red real: proveedor `ScriptedProvider` (ver `subagent.rs` tests) o similares.
- Comprobaciones obligatorias: `cargo build`, `cargo test`, `cargo fmt --all --check`, `cargo clippy -- -D warnings`.
- Entorno de build (Git Bash): `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe' CARGO_TARGET_DIR='E:/workspace/personal/loom/target' CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`. No crear otros `target*` (disco casi lleno). Formatear solo con `rustfmt --edition 2021 <ruta>`.
- Comentarios en español «pijo» (fino y ceremonioso), sin tacos ni andalucismos. No hacer commit.
- La fluidez del composer es prioridad máxima: nada de trabajo costoso en el hilo de dibujado.

## Review Focus

- Cancelar al hijo (`interrupt`, cancelación del padre) con una pregunta pendiente: su `ask_user` debe devolver el error de «sin respuesta» y la entrada debe desaparecer del buzón (sin fugas).
- Responder dos veces, o a un hijo sin pregunta pendiente: error claro, sin pánico.
- Número de respuestas distinto del de preguntas: error que lo diga, sin desbloquear al hijo.
- Los permisos del hijo (`decide`) siguen comportándose igual que antes (mismo servicio y misma caché del padre).
- Un aviso de espera no debe repetirse en cada vaciado.

---

### Tarea 1: buzón `InputBroker` (core, puro)

**Ficheros:** crear `crates/codeloom-core/src/subagent_input.rs`; añadir `pub mod subagent_input;` en `lib.rs` (orden alfabético).

**Interfaces — produce:**
```rust
pub struct PendingInput {
    pub parent: SessionId,
    pub child: SessionId,
    pub label: String,
    pub questions: Vec<Question>,
}
#[derive(Default)]
pub struct InputBroker { /* Mutex<HashMap<SessionId, Entry>> */ }
impl InputBroker {
    pub async fn request(&self, parent: SessionId, child: SessionId, label: String,
        questions: Vec<Question>) -> Option<Vec<Answer>>;   // espera; None si se descarta
    pub fn pending(&self, parent: SessionId) -> Vec<PendingInput>;
    pub fn take_unannounced(&self, parent: SessionId) -> Vec<PendingInput>; // marca como anunciadas
    pub fn answer(&self, parent: SessionId, child: SessionId, answers: Vec<String>)
        -> Result<()>;              // CoreError::Tool si no hay pendiente o el nº no cuadra
    pub fn discard(&self, child: SessionId);                  // descarta sin responder
}
```
Consume: `crate::question::{Question, Answer}`, `crate::session::SessionId`, `crate::error::{CoreError, Result}`.

- [ ] **Paso 1: tests en rojo** (`#[tokio::test]`), con estas aserciones:
  - `request_waits_until_answered`: lanzar `request` en una tarea, comprobar `pending(parent).len() == 1`, llamar `answer(parent, child, vec!["a".into()])` para una pregunta; el `request` devuelve `Some(vec![Answer { question: <texto de la pregunta>, selected: vec!["a".into()] }])` y `pending(parent)` queda vacío.
  - `answer_without_pending_is_an_error`.
  - `answer_with_wrong_count_is_an_error_and_keeps_waiting`: dos preguntas, una respuesta → `Err`, la petición sigue en `pending`.
  - `discard_unblocks_with_none_and_clears_entry`.
  - `dropping_the_request_future_clears_entry` (abortar la tarea que espera; `pending` vacío — implementar con un guard `Drop`).
  - `take_unannounced_returns_each_request_once`.
  - `pending_is_scoped_to_the_parent`.
- [ ] **Paso 2: implementar** con un `std::sync::Mutex` (secciones críticas cortas, sin `.await` dentro) y `tokio::sync::oneshot`. `request` crea la entrada, espera el receptor y limpia la entrada con un guard `Drop` si el futuro se cancela. Una pregunta nueva de un hijo que ya tiene una pendiente sustituye a la anterior (la anterior recibe `None`).
- [ ] **Paso 3: verificar:** `cargo test -p codeloom-core subagent_input::` en verde.

---

### Tarea 2: `ChildResponder` y cableado en el lanzador

**Ficheros:** modificar `crates/codeloom-core/src/subagent.rs`, `crates/codeloom-core/src/event/mod.rs`, `crates/codeloom-core/src/subagent_input.rs` (el responder puede vivir aquí).

**Interfaces — produce:**
```rust
// event/mod.rs
LiveEvent::SubagentInputRequested { parent: SessionId, child: SessionId, label: String },
LiveEvent::SubagentInputResolved  { parent: SessionId, child: SessionId },

// subagent_input.rs
pub struct ChildResponder { /* parent_service: Arc<PermissionService>, broker: Arc<InputBroker>,
                              parent, child, label, bus: EventBus */ }
impl PermissionResponder for ChildResponder {
    async fn decide(&self, req: &PermissionRequest) -> Decision; // = parent_service.check(req.clone()).await
    async fn ask(&self, questions: &[Question]) -> Option<Vec<Answer>>; // publica Requested, broker.request, publica Resolved
}

// subagent.rs
impl InProcessSubagentLauncher { pub fn new(..., bus: EventBus) -> Self }   // añade un argumento al final
```
`InProcessSubagentLauncher` gana un campo `broker: Arc<InputBroker>` (privado) y un `bus`. Consume `PermissionService::{check, with_responder}` y `PermissionRequest` (comprobar que es `Clone`; si no, reconstruirla campo a campo).

- [ ] **Paso 1: tests en rojo**
  - En `subagent_input.rs`: `child_responder_delegates_permissions_to_the_parent` (padre con responder que cuenta llamadas y devuelve `Deny`; el hijo obtiene `Deny` y se cuenta una llamada) y `child_responder_ask_goes_through_the_broker_and_emits_events` (se reciben `SubagentInputRequested` y luego `SubagentInputResolved` en el bus; la respuesta llega por `broker.answer`).
  - En `subagent.rs`: sustituir el test que exige `runtime.tools.get("ask_user").is_none()` por `child_keeps_ask_user_tool`; y añadir `child_that_asks_blocks_until_the_parent_answers`: proveedor guionizado cuyo primer paso invoca `ask_user` con una pregunta de dos opciones y cuyo segundo paso responde texto; lanzar el hijo; esperar (con `tokio::time::timeout`) a que `launcher.pending_inputs(parent)` tenga una entrada; llamar a `launcher.answer(parent, child, vec!["Sí".into()])`; el trabajo termina `Completed` y el historial del hijo contiene el resultado de `ask_user` con «Sí».
- [ ] **Paso 2: implementar.** Quitar `.without("ask_user")`. En `start`, construir por hijo `Arc::new(PermissionService::with_responder(Box::new(ChildResponder{...})))` y pasarlo al `AgentLoop` del hijo en lugar de `self.permission.clone()`. Ajustar todos los llamantes de `InProcessSubagentLauncher::new` (incluido `crates/codeloom-cli/src/main.rs` y los tests) pasando el `EventBus` real.
- [ ] **Paso 3: verificar:** `cargo test -p codeloom-core` y `cargo build` en verde.

---

### Tarea 3: acción `answer` y aviso al padre

**Ficheros:** `crates/codeloom-core/src/subagent.rs` (trait `SubagentLauncher`), `crates/codeloom-core/src/tools/subagent_control.rs`, `crates/codeloom-core/src/agent/mod.rs` (`flush_subagent_notices`).

**Interfaces — produce:** en el trait `SubagentLauncher`, con implementación por defecto para no romper otros implementadores:
```rust
async fn answer(&self, _parent: SessionId, _child: SessionId, _answers: Vec<String>) -> Result<()> {
    Err(CoreError::Tool("subagent answers unavailable".into()))
}
async fn pending_input_notices(&self, _parent: SessionId) -> Result<Vec<Message>> { Ok(Vec::new()) }
```
El esquema de `subagent_control` pasa a `action ∈ {send_message, interrupt, answer}` y gana `answers: array<string>`.

- [ ] **Paso 1: tests en rojo**
  - `subagent_control_answer_requires_answers` (sin `answers` → error) y `subagent_control_answer_calls_the_launcher` (launcher falso que registra la llamada).
  - `pending_input_notices_are_delivered_once`: tras una petición, el primer `pending_input_notices(parent)` devuelve un mensaje de usuario que empieza por `[Subagent <label> needs input]`, contiene el texto de cada pregunta con sus opciones y la indicación `subagent_control action=answer child_id=<n> answers=[…]`; el segundo devuelve vacío.
  - En `agent/mod.rs`: `flush_subagent_notices_injects_input_notices` (el mensaje se guarda en la sesión del padre y se publica como `SessionMessageAdded`).
- [ ] **Paso 2: implementar** `InProcessSubagentLauncher::{answer, pending_input_notices}` sobre `broker.answer` y `broker.take_unannounced`; ampliar `SubagentControlTool` (la acción `answer` llama a `launcher.answer`); en `flush_subagent_notices` añadir el bucle sobre `pending_input_notices`, guardando con `store.append_message` y publicando como el resto de avisos.
- [ ] **Paso 3: verificar:** `cargo test -p codeloom-core`, `cargo clippy -- -D warnings`.

---

### Tarea 4: proyección A2A y marca de espera en la TUI

**Ficheros:** `crates/codeloom-core/src/task.rs`; `crates/codeloom-tui/src/app.rs`, `crates/codeloom-tui/src/rail.rs`.

**Interfaces — produce:**
```rust
// task.rs
pub fn subagent_task_state(status: SubagentStatus, awaiting_input: bool) -> TaskState;
// app.rs
pub awaiting_input: std::collections::HashSet<SessionId>   // hijos en espera
```
`subagent_task_state` devuelve `InputRequired` solo si `status == Running && awaiting_input`; en otro caso, `TaskState::from(status)`.

- [ ] **Paso 1: tests en rojo**
  - `task.rs`: `awaiting_input_only_matters_while_running` (Running+true → `InputRequired`; Running+false → `Working`; Completed+true → `Completed`).
  - `app.rs`: `subagent_input_events_toggle_the_waiting_mark` (tras `SubagentInputRequested` el hijo está en `awaiting_input`; tras `SubagentInputResolved` ya no; un `SubagentChanged` con estado terminal también lo retira).
  - `rail.rs`: `rail_marks_subagents_waiting_for_input` (la línea del subagente en espera lleva el sufijo ` ?`; las demás no).
- [ ] **Paso 2: implementar** el manejo de los dos eventos en `App::apply`, la limpieza en `SubagentChanged` terminal, y el sufijo ` ?` en la línea de SUBAGENTS del rail. Sin trabajo extra en el dibujado (consulta a un `HashSet`).
- [ ] **Paso 3: verificar** suite completa, clippy `-D warnings` y `cargo fmt --all --check`.

---

### Tarea 5: documentación

- [ ] Actualizar `docs/specs/2026-10-20-codeloom-todos-and-questions-design.md`: sustituir la nota «los subagentes no tienen `ask_user`» por el flujo `input-required` (enlace a la spec nueva) y `docs/specs/2026-10-15-loom-agent-profile-a2a.md` §5 (el `input-required` deja de ser solo el aviso dormido: ahora hay buzón). Añadir a la spec nueva una sección «Estado» con lo implementado y las decisiones tomadas al ejecutar.

## Fuera de este plan

- **Despertar al padre ocioso** (decidido: sí, plan aparte, tras la tarea 4): al llegar `SubagentInputRequested` o un estado terminal de un hijo con el padre ocioso, la TUI arranca un turno del padre. Exige leer `crates/codeloom-tui/src/run.rs` antes de planificar.
- Transporte A2A, Agent Card y subagentes remotos.

## Autoauditoría

- Cobertura: flujo 1–4 de la spec → tareas 1–3; estado visible → tarea 4; cancelación/descartes → Review Focus de la tarea 1 y 2; permisos intactos → tarea 2.
- Tipos coherentes: `InputBroker::{request, pending, take_unannounced, answer, discard}`, `ChildResponder`, `SubagentLauncher::{answer, pending_input_notices}`, `LiveEvent::{SubagentInputRequested, SubagentInputResolved}` y `subagent_task_state` se nombran igual en todas las tareas.
- Riesgo conocido: padre ocioso; mientras no exista el despertar automático, el aviso solo se lee en el siguiente turno del padre.
