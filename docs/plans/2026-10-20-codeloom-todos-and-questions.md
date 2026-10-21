# `todo_write` y `ask_user` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Dos herramientas del core, `todo_write` (lista de tareas viva) y `ask_user` (preguntas con opciones), con su presentación en la TUI.

**Architecture:** Tipos y herramientas en `codeloom-core` (`todo`, `question`, `tools::{todo_write, ask_user}`); las preguntas viajan por `PermissionResponder::ask` (sin tocar `ToolCtx`). `batten` aporta `todo_lines` y una máquina de diálogo pura `QuestionDialog` más su widget. `codeloom-tui` conecta eventos, canal de preguntas, cola y render.

**Tech Stack:** Rust 2021, tokio, async-trait, serde_json, ratatui.

**Spec:** `docs/specs/2026-10-20-codeloom-todos-and-questions-design.md`

## Global Constraints

- Tras cada tarea: `cargo build`, `cargo test` (workspace), `cargo clippy -- -D warnings` sin avisos nuevos, `rustfmt --edition 2021 --check` sobre los ficheros tocados (nunca `cargo fmt --all`).
- Entorno (Git Bash): `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe' CARGO_TARGET_DIR='E:/workspace/personal/loom/target' CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`. `E:` está casi lleno: **no crear otros directorios `target*`**.
- **No hacer commits ni `git add`.** Prohibido `git checkout/restore/stash/reset`. El árbol tiene cambios sin commitear de entregas anteriores: no tocar ficheros fuera de los de la tarea.
- `codeloom-core` no depende de `ratatui`/`crossterm`; `batten` no depende de `codeloom-core`. Sin red en tests.
- El look Classic y los tests existentes no cambian (no se modifican tests previos; si uno rompe, arreglar el código propio y reportar).
- Mensajes de herramientas dirigidos al modelo, en inglés (como las demás herramientas). Comentarios de código en español de registro cuidado, sin andalucismos ni tacos.

## Review Focus

1. `todo_write` con lista vacía, con 51 elementos, con estado desconocido o contenido en blanco (C1: `parse_*`).
2. `ask_user` con 0 o 5 preguntas, con 1 o 7 opciones, con pregunta vacía; con el usuario ausente (`None`) y con cancelación del turno mientras espera (C2).
3. Diálogo: respuesta «Other…» vacía no avanza; flecha derecha no salta una pregunta sin responder; Esc estando en edición solo sale de la edición (B2).
4. Rail y transcript con listas de tareas largas: se recortan (B1, T1).
5. Un hijo no ve `ask_user`, y las tareas de un hijo no aparecen en la lista del padre (C2, T1).

---

### Task C1: Core: `todo`, `LiveEvent::TodosChanged` y `todo_write`

**Files:**
- Create: `crates/codeloom-core/src/todo.rs`, `crates/codeloom-core/src/tools/todo_write.rs`
- Modify: `crates/codeloom-core/src/lib.rs` (`pub mod todo;`), `crates/codeloom-core/src/tools/mod.rs` (`pub mod todo_write;`), `crates/codeloom-core/src/event/mod.rs`, `crates/codeloom-cli/src/main.rs` (registro), `crates/codeloom-tui/src/app.rs` (SOLO un brazo vacío en el `match` de `apply` para que compile)

**Interfaces:**
- Produces: `codeloom_core::todo::{TodoStatus {Pending, InProgress, Completed}, TodoItem {content: String, status: TodoStatus}, MAX_TODOS: usize = 50, parse_todos(&serde_json::Value) -> Result<Vec<TodoItem>, String>}` (`TodoStatus`: `Debug, Clone, Copy, PartialEq, Eq`; `TodoItem`: `Debug, Clone, PartialEq, Eq`); `LiveEvent::TodosChanged { session: SessionId, items: Vec<TodoItem> }`; `tools::todo_write::TodoWriteTool` (nombre `todo_write`, `ExecutionMode::Exclusive`).
- Entrada: `{"todos":[{"content":"…","status":"pending|in_progress|completed"}]}`. Errores de `parse_todos` (textos exactos): `"todos must be an array"`, `"too many todos (max 50)"`, `"todo {n}: empty content"`, `"todo {n}: invalid status '{s}'"` (`n` base 1). El contenido se recorta con `trim`. Lista vacía válida.
- `run`: `parse_todos` → error: `ToolOutput::err(msg)` sin publicar nada; correcto: publica `TodosChanged { session: ctx.session, items }` y devuelve `ToolOutput::ok("Todo list updated: {done}/{total} completed")` y, si hay alguna `in_progress`, añade `" · now: {content}"` con la primera.

- [ ] **Step 1: Tests.** En `todo.rs`: `parses_valid_list_and_trims`, `empty_list_is_valid`, `rejects_non_array`, `rejects_more_than_fifty` (51 → `"too many todos (max 50)"`), `rejects_empty_content` (`"todo 2: empty content"` con el segundo en blanco), `rejects_unknown_status` (`"todo 1: invalid status 'wip'"`). En `todo_write.rs` (copia el helper `ctx(dir)` de los tests de `tools/view.rs`): `publishes_and_reports_progress` (suscribe al bus, ejecuta con dos tareas `completed` e `in_progress`; el evento trae 2 items con la `session` del ctx y la salida es exactamente `"Todo list updated: 1/2 completed · now: Segunda"`), `invalid_input_is_an_error_and_publishes_nothing` (`rx.try_recv()` da error).
- [ ] **Step 2:** `cargo test -p codeloom-core todo` → no compila.
- [ ] **Step 3: Implementar.** Descripción y schema de la herramienta orientados al modelo («Replace the whole todo list; keep at most one task in_progress»…). Registrar `TodoWriteTool` en `codeloom-cli/src/main.rs` junto a las demás. Añadir en `codeloom-tui/src/app.rs`, en `apply`, el brazo `LiveEvent::TodosChanged { .. } => {}` (la tarea T1 lo rellena).
- [ ] **Step 4:** `cargo test` del workspace en verde; clippy; rustfmt de los ficheros tocados.

---

### Task C2: Core: `question`, `PermissionResponder::ask` y `ask_user`

**Files:**
- Create: `crates/codeloom-core/src/question.rs`, `crates/codeloom-core/src/tools/ask_user.rs`
- Modify: `crates/codeloom-core/src/lib.rs`, `crates/codeloom-core/src/permission/mod.rs`, `crates/codeloom-core/src/tools/mod.rs`, `crates/codeloom-core/src/subagent.rs` (`.without("ask_user")`), `crates/codeloom-cli/src/main.rs` (registro)

**Interfaces:**
- Produces: `question::{MAX_QUESTIONS = 4, MIN_OPTIONS = 2, MAX_OPTIONS = 6, QuestionOption {label, description}, Question {header: String, question: String, options: Vec<QuestionOption>, multi_select: bool}, Answer {question: String, selected: Vec<String>}, parse_questions(&Value) -> Result<Vec<Question>, String>, format_answers(&[Answer]) -> String}` (todos `Debug, Clone, PartialEq, Eq`); en `PermissionResponder`: `async fn ask(&self, _questions: &[Question]) -> Option<Vec<Answer>> { None }`; `PermissionService::ask_user(&self, questions: &[Question]) -> Option<Vec<Answer>>`; `tools::ask_user::AskUserTool` (`ask_user`, `Exclusive`).
- Entrada: `{"questions":[{"header":"short label","question":"…","options":[{"label":"…","description":"…"}],"multi_select":false}]}`; `header`, `description` y `multi_select` opcionales (por defecto `""`, `""`, `false`). Errores de `parse_questions` (textos exactos): `"questions must be an array"`, `"between 1 and 4 questions are required"`, `"question {n}: empty question"`, `"question {n}: between 2 and 6 options are required"`, `"question {n}: option {m} has an empty label"`.
- `format_answers`: una línea por pregunta, `"{question}: {a1}, {a2}"` (respuestas unidas con `", "`; si `selected` está vacío, `"(no answer)"`), unidas con `\n`.
- `run`: parse → error `ToolOutput::err(msg)`; luego `tokio::select!` entre `cancel.cancelled()` → `ToolOutput::err("cancelled")` y `ctx.permission.ask_user(&questions)`; `None` → `ToolOutput::err("The user did not answer (no interactive user or the question was dismissed).")`; `Some(answers)` → `ToolOutput::ok(format_answers(&answers))`.

- [ ] **Step 1: Tests.** En `question.rs`: parse válido (con valores por defecto), `rejects_zero_and_five_questions`, `rejects_one_and_seven_options`, `rejects_empty_question_and_label`, `format_answers_joins_lines_and_marks_empty`. En `ask_user.rs` (helper `ctx` como en `view.rs`, pero con `PermissionService::with_responder(Box::new(TestResponder))`): `returns_the_formatted_answers` (el responder de test devuelve `Some(vec![Answer{question:"Q", selected:vec!["a","b"]}])` → salida `"Q: a, b"`), `no_user_is_an_error` (con `PermissionService::allow_all()` la salida es el error exacto de arriba), `cancellation_wins_while_waiting` (responder que duerme 10 s; se cancela el token tras 20 ms; salida `"cancelled"` con `is_error`), `invalid_input_does_not_ask`. En `subagent.rs` o donde esté el test del registro de hijos, añade un test que compruebe que el registro del hijo no contiene `ask_user` (sigue el patrón del test existente de `without`).
- [ ] **Step 2:** `cargo test -p codeloom-core question ask_user` → no compila.
- [ ] **Step 3: Implementar.** `async_trait` ya está en el módulo `permission`. Registrar `AskUserTool` en `codeloom-cli/src/main.rs`.
- [ ] **Step 4:** `cargo test` del workspace en verde; clippy; rustfmt.

---

### Task B1: `batten`: `todo_lines`

**Files:**
- Create: `crates/batten/src/state/todo_lines.rs`
- Modify: `crates/batten/src/state/transcript.rs` (`LineKind::TodoItem(ToolStatus)`), `crates/batten/src/state/mod.rs`, `crates/batten/src/widgets/transcript.rs` (`prefix`, `kind_token`; sigue el patrón de `ActivityHead` y de `status_token`)

**Interfaces:**
- Produces: `LineKind::TodoItem(ToolStatus)`; `pub fn todo_lines(items: &[(&str, ToolStatus)]) -> Vec<Line>` (re-exportada en `batten::state`). Solo importan `Pending`, `Running` (en curso) y `Success` (hecha); cualquier otro estado se trata como `Pending`.
- Formato: vacío → `vec![]`. Si no, cabecera `ActivityHead(h)` con texto `"todos  {hechas}/{total}"` (spans: `"todos"` → `ToolName`, resto → `Muted`), con `h = Success` si `hechas == total`, `Running` si alguna está en curso, `Pending` en otro caso; después hasta 12 líneas `TodoItem(estado)` con el texto del elemento (una línea; sustituye saltos de línea por espacio); si hay más de 12, una línea `ToolMore` `"… ({n} more)"`.
- Prefijo del widget: `TodoItem(Running)` → `"◐ "`, `TodoItem(Success)` → `"☑ "`, el resto → `"☐ "`. Token: `Running` → `TodoInProgress`, `Success` → `TodoCompleted`, el resto → `TodoPending`.

- [ ] **Step 1: Tests** en `todo_lines.rs`: `empty_has_no_lines`; `header_counts_done_over_total` (3 items, 1 hecha → texto `"todos  1/3"`, `ActivityHead(Pending)`); `header_is_running_when_one_is_in_progress`; `header_is_success_when_all_done`; `items_keep_their_status_and_one_line` (contenido `"a\nb"` → `"a b"`); `long_lists_are_folded_after_twelve` (15 items → 1 + 12 + 1 líneas, última `ToolMore` `"… (3 more)"`). En `widgets/transcript.rs`: test de pintado como el de `ActivityHead` (contiene `"☑ "` y `"◐ "`, y `plain()` no los contiene).
- [ ] **Step 2–4:** `cargo test -p batten todo` falla → implementar → `cargo test -p batten` verde, `cargo build -p batten --examples`, clippy, rustfmt.

---

### Task B2: `batten`: `QuestionDialog` y su widget

**Files:**
- Create: `crates/batten/src/state/question.rs`, `crates/batten/src/widgets/question.rs`
- Modify: `crates/batten/src/state/mod.rs`, `crates/batten/src/widgets/mod.rs`

**Interfaces (estado puro, sin ratatui):**
```rust
pub struct QuestionSpec { pub header: String, pub question: String, pub options: Vec<(String, String)>, pub multi: bool } // (label, description)
pub enum QuestionKey { Up, Down, Left, Right, Enter, Esc, Backspace, Char(char) }
pub enum QuestionOutcome { Pending, Done(Vec<Vec<String>>), Cancelled }
pub struct QuestionDialog { .. }
impl QuestionDialog {
    pub fn new(specs: Vec<QuestionSpec>) -> Self;
    pub fn handle(&mut self, key: QuestionKey) -> QuestionOutcome;
    pub fn current(&self) -> usize;          // pregunta actual (base 0)
    pub fn len(&self) -> usize;
    pub fn spec(&self) -> &QuestionSpec;     // de la pregunta actual
    pub fn cursor(&self) -> usize;           // 0..=options.len(); options.len() = fila «Other…»
    pub fn is_selected(&self, option: usize) -> bool;
    pub fn editing_other(&self) -> bool;
    pub fn other_text(&self) -> &str;
}
```
(`QuestionSpec`, `QuestionOutcome`: `Debug, Clone, PartialEq, Eq`; `QuestionKey`: `Debug, Clone, Copy, PartialEq, Eq`.) Re-exportar en `batten::state`.

**Comportamiento de `handle`** (por pregunta se guardan: cursor, conjunto de seleccionadas, texto de «Other…» y flag de edición):
- `Up`/`Down` mueven el cursor con vuelta circular entre `0..=options.len()`; ignoradas mientras se edita.
- `Char(' ')` sin edición: en fila de opción → `multi`: alterna; única: deja solo esa seleccionada. En la fila «Other…» → empieza a editar. Con edición: `Char(c)` añade `c` al texto (incluido el espacio); `Backspace` borra el último carácter (sin edición se ignora).
- `Enter`: con edición → termina la edición; si el texto (trim) no está vacío, confirma la pregunta (en única, además, vacía la selección: la respuesta es el texto); si está vacío, solo sale de la edición y no avanza. Sin edición: en fila «Other…» empieza a editar; en fila de opción → única: selecciona esa y confirma; múltiple: si no hay nada seleccionado ni texto, selecciona la actual; confirma.
- Confirmar: si es la última pregunta devuelve `Done(respuestas)`; si no, pasa a la siguiente (cursor 0).
- `Left` (sin edición, `current > 0`): pregunta anterior. `Right` (sin edición): siguiente solo si la actual tiene respuesta (selección no vacía o texto no vacío) y no es la última; si no, se ignora.
- `Esc`: con edición → sale de la edición conservando el texto, `Pending`; sin edición → `Cancelled`.
- Respuestas de una pregunta: etiquetas de las seleccionadas en orden de opción, más el texto de «Other…» (trim, si no vacío) al final.
- `handle` devuelve `Pending` salvo en `Done`/`Cancelled`.

**Widget** `pub fn render(frame: &mut Frame, screen: Rect, dialog: &QuestionDialog, theme: &Theme)`: ventana centrada (ancho `min(72, screen.width - 4)`, alto lo que haga falta acotado a la pantalla; con tamaños diminutos no debe entrar en pánico), `Clear`, bloque redondeado con borde `accent` y título `" Question {i}/{n} · {header} "` (sin ` · ` si el header está vacío). Cuerpo: la pregunta (con wrap), línea en blanco, una fila por opción con marcador (`( )`/`(•)` en única, `[ ]`/`[x]` en múltiple), la etiqueta en negrita y la descripción en `muted` a continuación; la fila bajo el cursor con prefijo `› ` y estilo `accent`; última fila `Other…` que, si hay texto o se está editando, muestra el texto y un `▏` al final cuando se edita. Pie en `muted`: `"↑↓ move · space select · enter confirm · ←→ question · esc cancel"`, o `"type your answer · enter done · esc back"` si se edita.

- [ ] **Step 1: Tests de estado** en `state/question.rs` (helper `spec(multi, n_options)`): `single_choice_selects_and_finishes` (1 pregunta, 3 opciones; `Down`, `Enter` → `Done(vec![vec!["b"]])`); `multi_choice_toggles_then_confirms` (`Char(' ')`, `Down`, `Char(' ')`, `Enter` → `Done([["a","b"]])`); `multi_enter_with_nothing_selects_current`; `other_text_is_an_answer` (única; `Down`×n hasta «Other…», `Enter`, `Char('h')`, `Char('i')`, `Enter` → `Done([["hi"]])`); `empty_other_does_not_advance` (`Enter` en «Other…», `Enter` → `Pending` y `!editing_other()`); `multi_other_is_appended_after_selected`; `right_requires_an_answer` (`Right` sin responder → `current()` sigue 0; tras responder con `Char(' ')` en multi, `Right` → 1); `left_goes_back_and_keeps_answers`; `esc_cancels_but_only_leaves_edit_first` (editando: `Esc` → `Pending` y texto conservado; otro `Esc` → `Cancelled`); `cursor_wraps_and_up_down_are_ignored_while_editing`; `two_questions_flow_to_done` (devuelve dos vectores). Tests de widget en `widgets/question.rs` (TestBackend como en `permission.rs`): contiene `"Question 1/2"`, las etiquetas, `"Other"`, marcadores `(•)` tras seleccionar, `[x]` en múltiple, y `render` no entra en pánico con `(w,h)` en `[(1,1),(10,3),(40,8),(120,40)]`.
- [ ] **Step 2–4:** fallan → implementar → `cargo test -p batten` verde, `cargo build -p batten --examples`, clippy, rustfmt.

---

### Task T1: TUI: lista de tareas (evento, transcript, rail, restauración)

**Files:**
- Modify: `crates/codeloom-tui/src/app.rs`, `crates/codeloom-tui/src/rail.rs`, `crates/codeloom-tui/src/render.rs` (solo tests)

**Interfaces:**
- Consumes: C1 (`TodoItem`, `TodoStatus`, `parse_todos`, `LiveEvent::TodosChanged`), B1 (`todo_lines`).
- Produces en `App`: `pub todos: Vec<codeloom_core::todo::TodoItem>`; campo privado `todo_entry: Option<(EntryId, Vec<Line>)>`; método privado `sync_todos(&mut self)`.

**Comportamiento**
- `apply(TodosChanged { session, items })`: solo si `self.session_id == Some(session)`; `self.todos = items; self.sync_todos()`.
- `sync_todos`: mapea `TodoStatus` → `ToolStatus` (`Pending`→`Pending`, `InProgress`→`Running`, `Completed`→`Success`), construye `todo_lines`; igual que las tools: la entrada se crea la primera vez (siempre; vacía si el look no es `Inline`; con `Inline` pone `self.assistant = None`), después solo se reemplaza si cambió la caché. `sync_all_tools` también aplica la visibilidad de la entrada de tareas.
- Las entradas de tool con `name == "todo_write"` no se pintan en el transcript (vacías), pero siguen en `app.tools` y en el inspector.
- `restore_messages`: vacía `todos` y `todo_entry`; al recorrer mensajes de asistente, por cada `ContentPart::ToolCall` con `name == "todo_write"` cuyo `input` parsee con `parse_todos`, asigna `todos` y llama a `sync_todos`. Con `Err`, ignora.
- Rail: tras la sección MODEL y antes de FILES, sección `TODOS  {hechas}/{total}` (encabezado `accent` en negrita, contador `muted`) seguida de hasta 8 líneas con prefijo `☐ `/`◐ `/`☑ ` coloreado con `theme.styles.todo_pending|todo_in_progress|todo_completed`, y `"… +{n} more"` si sobran. Se omite si no hay tareas.

- [ ] **Step 1: Tests** en `app.rs` (con `crush_app()`): `todos_event_creates_and_updates_one_element` (dos `TodosChanged` seguidos: `entry_count` no crece con el segundo y el texto `"todos  1/2"` aparece/ cambia); `foreign_session_todos_are_ignored`; `classic_look_keeps_todos_out_of_the_transcript_but_stores_them`; `todo_write_tool_has_no_transcript_card` (ToolStarted `name:"todo_write"` en Crush → ninguna línea que empiece por `todo_write`); `empty_list_clears_the_element`; `restore_rebuilds_todos_from_history` (mensaje de asistente con `ToolCall{name:"todo_write", input: json!({"todos":[…]})}`; `app.todos.len()` y línea `"todos  0/2"`); `restore_ignores_invalid_todo_input`. En `render.rs`: `rail_lists_todos_with_progress` (contiene `"TODOS"`, `"1/2"` y el texto de una tarea) y `rail_caps_long_todo_lists` (20 tareas → `"… +12 more"`).
- [ ] **Step 2–4:** fallan → implementar → `cargo test` workspace, clippy, rustfmt (si rustfmt reformatea código ajeno de app.rs/rail.rs/render.rs, deshazlo solo en esos hunks).

---

### Task T2: TUI: diálogo de preguntas (canal, cola, teclas, render)

**Files:**
- Modify: `crates/codeloom-tui/src/widgets/permission.rs`, `crates/codeloom-tui/src/permission_flow.rs`, `crates/codeloom-tui/src/app.rs`, `crates/codeloom-tui/src/run.rs`, `crates/codeloom-tui/src/render.rs`, `crates/codeloom-cli/src/main.rs`

**Interfaces:**
- Consumes: C2 (`Question`, `Answer`, `PermissionResponder::ask`), B2 (`QuestionDialog`, `QuestionSpec`, `QuestionKey`, `QuestionOutcome`, `widgets::question::render`).
- Produces: `permission_flow::QuestionReply = oneshot::Sender<Option<Vec<Answer>>>`; `UiPermissionResponder::with_questions(self, tx: mpsc::Sender<(Vec<Question>, QuestionReply)>) -> Self` (campo `Option`) y su implementación de `ask` (sin canal → `None`; si el envío falla o el `oneshot` se cierra → `None`); en `App`: `pub question: Option<PendingQuestion>` (con `dialog: QuestionDialog`, `questions: Vec<Question>`, `reply`), `question_queue`, `pub fn enqueue_questions(&mut self, questions: Vec<Question>, reply: QuestionReply)`, `pub fn question_key(&mut self, key: batten::state::QuestionKey)`, `pub fn cancel_question(&mut self)`; `run(..)` recibe un nuevo `ask_rx: mpsc::Receiver<(Vec<Question>, QuestionReply)>` justo después de `perm_rx`; `codeloom-cli` crea el canal (`mpsc::channel(4)`), llama a `.with_questions(ask_tx)` y pasa `ask_rx`.

**Comportamiento**
- `enqueue_questions` cierra los demás overlays (como `enqueue_permission`), encola y abre la siguiente si no hay diálogo abierto. Mapea `Question` → `QuestionSpec` (`header`, `question`, `options` como `(label, description)`, `multi: multi_select`).
- `question_key`: delega en `dialog.handle`; `Done(respuestas)` → envía `Some(Vec<Answer>)` (una por pregunta: `question` = texto, `selected` = respuestas) y abre la siguiente de la cola; `Cancelled` → envía `None` y abre la siguiente. `cancel_question` equivale a `Cancelled` sobre el diálogo abierto (y vacía la cola enviando `None` a cada una al cerrar la aplicación).
- Teclas (`run.rs`): después del bloque existente de permisos (que mantiene prioridad), si `app.question.is_some()`: mapear `KeyCode::{Up,Down,Left,Right,Enter,Esc,Backspace}` y `Char(c)` (incluido el espacio) a `QuestionKey` y llamar a `question_key`; el resto de teclas se consumen sin efecto. Con diálogo de preguntas abierto, el composer no recibe teclas ni muestra cursor (añadir `&& app.question.is_none()` a la condición de foco de `draw_transcript`).
- `run` añade un brazo en su `select!` para `ask_rx.recv()` (mismo patrón que `perm_rx`) llamando a `enqueue_questions`; al salir de la aplicación se cancelan las preguntas abiertas y encoladas (igual que se hace con los permisos).
- Render (`render()` en `render.rs`): `if let Some(q) = &app.question { batten::widgets::question::render(frame, area, &q.dialog, &app.theme) }` **antes** del bloque de permisos, para que un permiso quede por encima.

- [ ] **Step 1: Tests.** `widgets/permission.rs`: `question_roundtrip_returns_answers` (canal de preguntas; el responder `ask` devuelve lo que responde el receptor) y `without_channel_ask_is_none`. `app.rs`: `enqueue_questions_opens_the_first_and_queues_the_rest`; `answering_sends_answers_and_opens_the_next` (reply con `oneshot::channel`, `rx.try_recv()` da `Some(vec![Answer{question,selected}])`); `esc_sends_none`; `cancel_question_drains_everything_with_none`; `questions_close_other_overlays`. `run.rs` (siguiendo cómo se testea hoy el teclado de permisos): `keys_reach_the_question_dialog_and_not_the_composer` (el composer no cambia con `Char('x')`). `render.rs`: `question_dialog_renders_and_hides_the_composer_cursor` (contiene `"Question 1/1"` y la etiqueta de una opción) y `permission_is_drawn_above_a_question` (con ambos abiertos, el contenido muestra el diálogo de permisos).
- [ ] **Step 2–4:** fallan → implementar → `cargo test` workspace en verde, clippy, rustfmt de los ficheros tocados.

---

## Validación manual (la hace el usuario)

Pedirle al modelo una tarea larga con varios pasos: la lista `todos  n/m` aparece y se va tachando en el transcript y en el rail (Crush); reabrir la sesión con `/sessions` la restaura. Pedirle que te haga varias preguntas con opciones: se abre el diálogo, probar selección única, múltiple, «Other…», navegación con ←→ y Esc.
