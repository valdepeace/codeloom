# Catálogo models.dev y diálogo de ajustes — Plan de implementación

> **Para agentes:** sub-skill requerida: `superpowers:subagent-driven-development` o `superpowers:executing-plans`. Una tarea cada vez, en orden. Al terminar y verificar cada tarea se commitea (regla del usuario).

**Meta:** (a) que `/settings` avise de los cambios sin guardar; (b) elegir proveedor y modelo de un catálogo de models.dev en vez de teclearlos.

**Arquitectura:** `codeloom-core` gana un módulo `catalog` (tipos, parseo, caché, obtención inyectable). La carga ocurre en segundo plano al arrancar y se entrega a la TUI como `Arc<Catalog>`. El diálogo de ajustes añade selectores con filtrado y marca de cambios sin guardar.

**Stack:** Rust, `serde`/`serde_json`, `reqwest` (ya en core), `tokio`, `ratatui`.

**Spec:** `docs/specs/2026-10-21-codeloom-models-catalog-design.md` (leerla antes). Muestra real del JSON, solo lectura: `C:\Users\negrero\AppData\Local\Temp\claude\E--workspace-personal-loom\dc105655-c40d-4e39-bc4c-3e4178431c74\scratchpad\modelsdev.json`.

## Restricciones globales

- `codeloom-core` no depende de `ratatui`/`crossterm`. **Sin red en tests**.
- Comprobaciones: `cargo build`, `cargo test -p <crate tocado>`, `cargo clippy -p <crate> -- -D warnings`, `rustfmt --edition 2021 --check <ficheros>`.
- Entorno de build (Git Bash): `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe' CARGO_TARGET_DIR='E:/workspace/personal/loom/target' CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`. No crear otros `target*`. Formatear solo con `rustfmt --edition 2021 <ruta>`, nunca `cargo fmt --all`. Ficheros en LF.
- Errores de librería con `thiserror`. Comentarios en español «pijo», sin tacos ni andalucismos.
- La fluidez del composer es prioridad máxima: nada de red ni de trabajo pesado en teclado o dibujado.
- No hacer `git checkout/restore/stash/reset` ni push.

## Review Focus

- Con campos sin guardar, `Esc` no cierra a la primera ni pierde cambios en silencio; guardar o descartar limpia la marca.
- JSON corrupto, vacío o con campos ausentes no hace panic: catálogo vacío y `/settings` sigue usable.
- Una caché a medio escribir (corte de luz) no se lee como válida: escritura atómica.
- Sin red ni caché, el arranque no se retrasa ni falla.
- Elegir del selector y luego editar a mano a un valor que no está en el catálogo es posible.

---

### Tarea 1: marca de cambios sin guardar en `/settings`

**Ficheros:** `crates/codeloom-tui/src/app.rs`, `crates/codeloom-tui/src/run.rs`, `crates/codeloom-tui/src/render.rs`.

**Interfaces — produce:**
```rust
// ProviderSettings: añadir derive PartialEq si falta.
// App
settings_baseline: Option<ProviderSettings>,        // copia al abrir el diálogo y tras guardar con éxito
pub fn settings_dirty(&self) -> bool;               // provider_settings != settings_baseline (ambos Some)
pub fn close_settings(&mut self) -> bool;           // true si cierra; con cambios sin guardar y sin aviso previo: pone settings_feedback = "Unsaved changes · Esc again to discard" y devuelve false; con el aviso ya dado (segunda Esc seguida) restaura provider_settings desde la copia y cierra
```
`open_settings` y `set_provider_settings` fijan `settings_baseline`; `settings_saved(Ok)` lo actualiza a la copia actual; cualquier edición posterior vuelve a poder activar el aviso (el aviso «ya dado» se anula al editar un campo o al pulsar cualquier otra tecla).
`render.rs` (diálogo de ajustes, junto a cada campo): si el valor del campo difiere del de la copia, se añade `● unsaved` (estilo de aviso del tema) tras el valor; la fila «Save and apply» muestra además el recuento (`Save and apply (2 changes)`). `run.rs`: la tecla `Esc` del diálogo llama a `close_settings` en vez de cerrar directamente.

- [ ] **Paso 1: tests en rojo** (indicar si el rojo es de compilación o de aserción): `settings_not_dirty_when_opened`, `editing_a_field_makes_settings_dirty`, `esc_with_unsaved_changes_warns_first`, `second_esc_discards_and_restores`, `esc_without_changes_closes_at_once`, `successful_save_clears_dirty`, `editing_after_warning_cancels_the_warning`, y en `render.rs` `settings_dialog_marks_changed_fields_unsaved` (el campo cambiado muestra `● unsaved`; el intacto no).
- [ ] **Paso 2: implementar.** **Paso 3: verificar** `cargo test -p codeloom-tui`, clippy, rustfmt, LF. **Paso 4: commit** (español pijo + trailer `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`).

---

### Tarea 2: núcleo — módulo `catalog` (parseo, filtros, caché)

**Ficheros:** `crates/codeloom-core/src/catalog.rs` (nuevo), `crates/codeloom-core/src/lib.rs` (`pub mod catalog;`), `crates/codeloom-core/tests`-style: pruebas dentro del módulo con un JSON de ejemplo embebido en el propio test (3 proveedores: uno `anthropic`, uno `openrouter` con un modelo sin `tool_call` y otro `deprecated`, uno con `npm` no soportado).

**Interfaces — produce:**
```rust
pub struct CatalogModel { pub id: String, pub name: String, pub context: Option<u64>, pub max_output: Option<u64>,
                          pub input_cost: Option<f64>, pub output_cost: Option<f64>, pub reasoning: bool, pub release_date: Option<String> }
pub struct CatalogProvider { pub id: String, pub name: String, pub kind: ProviderKind, pub base_url: String,
                             pub api_key_env: String /* primera de `env`, o "" */, pub models: Vec<CatalogModel> }
#[derive(Default, Clone)]
pub struct Catalog { pub providers: Vec<CatalogProvider> }
impl Catalog {
    pub fn from_json(raw: &str) -> Result<Catalog, CatalogError>;   // aplica traducción de `npm`, filtros y orden de la spec
    pub fn provider(&self, id: &str) -> Option<&CatalogProvider>;
    pub fn is_empty(&self) -> bool;
}
#[derive(Debug, thiserror::Error)] pub enum CatalogError { Parse(..), Io(..), Http(..) }
pub const CATALOG_URL: &str = "https://models.dev/api.json";
pub const CATALOG_TTL: std::time::Duration = std::time::Duration::from_secs(24 * 3600);
pub fn cache_path() -> Option<std::path::PathBuf>;                  // ~/.codeloom/models-dev.json
pub fn read_cache(path: &Path) -> Option<(Catalog, SystemTime)>;    // None si falta o está corrupto
pub fn is_stale(modified: SystemTime, now: SystemTime) -> bool;     // > CATALOG_TTL
pub fn write_cache_atomic(path: &Path, raw: &str) -> Result<(), CatalogError>;   // temporal + rename
pub async fn fetch_raw(url: &str) -> Result<String, CatalogError>;  // reqwest, timeout 15 s
pub async fn refresh<F, Fut>(path: &Path, fetch: F) -> Result<Catalog, CatalogError>
    where F: FnOnce() -> Fut, Fut: Future<Output = Result<String, CatalogError>>;  // obtiene, valida con from_json, escribe caché solo si el parseo salió bien
```
Reglas de `from_json`: traducción `npm`→`ProviderKind` y URL por defecto para `openai`/`anthropic` sin `api` (spec, «Decisiones»); `env` ausente → `api_key_env = ""`; modelos con `tool_call != true` o `status == "deprecated"` fuera; proveedores sin modelos válidos fuera; orden: `openrouter`, `anthropic`, `openai` primero y luego alfabético por `name`; modelos por `release_date` descendente (los sin fecha al final).

- [ ] **Paso 1: tests en rojo** con el JSON de ejemplo: `maps_npm_to_provider_kind`, `drops_unsupported_providers_and_models`, `orders_providers_and_models`, `tolerates_missing_fields`, `corrupt_json_is_an_error`, `stale_after_ttl`, `cache_roundtrip`, `corrupt_cache_reads_as_none`, `refresh_does_not_write_cache_on_bad_json` y `refresh_writes_cache_on_good_json` (con una función de obtención falsa; directorio temporal propio del test).
- [ ] **Paso 2: implementar.** **Paso 3: verificar** `cargo test -p codeloom-core`, clippy, rustfmt, LF. **Paso 4: commit.**

---

### Tarea 3: carga en segundo plano y entrega a la TUI

**Ficheros:** `crates/codeloom-cli/src/main.rs`, `crates/codeloom-tui/src/app.rs`, `crates/codeloom-tui/src/run.rs`.

**Interfaces — consume:** `catalog::{cache_path, read_cache, is_stale, refresh, fetch_raw, CATALOG_URL, Catalog}`. **Produce:** `App.catalog: std::sync::Arc<Catalog>` (por defecto vacío) y `App::set_catalog(&mut self, Arc<Catalog>)`; un canal `tokio::sync::mpsc::UnboundedReceiver<Arc<Catalog>>` que el bucle de `run.rs` atiende (un brazo más en su `select!`) llamando a `set_catalog` y pidiendo un fotograma.

En `main.rs` (solo modo TUI, no `--print`): leer la caché; si existe, enviar ya su catálogo; si falta o `is_stale`, lanzar `tokio::spawn` con `refresh(path, || fetch_raw(CATALOG_URL))` y enviar el resultado si es `Ok`. Los errores se ignoran en silencio (a lo sumo `app.status` no se toca). Nada de esto bloquea el arranque.

- [ ] **Paso 1: tests en rojo:** `set_catalog_replaces_the_catalog`, y en `run.rs` `catalog_update_is_applied_by_the_loop` (enviar un `Catalog` de ejemplo por el canal y comprobar `app.catalog`).
- [ ] **Paso 2: implementar.** **Paso 3: verificar** `cargo build`, `cargo test -p codeloom-tui -p codeloom-cli`, clippy, rustfmt. **Paso 4: commit.**

---

### Tarea 4: selectores de proveedor y modelo en `/settings`

**Ficheros:** `crates/codeloom-tui/src/app.rs`, `crates/codeloom-tui/src/run.rs`, `crates/codeloom-tui/src/render.rs`.

**Interfaces — consume:** `App.catalog`, `CatalogProvider`, `CatalogModel`, la marca de cambios de la tarea 1. **Produce:**
```rust
pub enum SettingsPicker { Provider, Model }            // en app.rs
App.settings_picker: Option<SettingsPicker>
App.picker_query: String
App.picker_selected: usize
pub fn picker_items(&self) -> Vec<PickerItem>;         // filtrado por `picker_query` (sin distinguir mayúsculas, sobre id y nombre)
pub struct PickerItem { pub id: String, pub label: String, pub detail: String }
pub fn open_picker(&mut self, picker: SettingsPicker); // no hace nada si el catálogo está vacío (y pone settings_feedback = "Catalog not loaded yet; type it by hand")
pub fn picker_choose(&mut self);                       // Provider: rellena kind, base_url, api_key_env y deja la lista de modelos de ese proveedor; Model: rellena model
```
Teclas en el diálogo: `Ctrl+L` (o `Enter` con el campo 0 «Provider» enfocado... conservar el cambio de tipo con ←/→) abre el selector del campo enfocado: en «Provider» el de proveedores, en «Model» el de modelos del proveedor cuya `base_url` coincida con la actual (si ninguno coincide, `settings_feedback = "No catalog provider matches the current URL"`). Dentro: escribir filtra, ↑/↓ mueven, `Enter` elige, `Esc` cierra el selector sin cerrar el diálogo. Lo que se rellena cuenta como cambio sin guardar (tarea 1). `detail` del modelo: `ctx 200k · $3/$15` (contexto y coste por millón; `-` si falta). Reutilizar `format_tokens` de `rail.rs`.
`render.rs`: el selector se pinta como lista encima del diálogo, con máximo 8 filas visibles y desplazamiento; la ayuda inferior del diálogo añade `Ctrl+L catalog`.

- [ ] **Paso 1: tests en rojo:** `picker_filters_by_query`, `choosing_a_provider_fills_kind_url_and_env`, `choosing_a_model_fills_the_model`, `picker_with_empty_catalog_sets_feedback`, `picker_changes_count_as_unsaved`, `manual_edit_still_works_after_picking`, `esc_closes_the_picker_but_not_the_dialog`, y en `render.rs` `picker_renders_items_with_detail`.
- [ ] **Paso 2: implementar.** **Paso 3: verificar** `cargo test -p codeloom-tui`, clippy, rustfmt, LF. **Paso 4: commit.**

---

### Tarea 5: documentación

**Ficheros:** `docs/specs/2026-10-21-codeloom-models-catalog-design.md` (sección `## Estado`: implementado, decisiones tomadas, limitaciones), `README.md` solo si menciona `/settings` (una línea sobre `Ctrl+L`).
- [ ] **Paso 1:** redactar el estado con lo realmente implementado. **Paso 2: commit.**
