# Elección del especialista al delegar (A12) — Plan de implementación

> **Para agentes:** una tarea cada vez, en orden. Quien supervisa commitea, así que **no commitees**.

**Meta:** que el padre pregunte a la persona usuaria cuando duda entre varios agentes (regla en su prompt, configurable) y que la interfaz muestre siempre qué agente lleva cada hijo.

**Arquitectura:** la regla es texto del prompt generado en `RegistrySnapshot::prompt_section`, con un modo leído de `[agents] confirm_delegation`. La visibilidad es una etiqueta de agente en el rail y en la cabecera de actividad de la TUI. No se toca el bucle de entrada ni el composer.

**Spec:** `docs/specs/2026-10-22-codeloom-a2a-delegation-choice-design.md` (**leerla entera antes**). Manda: A2A **no** fija cómo se elige agente, así que todo es política local y no se presenta como del protocolo. La regla del prompt es una petición al modelo, no una garantía. **La observabilidad es criterio de aceptación** (decisión del usuario, 2026-10-22): el agente de cada hijo debe verse siempre que se conozca, incluido `default`.

## Restricciones globales

- `codeloom-core` no depende de `ratatui`/`crossterm`. Errores de librería con `thiserror`. **Sin red en tests.**
- Comprobaciones: `cargo build`, `cargo test -p codeloom-core -p codeloom-tui -p codeloom-cli`, `cargo clippy -p codeloom-core -p codeloom-tui -p codeloom-cli -- -D warnings`, `rustfmt --edition 2021 --check <ficheros tocados>`.
- Entorno de build (Git Bash): `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe' CARGO_TARGET_DIR='E:/workspace/personal/loom/target' CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`. No crear otros `target*`. Formatear solo con `rustfmt --edition 2021 <ruta>`. Ficheros en LF.
- Comentarios en español «pijo», sin tacos ni andalucismos; textos del prompt en inglés, como el resto. No `git checkout/restore/stash/reset`. No tocar `codeloom.toml`.

## Review Focus

- Un `confirm_delegation` desconocido da error de configuración claro, no un valor por defecto silencioso.
- Sin sección `[agents]` o sin la clave, el modo es `Doubt`.
- Cada modo escribe su regla y **solo** la suya en el prompt.
- Un hijo sin agente conocido (`agent: None`, filas antiguas) no muestra etiqueta ni corchetes vacíos.
- La etiqueta no sustituye ni altera el estado A2A entre paréntesis de la cabecera.

---

### Tarea 1: el modo de confirmación en la configuración

**Ficheros:** `crates/codeloom-core/src/config/mod.rs`.

**Produce:**
```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfirmDelegation { #[default] Doubt, Always, Never }
// y en AgentsConfig: pub confirm_delegation: ConfirmDelegation   (cubierto por #[serde(default)])
```
Reexportar donde ya se reexporta `AgentsConfig`, si procede. Un valor desconocido debe fallar en `AgentsConfig::from_toml` con `CoreError::Config` (serde ya lo da).

- [ ] **Paso 1: tests en rojo** (módulo de tests existente de `config`): `confirm_delegation_defaults_to_doubt` (texto sin `[agents]` y con `[agents]` sin la clave), `confirm_delegation_reads_the_three_modes` (`"doubt"`, `"always"`, `"never"`), `confirm_delegation_rejects_unknown_values` (`"sometimes"` → `Err(CoreError::Config(_))`).
- [ ] **Paso 2: implementar. Paso 3: verificar** tests de `codeloom-core`, clippy, rustfmt, LF. **Paso 4: no commitear.**

---

### Tarea 2: la regla en el prompt del padre

**Ficheros:** `crates/codeloom-core/src/registry/mod.rs`, `crates/codeloom-core/src/agent/mod.rs`, `crates/codeloom-cli/src/main.rs`.

**Consume:** `ConfirmDelegation` (tarea 1). **Produce:**
```rust
impl RegistrySnapshot { pub fn prompt_section(&self, confirm: ConfirmDelegation) -> String }
impl AgentLoop { pub fn with_delegation_confirm(mut self, confirm: ConfirmDelegation) -> Self }  // por defecto Doubt
```
`prompt_section` añade, tras la lista, **una** de estas reglas (texto fijado, en inglés):

- `Doubt`: `Choosing an agent: if one agent clearly fits a task, or only \`default\` exists, delegate directly and say in your reply which agent you chose and why. If more than one agent could fit and you are not sure, call \`ask_user\` first, naming the agent you would pick and why, and wait for the answer before calling \`subagent\`.`
- `Always`: `Choosing an agent: before calling \`subagent\`, call \`ask_user\` with your proposed agent for each task and the reason, and wait for the answer. Do not start any subagent until the user agrees.`
- `Never`: `Choosing an agent: pick the best-fitting agent yourself and delegate directly. Say in your reply which agent you chose and why.`

`AgentLoop` guarda el modo y lo pasa a `prompt_section` en `run_turn`. En `main.rs`, `.with_delegation_confirm(agents_cfg.confirm_delegation)` junto a `with_agent_registry`.

- [ ] **Paso 1: tests en rojo.** Actualizar los dos tests existentes de `prompt_section`/del extremo a extremo a la nueva firma. Nuevos: `prompt_section_carries_only_the_rule_of_its_mode` (para cada modo: contiene su regla —buscar `"call \`ask_user\` first"` en `Doubt`, `"Do not start any subagent until"` en `Always`, `"delegate directly. Say in your reply"` en `Never`— y no contiene las de los otros dos) y `the_model_receives_the_configured_rule` (proveedor espía como en `the_model_is_told_which_agents_it_can_delegate_to`, con `.with_delegation_confirm(ConfirmDelegation::Always)`; el `system` contiene `"Do not start any subagent until"`).
- [ ] **Paso 2: implementar. Paso 3: verificar** tests de `codeloom-core` y `codeloom-cli`, clippy, rustfmt, LF. **Paso 4: no commitear.**

---

### Tarea 3: el agente de cada hijo, a la vista

**Ficheros:** `crates/codeloom-tui/src/app.rs`, `crates/codeloom-tui/src/rail.rs`.

**Produce:**
```rust
/// Etiqueta del agente de un hijo: `[front]`; con origen de proyecto o remoto,
/// `[front·project]` / `[front·remote]`. `None` si el descriptor no conoce su agente.
pub(crate) fn agent_tag(subagent: &SubagentDescriptor) -> Option<String>
```
El nivel sale del texto estable de `agent_origin` (`built-in`, `global:<ruta>`, `project:<ruta>`, `remote:<url>`): se muestra solo para `project` y `remote`. Se usa en **dos sitios**: (a) `rail.rs`, antes de la etiqueta del hijo (`◐ [front] review`, y también en la variante con `›`); (b) `sync_subagent_activity`, en la cabecera: `review [front] (working)`. **El estado A2A entre paréntesis sigue siendo lo último y no se toca.** Con `agent: None`, ni corchetes ni espacio sobrante.

- [ ] **Paso 1: tests en rojo.** `agent_tag_names_the_agent_and_flags_project_and_remote_origins` (casos: `None` → `None`; `default`/`built-in` → `[default]`; `rust`/`global:...` → `[rust]`; `front`/`project:...` → `[front·project]`; `ext`/`remote:...` → `[ext·remote]`), `rail_shows_the_agent_of_each_child` (hijo con agente `front` aparece `[front]` en el rail; con `None` no aparece `[`), `activity_header_keeps_the_a2a_state_last` (cabecera `subagent  review [front] (working)` para el hijo con agente; y la ya existente `activity_header_names_the_exact_a2a_state_in_parentheses` sigue en verde sin cambios porque su hijo no tiene agente).
- [ ] **Paso 2: implementar. Paso 3: verificar** tests de `codeloom-tui`, clippy, rustfmt, LF. **Paso 4: no commitear.**

---

### Tarea 4: documentación

**Ficheros:** `docs/specs/2026-10-22-codeloom-a2a-delegation-choice-design.md`, `docs/plans/2026-10-22-codeloom-a2a-roadmap.md`.

- [ ] **Paso 1:** `## Estado (fecha)` en la spec: implementado, decisiones tomadas al implementar (incluida la corrección de la spec: la etiqueta se muestra **siempre** que el agente se conozca, también `default`, por la decisión del usuario sobre observabilidad), limitaciones reales (la regla no es una garantía y su cumplimiento no se puede probar sin un modelo real; la etiqueta de un hijo antiguo sin agente no aparece; no se muestra el motivo). En la hoja de ruta: A12 [x] con los commits, y apuntar A12b (clasificador/enrutador) como pendiente.
- [ ] **Paso 2: verificar** LF. **Paso 3: no commitear.**
