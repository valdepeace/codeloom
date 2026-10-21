# Actividad de jobs y subagentes en el transcript Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** En el look Crush, jobs y subagentes aparecen en el transcript como elementos de actividad con estado, progreso y resultado, y sus avisos de texto dejan de pintarse como mensajes "You".

**Architecture:** Un `LineKind::ActivityHead(ToolStatus)` y un constructor puro `activity_lines` en `batten`; `App` mantiene entradas de actividad (mapa clave → entrada) igual que `tool_entries`, alimentadas por `JobChanged`/`SubagentChanged` y por los avisos `[Job …]`/`[Subagent …]`.

**Tech Stack:** Rust 2021, ratatui. Sigue el patrón de `docs/plans/2026-10-20-codeloom-look-presets.md` (tools inline).

**Spec:** `docs/specs/2026-10-20-codeloom-look-presets-design.md` (siguiente paso del apartado "No objetivos").

## Global Constraints

- Tras cada tarea: `cargo build`, `cargo test`, `cargo clippy -- -D warnings` sin avisos nuevos, y `rustfmt --edition 2021 --check` sobre los ficheros tocados (nunca `cargo fmt --all`).
- Entorno: `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe' CARGO_TARGET_DIR='E:/workspace/personal/loom/target' CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`. El disco `E:` está casi lleno: **no crear otros directorios `target*`**.
- **No hacer commits ni `git add`.** Prohibido `git checkout/restore/stash/reset`.
- `codeloom-core` no se toca. Sin red en tests.
- El look Classic no cambia: sus avisos de texto y su transcript quedan exactamente como hoy (el test `scoped_subagent_notices_only_reach_their_session` debe seguir verde sin tocarlo).
- Comentarios en español de registro cuidado, sin andalucismos ni tacos.
- Los avisos siguen guardándose y llegando al modelo como mensajes de usuario; solo cambia cómo los pinta la TUI en Crush.

## Review Focus

1. Dos subagentes con la misma etiqueta al restaurar una sesión: cada aviso crea su propio elemento y ninguno se descarta por error (T2: `restored_notices_with_the_same_label_each_create_an_item`).
2. Aviso que llega antes que el evento de estado: no puede duplicar el elemento (T2: `notice_before_event_does_not_duplicate`).
3. Jobs de otra sesión y jobs con `kind == "subagent"` no generan elemento (T2: `foreign_and_subagent_kind_jobs_are_ignored`).
4. Progreso fuera de rango (fracción <0, >1 o NaN) no rompe el texto (T1: `progress_is_clamped_to_a_percentage`).
5. Cambiar de look con elementos de actividad ya creados los oculta y los restaura (T2: `switching_look_hides_and_restores_activity`).

---

### Task 1: `batten`: `ActivityHead` y `activity_lines`

**Files:**
- Modify: `crates/batten/src/state/transcript.rs` (variante de `LineKind`)
- Modify: `crates/batten/src/state/tool_lines.rs` (extraer helper de plegado)
- Create: `crates/batten/src/state/activity_lines.rs`
- Modify: `crates/batten/src/state/mod.rs` (mod + re-export)
- Modify: `crates/batten/src/widgets/transcript.rs` (`prefix`, `kind_token`)

**Interfaces:**
- Produces en `batten::state`: `LineKind::ActivityHead(ToolStatus)`; `struct ActivityView<'a> { pub kind: &'a str, pub label: &'a str, pub status: ToolStatus, pub progress: Option<(f64, &'a str)>, pub body: Option<&'a str> }`; `fn activity_lines(view: &ActivityView) -> Vec<Line>`.
- Prefijo del widget: `ActivityHead(_)` → `"◆ "`; token como `ToolHead` del mismo estado (Pending→ToolPending, Running→ToolRunning, Success→ToolSuccess, Error→ToolError, Canceled→ToolCanceled, AwaitingPermission→Warning).
- Formato: cabecera `ActivityHead(view.status)` con texto `"{kind}  {label}"`, spans: `kind` → `StyleToken::ToolName`, `label` → `StyleToken::Muted` (offsets en bytes). Cuerpo:
  1. si `status == Running` y hay `progress`: una línea `ToolBody` `"{pct}% {etiqueta}"` con `pct = (fracción.clamp(0.0,1.0) * 100.0).round()` (NaN cuenta como 0) y sin la etiqueta ni el espacio final si esta está vacía;
  2. si `body` es `Some` y no vacío (tras `trim`): sus líneas como `ToolBody`; si hay más de 6, solo las 6 primeras más una `ToolMore` con texto `"… (N more lines)"` (`N` = restantes). Sin pista de teclas.

- [ ] **Step 1: Tests.** En `activity_lines.rs` (`mod tests`), con helper `fn view<'a>(status: ToolStatus) -> ActivityView<'a>` (kind `"job"`, label `"build"`, resto `None`):

```rust
#[test]
fn header_carries_kind_label_status_and_spans() {
    let lines = activity_lines(&view(ToolStatus::Running));
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].kind, LineKind::ActivityHead(ToolStatus::Running));
    assert_eq!(lines[0].plain(), "job  build");
    assert_eq!((lines[0].spans[0].start, lines[0].spans[0].len), (0, 3));
    assert_eq!(lines[0].spans[0].token, StyleToken::ToolName);
    assert_eq!(lines[0].spans[1].token, StyleToken::Muted);
}

#[test]
fn running_job_shows_progress_percentage() {
    let mut v = view(ToolStatus::Running);
    v.progress = Some((0.5, "compiling"));
    let lines = activity_lines(&v);
    assert_eq!(lines[1].kind, LineKind::ToolBody);
    assert_eq!(lines[1].plain(), "50% compiling");
    v.progress = Some((0.25, ""));
    assert_eq!(activity_lines(&v)[1].plain(), "25%");
}

#[test]
fn progress_is_clamped_to_a_percentage() {
    let mut v = view(ToolStatus::Running);
    v.progress = Some((7.0, "x"));
    assert_eq!(activity_lines(&v)[1].plain(), "100% x");
    v.progress = Some((-1.0, "x"));
    assert_eq!(activity_lines(&v)[1].plain(), "0% x");
    v.progress = Some((f64::NAN, "x"));
    assert_eq!(activity_lines(&v)[1].plain(), "0% x");
}

#[test]
fn progress_is_hidden_once_settled() {
    let mut v = view(ToolStatus::Success);
    v.progress = Some((0.5, "x"));
    assert_eq!(activity_lines(&v).len(), 1);
}

#[test]
fn long_result_is_folded_to_six_lines_plus_a_count() {
    let body = (1..=9).map(|n| n.to_string()).collect::<Vec<_>>().join("\n");
    let mut v = view(ToolStatus::Success);
    v.body = Some(&body);
    let lines = activity_lines(&v);
    assert_eq!(lines.len(), 1 + 6 + 1);
    assert_eq!(lines[7].kind, LineKind::ToolMore);
    assert_eq!(lines[7].plain(), "… (3 more lines)");
}

#[test]
fn blank_body_adds_nothing() {
    let mut v = view(ToolStatus::Error);
    v.body = Some("  \n ");
    assert_eq!(activity_lines(&v).len(), 1);
}
```
Y en `widgets/transcript.rs` (`mod tests`), siguiendo `tool_entries_paint_a_prefix_that_is_not_part_of_the_text`: una entrada `push_lines(activity_lines(..))` pinta `"◆ job"` y `plain()` no contiene `◆`.

- [ ] **Step 2:** `cargo test -p batten activity` → no compila.
- [ ] **Step 3: Implementar.** Añadir la variante a `LineKind` (junto a `ToolHead`), `prefix`/`kind_token` en `widgets/transcript.rs` (reutilizar el mapeo de `ToolHead`; si hay un helper, compartirlo en vez de duplicar el `match`), y `activity_lines`. Recorte de `body`: `trim()` y `lines()`. Registrar `pub mod activity_lines;` y `pub use activity_lines::{activity_lines, ActivityView};` en `state/mod.rs`. No es necesario extraer nada de `tool_lines.rs` si el plegado de actividad (6 líneas, sin pista) no comparte código útil con el de tools; no toques `tool_lines.rs` salvo necesidad.
- [ ] **Step 4:** `cargo test -p batten` verde; `rustfmt --edition 2021 --check` de los ficheros tocados; `cargo clippy -p batten -- -D warnings`; `cargo build -p batten --examples`.

---

### Task 2: `App`: elementos de actividad y avisos

**Files:**
- Modify: `crates/codeloom-tui/src/app.rs` (solo; no hace falta tocar `render.rs` ni `run.rs`)

**Interfaces:**
- Consumes: T1 (`ActivityView`, `activity_lines`, `LineKind::ActivityHead`); `Transcript::{push_lines, replace}`; tipos de core: `codeloom_core::job::{JobSnapshot, JobStatus, JobProgress, JobResult}`, `codeloom_core::session::{SubagentDescriptor, SubagentStatus}`.
- Produces en `App`: campo privado `activity_entries: Vec<ActivityEntry>` con
  `struct ActivityEntry { key: String, id: EntryId, lines: Vec<Line>, kind: &'static str, label: String, from_notice: bool, notice_consumed: bool }`
  (`key` = `"job:{id.0}"` o `"subagent:{child.0}"` para entradas en vivo; `"notice:{n}"` con `n` creciente para las nacidas de un aviso). Métodos privados: `sync_activity(&mut self, key: &str, kind: &'static str, label: &str, lines: Vec<Line>, from_notice: bool)` y `apply_notice(&mut self, text: &str) -> bool`. `sync_all_tools` también aplica la visibilidad a las entradas de actividad (renómbralo o amplía su doc; mantén el nombre público `sync_all_tools`).

Comportamiento:
- **Visibilidad:** igual que las tools: `self.tool_inline()`. Las entradas se crean siempre; con look Classic nacen vacías (0 líneas) y no ponen `self.assistant = None`; con Crush llevan las líneas y ponen `self.assistant = None`. `sync_activity` no reemplaza si las líneas no cambiaron (compara con la caché).
- **Mapeo de estado a `ToolStatus`:** `JobStatus::Running | Stopping` → `Running`; `Completed` → `Success`; `Failed` → `Error`; `Killed` → `Canceled`. `SubagentStatus::Running` → `Running`; `Completed` → `Success`; `Failed` → `Error`; `Killed | Interrupted` → `Canceled`.
- **`JobChanged(snapshot)`:** tras la lógica existente, si `snapshot.kind != "subagent"` y `Some(snapshot.owner) == self.session_id`: construir `ActivityView { kind: "job", label: &snapshot.label, status, progress: snapshot.progress.as_ref().map(|p| (p.fraction, p.label.as_str())), body }` donde `body` solo cuando el estado no es `Running`: `snapshot.detail` o, si no hay, `snapshot.result.content`. Llamar `sync_activity("job:{id}", …)`.
- **`SubagentChanged(d)`:** tras la lógica existente, si `Some(d.parent) == self.session_id`: vista `kind: "subagent"`, `label: &d.label`, `body: d.result` cuando no está `Running`; `sync_activity("subagent:{child}", …)`.
- **Avisos** (`Role::User` cuyo texto empieza por `"[Job "` o `"[Subagent "`), solo con look Crush (`tool_inline()`), tanto en `apply_message_added` como en `restore_messages` (y por tanto `load_session`): llamar `apply_notice(&text)`; si devuelve `true` **no** llamar a `push_user`. Con Classic: `push_user` como hoy (no se llama a `apply_notice`).
  - `apply_notice` interpreta `"[Job {id} {status}] {label}\n{cuerpo}"` y `"[Subagent {label} completed]\n{cuerpo}"` (la primera línea hasta el `]` es el encabezado; el resto, el cuerpo; `status` ∈ `completed|killed|failed` → `Success|Canceled|Error`). Si el encabezado no se puede interpretar, devuelve `false` (se pinta como mensaje normal).
  - **Descarte:** Job: si existe entrada con `key == "job:{id}"` → `true` sin crear nada. Subagent: si existe una entrada **en vivo** (`!from_notice`) con `kind == "subagent"`, misma etiqueta y `!notice_consumed` → marcarla `notice_consumed = true` y devolver `true`. Si no hay coincidencia → crear una entrada con `from_notice = true`, clave `"notice:{n}"`, estado derivado del aviso y el cuerpo, y devolver `true`.
  - Una entrada creada por un aviso nunca se descarta ni se consume por otro aviso.
- `restore_messages` vacía `activity_entries` junto con `tool_entries`.

- [ ] **Step 1: Tests** en `app.rs` (`mod tests`, con `crush_app()` ya existente; construir `JobSnapshot`/`SubagentDescriptor` leyendo sus definiciones en `codeloom-core`, con un helper `fn job(id: &str, status: JobStatus, owner: SessionId) -> JobSnapshot` y `fn sub(label: &str, status: SubagentStatus, parent: SessionId) -> SubagentDescriptor`; en todos los tests `app.session_id = Some(SessionId(1))`):

```rust
#[test]
fn job_activity_appears_and_updates_in_place() {
    // Running con progress (0.5, "building"): existe una línea que empieza por "job  " y otra "50% building".
    // Segundo JobChanged del mismo id, Completed con result "ok": el nº de entradas no cambia,
    // aparece una línea "ok" y desaparece "50% building".
}

#[test]
fn subagent_activity_shows_label_status_and_result() {
    // SubagentChanged Running -> línea "subagent  review" con kind ActivityHead(Running).
    // Luego Completed con result Some("listo") -> misma entrada, kind ActivityHead(Success), línea "listo".
}

#[test]
fn foreign_and_subagent_kind_jobs_are_ignored() {
    // Job con owner SessionId(2) y job con kind "subagent": transcript.len() == 0 y entry_count() == 0.
}

#[test]
fn live_notice_is_suppressed_when_the_item_exists() {
    // Subagent live Completed "review"/"listo"; luego SessionMessageAdded{session 1, Message::user("[Subagent review completed]\nlisto")}.
    // Ninguna línea del transcript tiene kind LineKind::User; entry_count() no aumenta.
    // Lo mismo con un job: aviso "[Job j1 completed] build\nok" tras el JobChanged de j1.
}

#[test]
fn notice_before_event_does_not_duplicate() {
    // Aviso "[Subagent review completed]\nlisto" primero (crea elemento from_notice),
    // después SubagentChanged Completed "review": hay exactamente DOS entradas de actividad
    // (el contrato lo permite: la del aviso y la viva) y ninguna línea LineKind::User.
}

#[test]
fn restored_notices_become_items_not_user_messages() {
    // restore_messages con tema Crush: [Message::user("hola"), Message::user("[Subagent review completed]\nready"),
    //   Message::user("[Job j1 failed] build\nboom")].
    // Solo la primera línea es LineKind::User ("hola"); hay un ActivityHead(Success) "subagent  review" con cuerpo "ready"
    // y un ActivityHead(Error) "job  build" con cuerpo "boom"; el orden es hola, subagent, job.
}

#[test]
fn restored_notices_with_the_same_label_each_create_an_item() {
    // Dos avisos "[Subagent review completed]" con cuerpos "uno" y "dos" al restaurar: aparecen ambos cuerpos.
}

#[test]
fn unparseable_notice_falls_back_to_a_plain_message() {
    // Message::user("[Job sin cerrar") con Crush -> línea LineKind::User con ese texto.
}

#[test]
fn classic_look_keeps_notices_as_user_messages_and_hides_activity() {
    // App::new() (Classic): SubagentChanged Running + aviso "[Subagent review completed]\nready":
    // el aviso se pinta como antes (líneas "[Subagent review completed]" y "ready" de kind User)
    // y ninguna línea del transcript es ActivityHead.
}

#[test]
fn switching_look_hides_and_restores_activity() {
    // Crush: SubagentChanged crea el elemento; cambiar a Batten + sync_all_tools lo oculta
    // (sin líneas ActivityHead); volver a Crush + sync_all_tools lo restaura.
}
```
Cada test debe escribirse completo con aserciones reales (los comentarios arriba son la especificación, no el código).

- [ ] **Step 2:** `cargo test -p codeloom-tui activity` → fallan.
- [ ] **Step 3: Implementar** según "Comportamiento".
- [ ] **Step 4:** `cargo test` del workspace (todo verde, incluidos los tests previos de tools y avisos), `cargo clippy -- -D warnings`, `rustfmt --edition 2021 --check crates/codeloom-tui/src/app.rs`. Si rustfmt reformatea código ajeno, deshazlo solo en esos hunks.

---

## Validación manual (la hace el usuario)

En Crush, lanzar un job largo (`job_start`) y un subagente: ver el elemento `◆` actualizándose en su sitio con progreso y color, el resultado plegado a 6 líneas al terminar, y que el aviso de texto no aparece como "You". Reabrir la sesión con `/sessions` y comprobar que los avisos se ven como elementos. En Classic, todo igual que antes.
