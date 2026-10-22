# Registro de agentes con fuentes intercambiables — Plan de implementación

> **Para agentes:** una tarea cada vez, en orden. Quien supervisa commitea, así que **no commitees**.

**Meta:** un registro de agentes (tarjeta + ejecución local) alimentado por ficheros globales y de proyecto, con agente `default`, precedencia, recarga en caliente, aprobación de los agentes de proyecto, y un parámetro de agente en `SubagentLauncher::start`.

**Arquitectura:** módulo nuevo `crates/codeloom-core/src/registry/` (`mod.rs`, `profile.rs`, `source.rs`, `trust.rs`, `watch.rs`). Aditivo hasta la tarea 6, que toca el lanzador. El registro entrega instantáneas inmutables `Arc<RegistrySnapshot>`.

**Spec:** `docs/specs/2026-10-22-codeloom-a2a-agent-registry-design.md` (**leerla entera antes**, con `2026-10-22-codeloom-a2a-agent-card-design.md` y la sección 2 de `2026-10-15-loom-agent-profile-a2a.md`). Manda: **no inventar nada que A2A contemple de otra forma**; lo propio se rotula como propio.

## Restricciones globales

- `codeloom-core` no depende de `ratatui`/`crossterm`; errores de librería con `thiserror`. **Sin red en tests**; los tests de ficheros usan `tempfile`.
- Comprobaciones: `cargo build`, `cargo test -p batten -p codeloom-core -p codeloom-tui -p codeloom-cli`, `cargo clippy -p codeloom-core -p codeloom-tui -- -D warnings`, `rustfmt --edition 2021 --check <ficheros tocados>`. Si `clippy` muere con «memory allocation failed», repetir; si persiste, `RUST_MIN_STACK=67108864`.
- Entorno de build (Git Bash): `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe' CARGO_TARGET_DIR='E:/workspace/personal/loom/target' CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`. No crear otros `target*`. Formatear solo con `rustfmt --edition 2021 <ruta>`, nunca `cargo fmt --all`. Ficheros en LF (`tr -cd '\r' < f | wc -c` = 0).
- Comentarios en español «pijo», sin tacos ni andalucismos. No `git checkout/restore/stash/reset` ni push. No tocar `codeloom.toml` de la raíz.
- **Dependencias nuevas:** elige un parser de YAML **mantenido** (verifica que no esté archivado) y, si hace falta, una huella (`sha2`) y vigilancia de ficheros (`notify`); informa de qué elegiste y de la versión verificada. Nada de red en tests.

## Review Focus

- Un fichero roto o a medias **no** hace desaparecer al agente: se conserva la última versión válida y se anota el problema.
- Un nombre con mayúsculas, `..`, `/`, `\` o vacío se rechaza; `name` distinto del nombre del fichero se rechaza.
- Un agente de proyecto sin aprobar **no se puede lanzar**; cambiar un byte de su fichero invalida la aprobación; uno global no la requiere.
- Una recarga con una tarea en curso no altera la instantánea de esa tarea.
- `start` con agente inexistente o pendiente **no crea tarea** (validación de entrada).
- `scope` declarado produce el aviso «declarado y no aplicado» y no se aplica.
- La vigilancia no bloquea el hilo de la interfaz ni emite avalanchas de eventos (debounce).

---

### Tarea 1: perfil (lectura y validación, pura)

**Ficheros:** crear `crates/codeloom-core/src/registry/mod.rs` y `registry/profile.rs`; `pub mod registry;` en `lib.rs`.

**Interfaces — produce:**
```rust
// profile.rs
pub struct AgentProfile {
    pub name: String, pub description: String, pub version: Option<String>,
    pub persona_role: Option<String>, pub persona_goal: Option<String>, pub persona_backstory: Option<String>,
    pub body: String,                    // cuerpo markdown = prompt de sistema
    pub model: Option<String>, pub effort: Option<String>,
    pub max_turns: Option<u32>, pub max_depth: Option<u32>,
    pub declared_not_applied: Vec<String>,   // claves presentes: "scope", "delegation", "transport", "a2a"
}
pub enum ProfileProblem { InvalidName(String), NameMismatch{ file: String, declared: String },
                          MissingField(&'static str), BadFrontmatter(String), UnknownField(String) }
pub fn parse_profile(file_stem: &str, text: &str) -> Result<(AgentProfile, Vec<ProfileProblem>), ProfileProblem>
// Err = el perfil no se puede cargar. Los avisos (UnknownField, y el de scope/delegation) van en el Vec.
pub fn is_valid_agent_name(name: &str) -> bool   // solo a-z, 0-9 y '-', no vacío, sin '-' inicial/final, ≤ 64
```
`description` es obligatoria (`MissingField("description")`). Frontmatter delimitado por líneas `---`. Campos desconocidos: aviso, no error. CRLF en la entrada se tolera (se normaliza al leer).

- [ ] **Paso 1: tests en rojo:** `parses_a_complete_profile`, `parses_a_minimal_profile`, `unknown_fields_warn_but_load`, `declared_scope_is_reported_as_not_applied`, `rejects_invalid_names` (mayúsculas, `..`, `a/b`, `a\b`, vacío, `-x`), `rejects_name_that_differs_from_file_stem`, `rejects_missing_description`, `rejects_missing_or_unterminated_frontmatter`, `body_is_everything_after_the_frontmatter`, `crlf_input_is_accepted`.
- [ ] **Paso 2: implementar. Paso 3: verificar** `cargo test -p codeloom-core`, clippy, rustfmt, LF. **Paso 4: no commitear.**

---

### Tarea 2: registro, fuente de ficheros, `default` y tarjeta

**Ficheros:** `registry/mod.rs`, `registry/source.rs`.

**Interfaces — consume:** `AgentProfile`, `parse_profile`, `is_valid_agent_name` (tarea 1); `AgentCard`, `AgentSkill` (`card.rs`). **Produce:**
```rust
pub enum ProfileLevel { Global, Project }
pub enum AgentOrigin { BuiltIn, File { path: PathBuf, level: ProfileLevel }, Remote { card_url: String } }
pub struct LocalSpec { pub body: String, pub model: Option<String>, pub effort: Option<String>,
                       pub max_turns: Option<u32>, pub max_depth: Option<u32> }
pub enum Execution { Local(LocalSpec), Remote }
pub struct AgentEntry { pub name: String, pub origin: AgentOrigin, pub card: AgentCard, pub execution: Execution }
pub enum LoadProblem { Profile { path: PathBuf, problem: ProfileProblem }, Shadowed { name: String, by: AgentOrigin, loser: AgentOrigin },
                       Unreadable { path: PathBuf, error: String } }
pub struct SourceLoad { pub entries: Vec<AgentEntry>, pub problems: Vec<LoadProblem> }
pub trait AgentSource: Send + Sync { fn id(&self) -> &str; fn load(&self) -> SourceLoad; }
pub struct FileSource { /* dir, level */ }  impl FileSource { pub fn new(dir: PathBuf, level: ProfileLevel) -> Self }
pub struct BuiltInSource;                   // aporta `default`
pub struct RegistrySnapshot { /* entries por nombre, problems */ }
impl RegistrySnapshot { pub fn get(&self, name: &str) -> Option<&AgentEntry>; pub fn entries(&self) -> Vec<&AgentEntry>; pub fn problems(&self) -> &[LoadProblem] }
pub struct AgentRegistry { /* fuentes en orden de precedencia ascendente */ }
impl AgentRegistry { pub fn new(sources: Vec<Box<dyn AgentSource>>) -> Self; pub fn reload(&self) -> Arc<RegistrySnapshot>; pub fn snapshot(&self) -> Arc<RegistrySnapshot> }
```
Precedencia: las fuentes se pasan de **menor a mayor** (`[BuiltIn, Global, Project]`); el último gana y el vencido se anota `Shadowed`. `default` integrado: `Execution::Local` con `body` vacío y sin modelo ni esfuerzo (se hereda del padre). Tarjeta sintetizada con `AgentCard::local(name, description, version.unwrap_or("0.0.0"))`; **sin** persona, `scope`, `model` ni `effort` en ella; sin skills (decisión provisional de la spec, pregunta abierta). Un directorio inexistente da cero entradas y ningún problema. Solo se leen ficheros `*.md` del directorio (no recursivo).

- [ ] **Paso 1: tests en rojo:** `builtin_default_is_always_present`, `file_source_loads_md_files_only`, `missing_directory_yields_nothing_and_no_problem`, `project_overrides_global_overrides_builtin`, `overridden_agent_is_reported_as_shadowed`, `default_can_be_redefined_by_a_file`, `card_is_synthesized_without_persona_model_or_effort`, `card_is_not_servable`, `invalid_profile_is_reported_and_others_still_load`, `snapshot_is_immutable_across_reload`.
- [ ] **Paso 2: implementar. Paso 3: verificar** como en la tarea 1. **Paso 4: no commitear.**

---

### Tarea 3: recarga en caliente y evento

**Ficheros:** `registry/watch.rs`, `registry/mod.rs`; `event/mod.rs` (variante nueva).

**Interfaces — produce:**
```rust
// event: nueva variante en el enum de eventos existente
Event::AgentRegistryChanged { added: Vec<String>, removed: Vec<String>, changed: Vec<String>, problems: usize }
// registry
impl AgentRegistry {
    pub fn reload_and_publish(&self, bus: &EventBus) -> Arc<RegistrySnapshot>   // recarga y emite el evento solo si hubo diferencias
    pub fn spawn_watcher(self: &Arc<Self>, dirs: Vec<PathBuf>, bus: EventBus) -> WatcherHandle  // al soltarlo, se detiene
}
```
**Última versión válida:** si un fichero que antes cargaba pasa a ser inválido o ilegible, la entrada anterior **se conserva** y se añade el `LoadProblem`; si el fichero se borra, la entrada se retira. Vigilancia con *debounce* (≥ 200 ms) o sondeo de la fecha de modificación; en ningún caso en el hilo de la interfaz. Usa el nombre real del tipo del bus de eventos del crate (`grep` de `EventBus`); informa.

- [ ] **Paso 1: tests en rojo:** `new_file_is_added_and_published`, `edited_file_is_reported_as_changed`, `deleted_file_is_removed`, `broken_edit_keeps_the_last_valid_version_and_reports_the_problem`, `unchanged_reload_publishes_nothing`, `watcher_picks_up_a_new_file` (con espera acotada, sin dormir a ciegas), `dropping_the_watcher_stops_it`.
- [ ] **Paso 2: implementar. Paso 3: verificar** como antes. **Paso 4: no commitear.**

---

### Tarea 4: aprobación de los agentes de proyecto

**Ficheros:** `registry/trust.rs`, `registry/mod.rs`.

**Interfaces — produce:**
```rust
pub struct TrustStore { /* ruta del fichero de datos */ }
impl TrustStore {
    pub fn open(data_dir: PathBuf) -> Result<Self>;                         // `<data_dir>/agent-trust.json`
    pub fn is_approved(&self, project_root: &Path, name: &str, content_hash: &str) -> bool;
    pub fn approve(&self, project_root: &Path, name: &str, content_hash: &str) -> Result<()>;
    pub fn revoke(&self, project_root: &Path, name: &str) -> Result<()>;
}
pub fn content_hash(text: &str) -> String   // huella estable entre ejecuciones y versiones (no `DefaultHasher`)
pub enum Approval { NotRequired, Approved, Pending }
impl RegistrySnapshot { pub fn approval(&self, name: &str) -> Option<Approval> }
```
`AgentEntry` gana `pub content_hash: Option<String>` (solo ficheros). `AgentRegistry` recibe `with_trust(TrustStore, project_root: PathBuf, trust_all_projects: bool)`: el nivel `Global` y `BuiltIn` son `NotRequired`; `Project` es `Approved` si `trust_all_projects` o si la huella está aprobada, `Pending` si no. La raíz se canoniza. Un fichero ilegible o JSON corrupto del almacén cuenta como «nada aprobado», nunca como pánico.

- [ ] **Paso 1: tests en rojo:** `global_and_builtin_do_not_need_approval`, `project_agent_is_pending_until_approved`, `approval_is_invalidated_by_a_content_change`, `approval_is_per_project_root`, `revoke_returns_it_to_pending`, `trust_all_projects_skips_the_approval`, `approvals_survive_reopening_the_store`, `corrupt_trust_file_means_nothing_is_approved`, `content_hash_is_stable_and_sensitive_to_one_byte`.
- [ ] **Paso 2: implementar. Paso 3: verificar** como antes. **Paso 4: no commitear.**

---

### Tarea 5: el descriptor recuerda con qué agente arrancó

**Ficheros:** `session/mod.rs`, `session/sqlite.rs`, `subagent.rs` y lo que el compilador exija (literales de `SubagentDescriptor` en `codeloom-tui`).

**Interfaces — produce:** `SubagentDescriptor` gana `pub agent: Option<String>` y `pub agent_origin: Option<String>` (texto: `built-in`, `global:<ruta>`, `project:<ruta>`, `remote:<url>`). SQLite: columnas `agent` y `agent_origin` añadidas de forma idempotente (`PRAGMA table_info`); filas antiguas dan `None`.

- [ ] **Paso 1: tests en rojo:** `agent_name_and_origin_roundtrip_through_sqlite`, `old_rows_without_agent_columns_read_as_none`, `agent_columns_migration_is_idempotent`.
- [ ] **Paso 2: implementar** (todos los productores existentes pasan `None`). **Paso 3: verificar** build, tests de los cuatro crates, clippy, rustfmt, LF. **Paso 4: no commitear.**

---

### Tarea 6: `start` recibe un agente

**Ficheros:** `subagent.rs` (trait `SubagentLauncher` y `InProcessSubagentLauncher`), el lugar donde la herramienta de delegar construye la llamada a `start` (haz `grep` de `.start(`), `codeloom-tui`/`codeloom-cli` si el compilador lo exige.

**Interfaces — produce:**
```rust
async fn start(&self, parent: SessionId, label: String, prompt: String,
               agent: Option<String>, cancel: CancellationToken) -> Result<(SessionId, JobId)>;
impl InProcessSubagentLauncher { pub fn with_registry(mut self, registry: Arc<AgentRegistry>) -> Self }
```
Comportamiento: `None` = `default`. Sin registro configurado, `Some(nombre)` distinto de `default` es error de validación. Resolución en la instantánea **vigente al arrancar**; la entrada se copia por valor (una recarga posterior no la altera). Errores de validación **sin crear tarea**: agente inexistente, agente `Pending` (mensaje que diga «pendiente de aprobación») y `Execution::Remote` (todavía sin transporte: «agente remoto no disponible»). Se aplica a la sesión hija: el `body` como prompt de sistema, `model`/`effort` del perfil con herencia del padre si faltan, `max_turns` y `max_depth`. Un `model` que no exista en el catálogo: error claro. El descriptor guarda `agent` y `agent_origin`. Averigua en el código cómo recibe hoy la sesión hija su modelo y prompt de sistema y reutiliza ese camino; informa de dónde.

- [ ] **Paso 1: tests en rojo:** `start_without_agent_uses_default_and_inherits_model`, `start_with_a_file_agent_applies_persona_model_and_effort`, `start_with_missing_model_in_profile_inherits_from_parent`, `start_with_unknown_agent_creates_no_task`, `start_with_pending_project_agent_creates_no_task_and_says_so`, `start_with_remote_agent_creates_no_task`, `reload_after_start_does_not_change_the_running_task`, `descriptor_records_agent_name_and_origin`, `start_with_a_named_agent_without_registry_is_a_validation_error`.
- [ ] **Paso 2: implementar. Paso 3: verificar** build, tests de los cuatro crates, clippy, rustfmt, LF; `grep -rn "\.start(" crates` sin llamadas con la firma antigua. **Paso 4: no commitear.**

---

### Tarea 7: documentación

**Ficheros:** `docs/specs/2026-10-22-codeloom-a2a-agent-registry-design.md`, `docs/specs/2026-10-15-loom-agent-profile-a2a.md`, `docs/plans/2026-10-22-codeloom-a2a-roadmap.md`.

- [ ] **Paso 1:** `## Estado (fecha)` en la spec del registro con lo implementado (commits de `git log`), decisiones tomadas que la spec no fijaba, y limitaciones reales (`scope` no aplicado; aprobación sin diálogo de interfaz; sin fuente remota; sin edición desde la TUI; `skills[]` de la tarjeta vacío; validación visual pendiente). Nota fechada en la spec del perfil. En la hoja de ruta, A13 [x] con commits y lo abierto.
- [ ] **Paso 2: verificar** LF. **Paso 3: no commitear.**
