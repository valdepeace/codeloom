# El subagente como nodo de un árbol — Plan de implementación

> **Para agentes:** una tarea cada vez, en orden. Quien supervisa commitea, así que **no commitees**.

**Meta:** que cada subagente sea un nodo plegable de la conversación del padre, con iconos de estado A2A, su conversación y sus herramientas colgando de él, y las llamadas del padre sobre él anidadas bajo su nodo.

**Arquitectura:** el core reenvía también los mensajes del hijo por el canal etiquetado `ChildEvent`. La TUI guarda un registro acotado por hijo y dibuja la entrada de actividad del subagente con un constructor de líneas de nodo (`batten`), con dos tipos de línea nuevos. El estado plegado/desplegado vive en memoria de la interfaz.

**Spec:** `docs/specs/2026-10-22-codeloom-subagent-tree-design.md` (**leerla entera antes**). Manda: los iconos son los ocho glifos A2A de `AgentState::glyph`; el estado A2A **exacto** va siempre entre paréntesis; el árbol es presentación local y no se presenta como de A2A. **Plegado por defecto.**

## Restricciones globales

- `codeloom-core` no depende de `ratatui`/`crossterm`. **Sin red en tests.**
- Comprobaciones: `cargo test -p batten -p codeloom-core -p codeloom-tui -p codeloom-cli`, `cargo clippy -p batten -p codeloom-core -p codeloom-tui -p codeloom-cli -- -D warnings`, `rustfmt --edition 2021 --check <ficheros tocados>`, y `tr -cd '\r' < fichero | wc -c` = 0.
- Entorno de build (Git Bash): `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe' CARGO_TARGET_DIR='E:/workspace/personal/loom/target' CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`. No crear otros `target*`. Formatear solo con `rustfmt --edition 2021 <ruta>`. Ficheros en LF.
- Comentarios en español «pijo», sin tacos ni andalucismos. No `git checkout/restore/stash/reset`. No tocar `codeloom.toml`. La salida de Bash puede llegar comprimida: ante la duda, redirigir a fichero y mostrar el código de salida.

## Review Focus

- **Fluidez del composer:** nada por token (los deltas del hijo no se reenvían); registro del hijo **acotado**; un nodo plegado ocupa **una línea**; la entrada solo se reemplaza si sus líneas cambian.
- Los ocho estados A2A dan **ocho iconos distintos** y su **nombre exacto** entre paréntesis.
- Un hijo sin eventos (descriptor sin registro) se dibuja plegado sin fallar.
- Una llamada `job_output` a un id que **no** es de ningún hijo sigue siendo una línea normal del padre.
- Con el look compacto, el nodo del subagente muestra **todas** sus líneas (las demás herramientas siguen mostrando solo su cabecera).

---

### Tarea 1: el core reenvía los mensajes del hijo

**Ficheros:** `crates/codeloom-core/src/event/mod.rs`, `crates/codeloom-core/src/subagent.rs` (tests), y lo mínimo en `crates/codeloom-tui/src/app.rs` para que siga compilando y sin cambiar comportamiento (`apply_child_event` debe **ignorar** `MessageAdded` por ahora).

**Produce:** `LiveEvent::is_child_relevant` devuelve `true` para `LiveEvent::MessageAdded(_)` (sigue `false` para `Delta` y `ReasoningDelta`).

- [ ] **Paso 1: tests en rojo.** En `event/mod.rs`: `message_added_is_child_relevant_but_deltas_are_not`. En `subagent.rs`, junto al test que comprueba el reenvío etiquetado (`child_events_are_forwarded_tagged_with_the_child`): `child_messages_are_forwarded_tagged_with_the_child` (un `ChildEvent` cuyo `event` es `MessageAdded` llega al bus del padre con la identidad del hijo).
- [ ] **Paso 2: implementar. Paso 3: verificar** (`cargo test -p codeloom-core -p codeloom-tui`, clippy, rustfmt, LF). **Paso 4: no commitear.**

---

### Tarea 2: líneas de nodo en `batten`

**Ficheros:** `crates/batten/src/state/transcript.rs` (dos variantes de `LineKind`), `crates/batten/src/state/node_lines.rs` (nuevo; declarar y reexportar en `state/mod.rs`), `crates/batten/src/widgets/transcript.rs` (`prefix` y `kind_token`).

**Produce:**
```rust
// LineKind: NodeHead(ToolStatus)  -> prefijo ""  y color de status_token(status)
//           NodeBody              -> prefijo ""  y StyleToken::Muted
pub struct NodeChild<'a> { pub glyph: &'a str, pub text: &'a str }
pub struct NodeView<'a> {
    pub glyph: &'a str,          // icono de estado A2A (un carácter)
    pub kind: &'a str,           // "subagent"
    pub label: &'a str,
    pub tag: Option<&'a str>,    // p. ej. "[rust]" o "[front·project]"
    pub state: &'a str,          // nombre A2A exacto: "working", "input-required"…
    pub status: ToolStatus,      // agrupado, solo para el color
    pub summary: Option<&'a str>,// p. ej. "step 2/30 · read_file"
    pub expanded: bool,
    pub children: &'a [NodeChild<'a>],
}
pub fn node_lines(view: &NodeView<'_>) -> Vec<Line>
```
Cabecera (una `Line` de tipo `NodeHead(status)`): `"{▾ si expanded, ▸ si no} {glyph} {kind}  {label}[ {tag}] ({state})[ · {summary}]"`, con un `SpanSpec` `ToolName` sobre `kind`. Plegado: **solo** esa línea. Desplegado: a continuación una `Line` `NodeBody` por hijo, con texto `"├─ {glyph} {text}"` y la última `"└─ {glyph} {text}"`; sin hijos, nada más que la cabecera.

- [ ] **Paso 1: tests en rojo** (en `node_lines.rs`): `folded_node_is_one_line_with_glyph_and_exact_state`, `expanded_node_lists_children_with_tree_guides` (guía `├─` en los intermedios y `└─` en el último), `node_head_without_tag_or_summary_has_no_stray_spaces`. Y en `widgets/transcript.rs`: un test de que `NodeHead` y `NodeBody` no llevan prefijo de glifo (`prefix` devuelve `""`).
- [ ] **Paso 2: implementar. Paso 3: verificar** (`cargo test -p batten`, clippy, rustfmt, LF). **Paso 4: no commitear.**

---

### Tarea 3: registro acotado del hijo en la TUI

**Ficheros:** `crates/codeloom-tui/src/app.rs`.

**Consume:** tarea 1. **Produce:**
```rust
pub const MAX_CHILD_ITEMS: usize = 200;
pub enum ChildItem {
    Text(String),                                   // texto del asistente del hijo, recortado a 240 caracteres
    Tool { call_id: String, name: String, summary: Option<String>, status: batten::state::ToolStatus },
}
pub struct ChildLog { pub items: std::collections::VecDeque<ChildItem> }
// App: child_logs: HashMap<i64, ChildLog>
```
`apply_child_event` pasa a alimentar el registro: `MessageAdded` con `ContentPart::Text` → `Text`; con `ContentPart::ToolCall{id,name,input}` → `Tool{..., status: Running}` con `summary` = `tool_facts(&name,&input).primary` recortado a 80 caracteres; con `ContentPart::ToolResult{call_id,is_error,..}` → pone el `status` de ese `Tool` a `Success` o `Error`; `ToolFinished{call_id,is_error}` hace lo mismo. Al superar `MAX_CHILD_ITEMS`, se descarta el más antiguo. Un mensaje de usuario del hijo no se registra.

- [ ] **Paso 1: tests en rojo:** `child_log_records_text_and_tool_calls_and_results`, `child_log_is_bounded` (insertar `MAX_CHILD_ITEMS + 5` y comprobar la cota y que se conservan los más recientes), `child_log_ignores_user_messages_and_unknown_results`.
- [ ] **Paso 2: implementar. Paso 3: verificar** (`cargo test -p codeloom-tui`, clippy, rustfmt, LF). **Paso 4: no commitear.**

---

### Tarea 4: el subagente se dibuja como nodo

**Ficheros:** `crates/codeloom-tui/src/app.rs` (`sync_subagent_activity`, `sync_activity`, `sync_all_tools`).

**Consume:** tareas 2 y 3. **Produce:** `sync_subagent_activity` construye las líneas con `batten::state::node_lines(&NodeView{..})` en lugar de `activity_lines`: `glyph = agent_state(state).glyph()`, `state = agent_state(state).label()`, `tag = agent_tag(subagent)`, `status = subagent_tool_status(state)`, `summary` desde `agent_stats` (`"step {step}/{max_steps} · {tool}"`, omitiendo lo que falte), `expanded` desde `self.expanded_nodes.contains(&child)`, y `children` desde el `ChildLog` más, si el estado es terminal y hay `result_text()`, una última línea `result: {primera línea}`. Icono de cada herramienta hija: `Running` `…`, `Success` `✓`, `Error` `✗`, otros `·`. Texto hijo: glifo `›`. Se añade `expanded_nodes: HashSet<i64>` a `App` (vacío = todo plegado).

**Regla de visibilidad:** `sync_activity` y `sync_all_tools` muestran **todas** las líneas de una entrada de tipo `"subagent"` también con el look compacto (el resto de entradas siguen mostrando solo la cabecera).

El cuerpo con el resultado completo que antes llevaba la entrada se sustituye por la línea `result:` del nodo (el resultado entero se sigue viendo en la sesión del hijo).

- [ ] **Paso 1: tests en rojo:** `subagent_node_is_one_folded_line_by_default`, `each_a2a_state_has_its_own_glyph_and_exact_name` (recorre `TaskState::ALL`: ocho glifos distintos y `({nombre})` exacto en la cabecera), `node_summary_shows_step_and_current_tool`, `classic_look_shows_every_line_of_a_subagent_node`. **Adaptar** los tests existentes que comprueban `subagent  review (working)` y `activity_header_*` a la cabecera nueva (`▸ ● subagent  review … (working)`), dejando constancia de por qué cambian.
- [ ] **Paso 2: implementar. Paso 3: verificar** (`cargo test -p batten -p codeloom-tui`, clippy, rustfmt, LF). **Paso 4: no commitear.**

---

### Tarea 5: desplegar y plegar

**Ficheros:** `crates/codeloom-tui/src/app.rs`, `crates/codeloom-tui/src/run.rs` (`transcript_key`).

**Produce:** `App::set_node_expanded(&mut self, expanded: bool) -> bool` actúa sobre la entrada de actividad `"subagent"` cuyo `EntryId` coincide con el bloque activo (`block_index()`); añade o quita el hijo de `expanded_nodes`, vuelve a sincronizar el nodo y devuelve `true` si había nodo. En `transcript_key`, con el foco en el transcript: `KeyCode::Right` o `KeyCode::Enter` llaman a `set_node_expanded(true)` y `KeyCode::Left` a `set_node_expanded(false)`, y **solo** consumen la tecla si había nodo (si no, se comportan como hasta ahora). Cada evento `ChildEvent` que cambie el registro de un hijo desplegado vuelve a sincronizar su nodo; con el nodo plegado, solo se resincroniza su cabecera (resumen).

- [ ] **Paso 1: tests en rojo:** `right_expands_the_active_subagent_node_and_left_folds_it` (el nodo pasa de 1 línea a 1 + hijos y vuelve), `expanding_without_an_active_node_does_nothing`, `expanded_node_follows_new_child_events`.
- [ ] **Paso 2: implementar. Paso 3: verificar** (`cargo test -p codeloom-tui`, clippy, rustfmt, LF). **Paso 4: no commitear.**

---

### Tarea 6: las llamadas del padre sobre el hijo cuelgan de su nodo

**Ficheros:** `crates/codeloom-tui/src/app.rs` (`sync_tool`, `sync_all_tools`, construcción del nodo).

**Produce:** una llamada a `job_output` o `job_kill` cuyo `id` (`tool.facts.primary`) coincide con el `job_id` de algún hijo de `self.subagents`, o a `subagent_control` cuyo `secondary` es el id de sesión de un hijo, **no genera línea visible** en el padre (como la lista de tareas) y se anota en `node_actions: HashMap<i64, Vec<String>>` (clave: id de sesión del hijo; valor: nombre de la herramienta). El nodo, desplegado, añade una línea hija `↳ parent: {herramienta}` por cada una; plegado, su resumen añade `· {n} queries` cuando `n > 0`. Una llamada cuyo id no es de ningún hijo sigue siendo una línea normal.

- [ ] **Paso 1: tests en rojo:** `job_output_on_a_child_job_hangs_from_its_node_and_leaves_no_line`, `job_output_on_an_unknown_id_stays_a_normal_line`, `folded_node_counts_the_parent_queries`.
- [ ] **Paso 2: implementar. Paso 3: verificar** (`cargo test -p codeloom-tui`, clippy, rustfmt, LF). **Paso 4: no commitear.**

---

### Tarea 7: documentación

**Ficheros:** `docs/specs/2026-10-22-codeloom-subagent-tree-design.md`, `docs/plans/2026-10-22-codeloom-a2a-roadmap.md`.

- [ ] **Paso 1:** `## Estado (fecha)` en la spec: implementado (commits), decisiones tomadas al implementar, limitaciones reales (sin streaming del hijo, sin cuerpos de herramienta, un nivel, estado de despliegue no persistente, `⚿` sin verificar en la fuente de la persona usuaria, **sin ver en terminal real**, medición de fluidez pendiente si no se hizo). En la hoja de ruta, anotar la funcionalidad con sus commits.
- [ ] **Paso 2: verificar** LF. **Paso 3: no commitear.**
