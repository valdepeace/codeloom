# Esfuerzo de razonamiento — Plan de implementación

> **Para agentes:** sub-skill requerida: `superpowers:subagent-driven-development` o `superpowers:executing-plans`. Una tarea cada vez, en orden. Al terminar y verificar cada tarea se commitea (regla del usuario).

**Meta:** elegir el esfuerzo de razonamiento de un modelo desde `/settings` con los datos de models.dev y enviarlo al proveedor.

**Arquitectura:** el catálogo ya existente (`codeloom_core::catalog`) gana el soporte de razonamiento por modelo; la configuración y `ProviderRequest` llevan `effort`; cada proveedor lo traduce a su forma; el diálogo añade un campo.

**Spec:** `docs/specs/2026-10-21-codeloom-reasoning-effort-design.md` (leerla antes). Muestra real del JSON, solo lectura: `C:\Users\negrero\AppData\Local\Temp\claude\E--workspace-personal-loom\dc105655-c40d-4e39-bc4c-3e4178431c74\scratchpad\modelsdev.json` (clave `reasoning_options` de cada modelo).

## Restricciones globales

- `codeloom-core` no depende de `ratatui`/`crossterm`. **Sin red en tests**.
- Comprobaciones: `cargo build`, `cargo test -p <crate>`, `cargo clippy -p <crate> -- -D warnings`, `rustfmt --edition 2021 --check <ficheros>`.
- Entorno de build (Git Bash): `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe' CARGO_TARGET_DIR='E:/workspace/personal/loom/target' CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`. No crear otros `target*`. Formatear solo con `rustfmt --edition 2021 <ruta>`, nunca `cargo fmt --all`. Ficheros en LF.
- Comentarios en español «pijo», sin tacos ni andalucismos. No `git checkout/restore/stash/reset` ni push.
- Sin esfuerzo configurado, **las peticiones salen byte a byte iguales que hoy**.

## Review Focus

- Config antigua sin `effort` se lee sin error y equivale a «default».
- Nivel guardado que el modelo nuevo no admite: se descarta, no se envía.
- Con Anthropic y `thinking`, `max_tokens` > presupuesto y no se envía `temperature`.
- Modelo sin razonamiento: el campo «Effort» no aparece y no hay marca en el rail.

---

### Tarea 1: núcleo — catálogo y configuración

**Ficheros:** `crates/codeloom-core/src/catalog.rs`, el módulo de configuración donde viven `CodeLoomConfig`/`ProviderKind` (localizarlo con grep), `crates/codeloom-core/src/provider/mod.rs` (`ProviderRequest`).

**Interfaces — produce:**
```rust
// catalog.rs
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Reasoning { pub efforts: Vec<String>, pub budget: Option<(Option<u64>, Option<u64>)> /* (min, max) */, pub toggle: bool }
// CatalogModel gana: pub reasoning: Option<Reasoning>   // None si el modelo no razona
// from_json: reasoning == true => Some(Reasoning) con lo de `reasoning_options`:
//   efforts = values de la primera opción type "effort"; budget = type "budget_tokens"; toggle = hay type "toggle".
//   reasoning != true => None.
// config: CodeLoomConfig gana `pub effort: Option<String>` (serde default, omitido al guardar si es None)
// provider: ProviderRequest gana `pub effort: Option<String>`; TODOS los sitios que construyen
//   ProviderRequest (agente, subagente, tests) lo rellenan desde la config (None en tests/fixtures).
```
- [ ] **Paso 1: tests en rojo:** `parses_effort_values`, `parses_budget_range`, `non_reasoning_model_has_no_reasoning`, `reasoning_true_without_options_is_some_default`, `config_without_effort_still_loads`, `config_roundtrips_effort`, `agent_passes_config_effort_to_the_request` (proveedor de prueba que captura la petición).
- [ ] **Paso 2: implementar. Paso 3: verificar** `cargo test -p codeloom-core`, clippy, rustfmt, LF. **Paso 4: commit** (español pijo + trailer `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`).

---

### Tarea 2: proveedores — traducir el esfuerzo

**Ficheros:** `crates/codeloom-core/src/provider/openai.rs`, `crates/codeloom-core/src/provider/anthropic.rs`.

**Interfaces — consume:** `ProviderRequest.effort`. **Produce:** en `build_body` de cada proveedor, según la sección «Decisiones de diseño» de la spec (OpenAI-compatible: `reasoning_effort`, o `reasoning: {"effort": ..}` si la URL contiene `openrouter.ai`; Anthropic: tabla de presupuestos, `thinking`, `max_tokens` ampliado y `temperature` omitida). Función pura `fn anthropic_budget(effort: &str) -> Option<u32>`.
- [ ] **Paso 1: tests en rojo:** `openai_body_without_effort_is_unchanged`, `openai_body_sends_reasoning_effort`, `openrouter_body_nests_effort_under_reasoning`, `anthropic_body_without_effort_is_unchanged`, `anthropic_effort_enables_thinking_and_raises_max_tokens`, `anthropic_effort_omits_temperature`, `anthropic_budget_table`, `unknown_effort_sends_nothing`.
- [ ] **Paso 2: implementar. Paso 3: verificar** `cargo test -p codeloom-core`, clippy, rustfmt, LF. **Paso 4: commit.**

---

### Tarea 3: TUI — campo «Effort» y rail

**Ficheros:** `crates/codeloom-tui/src/{app,run,render,rail}.rs`, `crates/codeloom-cli/src/main.rs` (pasar `effort` entre `ProviderSettings` y `CodeLoomConfig` en `config_from_provider_settings` y al construir los ajustes iniciales).

**Interfaces — consume:** `CatalogModel.reasoning`, `CodeLoomConfig.effort`. **Produce:**
```rust
// ProviderSettings gana: pub effort: Option<String>
// App
pub fn effort_options(&self) -> Vec<String>;   // efforts del modelo actual según el catálogo (buscando por base_url + model); vacío si no hay catálogo, no razona o no tiene efforts
pub fn cycle_effort(&mut self, forward: bool); // None -> v1 -> v2 … -> None; sin opciones no hace nada
```
Diálogo: campo «Effort» visible solo si `effort_options()` no está vacío (cuidado con los índices del resto de campos: ajustar los tests afectados), `←`/`→` lo ciclan, cuenta como cambio sin guardar. Al cambiar el modelo (selector o edición), si el nivel actual no está en las opciones nuevas se pone `None` y `settings_feedback = "Effort reset: not supported by this model"`. Rail: bajo el modelo, `effort high` (estilo tenue) si hay nivel.
- [ ] **Paso 1: tests en rojo:** `effort_options_follow_the_catalog_model`, `cycle_effort_walks_default_and_values`, `effort_field_hidden_for_non_reasoning_model`, `changing_model_drops_unsupported_effort`, `effort_change_counts_as_unsaved`, `rail_shows_effort_when_set`.
- [ ] **Paso 2: implementar. Paso 3: verificar** `cargo build`, `cargo test -p codeloom-tui -p codeloom-cli`, clippy, rustfmt. **Paso 4: commit.**

---

### Tarea 4: documentación

Sección `## Estado` de la spec con lo implementado, decisiones y limitaciones (tabla de presupuestos y forma de OpenRouter sin comprobar contra el servicio real; `toggle` no soportado). **Commit.**
