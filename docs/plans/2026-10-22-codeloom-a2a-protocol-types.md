# Tipos de A2A en el núcleo (`Message`, `Part`, `Artifact`) — Plan de implementación

> **Para agentes:** una tarea cada vez, en orden. Quien supervisa commitea, así que **no commitees**.

**Meta:** que el mensaje de estado de una tarea sea un `Message` de A2A y el resultado de un subagente sean `Artifact`s, retirando las cadenas.

**Arquitectura:** tipos nuevos en `codeloom-core::a2a` (módulo nuevo), `TaskStatus.message: Option<Message>`, `SubagentDescriptor.result` pasa a `artifacts: Vec<Artifact>`, el almacén los guarda como JSON y lee lo antiguo, la TUI y el aviso al padre derivan su texto de las `Part`s.

**Spec:** `docs/specs/2026-10-22-codeloom-a2a-protocol-types-design.md` (**leerla entera antes**). Manda: **no inventar nada que el proto no contemple**; lo rotulado como lectura nuestra no se amplía.

## Restricciones globales

- `codeloom-core` no depende de `ratatui`/`crossterm`; `batten` no depende de `codeloom-core`. Errores de librería con `thiserror`. **Sin red en tests.**
- Comprobaciones: `cargo build`, `cargo test -p batten -p codeloom-core -p codeloom-tui -p codeloom-cli`, `cargo clippy -p codeloom-core -p codeloom-tui -- -D warnings`, `rustfmt --edition 2021 --check <ficheros tocados>`. Si `clippy` muere con «memory allocation failed», repetir; si persiste, `RUST_MIN_STACK=67108864`.
- Entorno de build (Git Bash): `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe' CARGO_TARGET_DIR='E:/workspace/personal/loom/target' CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`. No crear otros `target*`. Formatear solo con `rustfmt --edition 2021 <ruta>`, nunca `cargo fmt --all`. Ficheros en LF (`tr -cd '\r' < f | wc -c` debe dar 0).
- Comentarios en español «pijo», sin tacos ni andalucismos. No `git checkout/restore/stash/reset` ni push. No tocar `codeloom.toml` de la raíz. No reescribir planes ni specs históricos.
- `JobStatus`/`JobResult` de los trabajos **no se tocan**.

## Review Focus

- Una base con `status_message` en texto plano y `result` sin artefactos se abre sin error y da `Message`/`Artifact` equivalentes; abrir dos veces seguidas no falla (migración idempotente).
- El texto del aviso al padre y lo que muestra la interfaz son **idénticos** a los de antes del cambio.
- `Part` con `Raw` (bytes) y `Data` (JSON) sobrevive a la ida y vuelta serde, no solo `Text`.
- Un `Message` sin `Part`s de texto da cadena vacía al pedir su texto, sin pánico.
- Cero `Option<String>` de mensaje de estado y cero `result: Option<String>` en `SubagentDescriptor` al terminar.

---

### Tarea 1: tipos `a2a` (aditiva, no rompe nada)

**Ficheros:** crear `crates/codeloom-core/src/a2a.rs`; declarar `pub mod a2a;` en `crates/codeloom-core/src/lib.rs`.

**Interfaces — produce:**
```rust
// a2a.rs  (todos: Debug, Clone, PartialEq, Serialize, Deserialize; Eq cuando sea posible)
pub enum Role { User, Agent }                       // sin UNSPECIFIED
pub enum PartContent { Text(String), Raw(Vec<u8>), Url(String), Data(serde_json::Value) }
pub struct Part { pub content: PartContent, pub metadata: Option<serde_json::Value>,
                  pub filename: Option<String>, pub media_type: Option<String> }
pub struct Message { pub message_id: String, pub context_id: Option<String>, pub task_id: Option<String>,
                     pub role: Role, pub parts: Vec<Part>, pub metadata: Option<serde_json::Value>,
                     pub extensions: Vec<String>, pub reference_task_ids: Vec<String> }
pub struct Artifact { pub artifact_id: String, pub name: Option<String>, pub description: Option<String>,
                      pub parts: Vec<Part>, pub metadata: Option<serde_json::Value>, pub extensions: Vec<String> }
impl Part     { pub fn text(text: impl Into<String>) -> Part; pub fn as_text(&self) -> Option<&str> }
impl Message  { pub fn agent_text(text: impl Into<String>) -> Message; pub fn text(&self) -> String }
impl Artifact { pub fn text_artifact(text: impl Into<String>) -> Artifact; pub fn text(&self) -> String }
pub fn artifacts_text(artifacts: &[Artifact]) -> String   // une el texto de cada artefacto con "\n"
pub fn new_id() -> String                                  // generador de ids del módulo
```
`Message::text`/`Artifact::text` unen con `"\n"` las `Part`s de texto y devuelven `""` si no hay ninguna. `agent_text` y `text_artifact` rellenan los ids con `new_id()`; `new_id` reutiliza el generador del núcleo si existe, si no un contador atómico más `now_ms` (informa de cuál usaste).

- [ ] **Paso 1: tests en rojo:** `part_roundtrips_through_serde_for_every_variant` (Text, Raw, Url, Data), `message_text_joins_text_parts_and_ignores_the_rest`, `message_without_text_parts_has_empty_text`, `artifact_text_joins_text_parts`, `artifacts_text_joins_artifacts_with_newline`, `agent_text_uses_agent_role_and_one_text_part`, `new_id_is_unique_across_calls`.
- [ ] **Paso 2: implementar. Paso 3: verificar** `cargo test -p codeloom-core`, clippy, rustfmt, LF. **Paso 4: no commitear.**

---

### Tarea 2: del texto a los tipos (núcleo, almacén e interfaz)

**Ficheros:** `crates/codeloom-core/src/{task.rs,subagent.rs,subagent_input.rs,session/mod.rs,session/sqlite.rs}`, `crates/codeloom-tui/src/{app.rs,rail.rs}` y lo que el compilador exija.

**Interfaces — consume:** los de la tarea 1. **Produce:**
```rust
// task.rs
pub struct TaskStatus { pub state: TaskState, pub message: Option<Message>, pub timestamp_ms: i64 }
impl TaskStatus { pub fn with_message(self, message: impl Into<String>) -> Self  // sigue aceptando texto: construye Message::agent_text
                  pub fn message_text(&self) -> Option<String> }
// session/mod.rs
SubagentDescriptor { ..., pub artifacts: Vec<Artifact>, ... }   // sustituye a `result: Option<String>`
impl SubagentDescriptor { pub fn result_text(&self) -> Option<String>   // None si no hay artefactos
                          pub fn context_id(&self) -> &str }            // = id de la sesión del padre
// complete_subagent recibe `&[Artifact]` en lugar de `&str`
```

- [ ] **Paso 1: tests en rojo:** `status_message_roundtrips_through_sqlite_as_a_message`, `legacy_plain_text_status_message_reads_as_agent_message`, `artifacts_roundtrip_through_sqlite`, `legacy_result_column_reads_as_one_text_artifact`, `column_migration_for_artifacts_is_idempotent`, `complete_subagent_stores_a_text_artifact`, `parent_notice_text_is_unchanged_after_the_artifact_change`, `result_text_is_none_without_artifacts`, `context_id_is_the_parent_session_id`, `tui_shows_the_same_result_text_as_before`.
- [ ] **Paso 2: implementar.** `with_message` conserva la firma de texto para no tocar los productores (`subagent.rs`, `subagent_input.rs`). SQLite: `status_message` guarda el JSON del `Message` (lectura tolerante: si no parsea, texto plano → `Message::agent_text`); columna nueva `artifacts_json TEXT` añadida de forma idempotente (`PRAGMA table_info`); si es `NULL` y `result` no, un único artefacto de texto. `complete_subagent` escribe `artifacts_json` y **no** vuelve a escribir `result`. Aviso al padre (`subagent.rs`, donde hoy lee `descriptor.result`) usa `result_text()`; la TUI (`app.rs`, `rail.rs`) usa `result_text()` y `message_text()`: **mismo texto en pantalla que antes**.
- [ ] **Paso 3: verificar** build, tests de los cuatro crates, clippy, rustfmt, LF; `grep -rn "result: Option<String>" crates/codeloom-core/src/session/mod.rs` sin resultados. **Paso 4: no commitear.**

---

### Tarea 3: documentación

**Ficheros:** `docs/specs/2026-10-22-codeloom-a2a-protocol-types-design.md`, `docs/specs/2026-10-22-codeloom-a2a-task-model-design.md`, `docs/specs/2026-10-15-loom-agent-profile-a2a.md`, `docs/plans/2026-10-22-codeloom-a2a-roadmap.md`.

- [ ] **Paso 1:** añadir `## Estado (fecha)` a la spec de tipos con lo implementado, decisiones tomadas y limitaciones reales. En la spec del modelo de tareas, marcar como **resueltas** las divergencias del mensaje de estado y del resultado, y dejar abierta `reference_task_ids`/`contextId` compartido (A11). En la spec A2A, actualizar la nota de divergencias. En la hoja de ruta, marcar A5 hecha con los commits y anotar lo abierto.
- [ ] **Paso 2: verificar** LF. **Paso 3: no commitear.**
