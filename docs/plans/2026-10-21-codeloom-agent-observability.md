# Observabilidad de agentes (Fase O1) — Plan de implementación

> **Para agentes:** sub-skill requerida: `superpowers:subagent-driven-development` (recomendada) o `superpowers:executing-plans`. Los pasos usan casillas `- [ ]`. Una tarea cada vez, en orden. Al terminar y verificar cada tarea se commitea (regla del usuario).

**Meta:** que la TUI vea el interior de cada subagente (paso, herramienta en curso, tokens, tiempo, estado) y lo muestre en el rail, con totales de la jerarquía.

**Arquitectura:** el lanzador reenvía al bus del padre, envueltos en `LiveEvent::ChildEvent`, los eventos relevantes del bus aislado de cada hijo. La TUI mantiene `AgentStats` por hijo alimentado solo por ese envoltorio y el rail lo pinta.

**Stack:** Rust, `tokio` (broadcast), `ratatui`.

**Spec:** `docs/specs/2026-10-21-codeloom-agent-observability-design.md` (leerla antes).

## Restricciones globales

- `codeloom-core` no depende de `ratatui`/`crossterm`. Sin red en tests.
- Comprobaciones obligatorias: `cargo build`, `cargo test`, `cargo fmt --all --check`, `cargo clippy -- -D warnings`.
- Entorno de build (Git Bash): `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe' CARGO_TARGET_DIR='E:/workspace/personal/loom/target' CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`. No crear otros `target*`. Formatear solo con `rustfmt --edition 2021 <ruta>`. Ficheros en LF.
- Comentarios en español «pijo», sin tacos ni andalucismos.
- La fluidez del composer es prioridad máxima: nada de temporizadores nuevos ni trabajo extra en el bucle.

## Review Focus

- Un hijo verboso no debe saturar el bus del padre: solo se reenvían los siete eventos del filtro y un `Lagged` se tolera sin pánico.
- La tarea de reenvío debe terminar cuando el hijo termina (bus cerrado); sin tareas huérfanas.
- Los eventos de un hijo nunca deben alterar el transcript, `turn_in_flight`, `status` ni los totales propios del padre.
- Un `ChildEvent` de un hijo que no es de la sesión abierta se ignora.
- Los tokens de un hijo suman a su `AgentStats` y al total de la jerarquía, no al contador del turno del padre.
- Cifras enormes o etiquetas largas no rompen la anchura de 32 columnas del rail.

---

### Tarea 1: núcleo — `ChildEvent` y reenvío filtrado

**Ficheros:** `crates/codeloom-core/src/event/mod.rs`, `crates/codeloom-core/src/subagent.rs`, brazo mínimo que ignore el evento en `crates/codeloom-tui/src/app.rs` (la tarea 2 lo sustituye).

**Interfaces — produce:**
```rust
// event/mod.rs
LiveEvent::ChildEvent { parent: SessionId, child: SessionId, event: Box<LiveEvent> },
impl LiveEvent { pub fn is_child_relevant(&self) -> bool }  // true para TurnStarted, StepStarted, ToolStarted, ToolFinished, Usage, TurnEnded, Error
```
En `InProcessSubagentLauncher::start`: el bus del hijo (`EventBus::new(64)` actual) pasa a `EventBus::new(256)`; **antes** de lanzar el hijo se hace `let mut rx = child_bus.subscribe()` y se lanza una tarea que, mientras `rx.recv()` devuelva `Ok(ev)` y `ev.is_child_relevant()`, publica `LiveEvent::ChildEvent { parent, child, event: Box::new(ev) }` en `self.bus`; ante `Lagged` continúa; ante `Closed` termina.

- [ ] **Paso 1: tests en rojo** (indicar si el rojo es de compilación o de aserción):
  - `event/mod.rs`: `child_relevance_filter_excludes_deltas` (`Delta`, `ReasoningDelta`, `MessageAdded`, `JobChanged` → false; los siete del filtro → true).
  - `subagent.rs`: `child_events_are_forwarded_tagged_with_the_child` (hijo con proveedor guionizado que llama a una herramienta y responde; el bus del padre recibe `ChildEvent` con `child` correcto que envuelve `ToolStarted`, `ToolFinished` y `TurnEnded`, y ningún `ChildEvent` envuelve un `Delta`) y `forwarder_ends_when_the_child_finishes` (tras `jobs.wait`, el número de tareas no crece: comprobar que `rx` del bus del padre deja de recibir `ChildEvent` tras el último y que no se cuelga el runtime al soltar el lanzador).
- [ ] **Paso 2: implementar.** Brazo `LiveEvent::ChildEvent { .. } => {}` provisional en `App::apply`.
- [ ] **Paso 3: verificar** `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --all --check`. **Paso 4: commit** (español pijo + trailer).

---

### Tarea 2: TUI — `AgentStats`

**Ficheros:** `crates/codeloom-tui/src/app.rs`.

**Interfaces — produce:**
```rust
#[derive(Debug, Clone, Default)]
pub struct AgentStats {
    pub step: usize,
    pub max_steps: usize,
    pub tool: Option<String>,        // herramienta en curso (nombre)
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub started_at: Option<std::time::Instant>,
    pub finished_at: Option<std::time::Instant>,
}
// App
pub agent_stats: std::collections::HashMap<i64, AgentStats>,   // clave: child.0
pub fn hierarchy_tokens(&self) -> (u64, u64);                  // suma de todos los hijos de la sesión abierta
pub fn active_agents(&self) -> usize;                          // subagentes en marcha de la sesión abierta
```
Reglas de `apply(ChildEvent { parent, child, event })`, solo si `Some(parent) == self.session_id`: `TurnStarted` → crea/reinicia stats con `started_at = Some(Instant::now())`; `StepStarted { step, max_steps }` → actualiza; `ToolStarted { name, .. }` → `tool = Some(name)`; `ToolFinished` → `tool = None`; `Usage { input_tokens, output_tokens }` → suma; `TurnEnded` / `Error` → `finished_at = Some(now)`, `tool = None`. `ToolStarted` solo lleva `call_id` y `name`: comprobar los campos reales. Se limpia `agent_stats` en `load_session` junto con `subagents`.

- [ ] **Paso 1: tests en rojo:** `child_events_feed_the_stats` (secuencia completa y aserciones de cada campo), `child_usage_accumulates_and_does_not_touch_the_parents_counters` (`turn_input_tokens`/`turn_output_tokens` del padre intactos), `child_events_of_another_session_are_ignored`, `child_events_do_not_touch_transcript_or_status` (`status`, `turn_in_flight`, `transcript.len()` iguales), `hierarchy_tokens_sums_all_children`, `stats_are_cleared_on_session_load`.
- [ ] **Paso 2: implementar.** **Paso 3: verificar** suite completa, clippy, fmt. **Paso 4: commit.**

---

### Tarea 3: TUI — métricas en el rail

**Ficheros:** `crates/codeloom-tui/src/rail.rs`.

**Interfaces — produce:** `pub fn format_tokens(n: u64) -> String` (`0`→`0`, `999`→`999`, `1_200`→`1.2k`, `15_000`→`15k`, `1_500_000`→`1.5M`), `pub fn format_elapsed(d: Duration) -> String` (`7s`, `65s`→`1m05s`, `3_700s`→`1h01m`) y `fn metrics_line(stats: &AgentStats, now: Instant, width: usize) -> String` que compone `"{step}/{max} {tool} · {in}/{out} · {elapsed}"` omitiendo lo que no exista (sin herramienta, sin paso aún) y **recortando por anchura** con `…`. En la sección SUBAGENTS: cada subagente con stats gana una segunda línea tenue (`theme.styles.muted`) con sangría de dos espacios y esa métrica; el título pasa a `SUBAGENTS · {activos} · {in}/{out}` (con los totales de `hierarchy_tokens`) cuando hay actividad. El tiempo usa `finished_at` si existe, si no `now`. Los subagentes sin stats siguen con una sola línea.

- [ ] **Paso 1: tests en rojo:** `format_tokens_scales`, `format_elapsed_scales`, `metrics_line_omits_missing_parts`, `metrics_line_truncates_to_width` (anchura 30, etiquetas de herramienta de 50 caracteres: nunca supera 30 columnas y acaba en `…`), `rail_shows_metrics_second_line_for_a_subagent_with_stats` (render con `TestBackend` 40x24 como el test existente `rail_marks_subagents_waiting_for_input`), `rail_title_shows_hierarchy_totals`.
- [ ] **Paso 2: implementar** sin temporizadores nuevos ni cambios en el cálculo de `animating`. **Paso 3: verificar** suite completa, clippy, fmt. **Paso 4: commit.**

---

### Tarea 4: documentación

- [ ] Sección «Estado» en `docs/specs/2026-10-21-codeloom-agent-observability-design.md` (qué se implementó, decisiones al ejecutar, límites conocidos, fases O2/O3 pendientes) y nota en `2026-10-21-codeloom-agent-autonomy-design.md` de que la observabilidad precede a las fases B y C. **Commit.**

## Autoauditoría

- Cobertura de la spec: objetivos 1 (T1), 2 (T2+T3), 3 (T2+T3), 4 (T3); anti-objetivos en Review Focus.
- Tipos coherentes: `ChildEvent`, `is_child_relevant`, `AgentStats`, `agent_stats`, `hierarchy_tokens`, `active_agents`, `format_tokens`, `format_elapsed`, `metrics_line` se nombran igual en todas las tareas.
- Riesgo: validación visual en terminal real pendiente.
