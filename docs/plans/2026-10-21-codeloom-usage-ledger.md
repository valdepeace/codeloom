# Libro de consumo por sesión — Plan de implementación

> **Para agentes:** sub-skill requerida: `superpowers:subagent-driven-development` o `superpowers:executing-plans`. Una tarea cada vez, en orden. Al terminar y verificar cada tarea se commitea (regla del usuario); en esta ejecución quien supervisa commitea, así que **no commitees**.

**Meta:** guardar cada consumo (modelo, tokens, precio vigente) y sumar el coste correcto de una sesión y sus subagentes, también tras reabrirla.

**Arquitectura:** el núcleo guarda apuntes en SQLite al recibir el uso del proveedor y expone `Catalog::prices`; el agente comparte un catálogo con la tarea de carga; la TUI recibe los apuntes al abrir sesión, suma en vivo y pinta el total y el precio por millón.

**Spec:** `docs/specs/2026-10-21-codeloom-usage-ledger-design.md` (leerla antes).

## Restricciones globales

- `codeloom-core` no depende de `ratatui`/`crossterm`. Errores de librería con `thiserror`. **Sin red en tests.**
- Comprobaciones: `cargo build`, `cargo test -p <crate>`, `cargo clippy -p <crate> -- -D warnings`, `rustfmt --edition 2021 --check <ficheros>`.
- Entorno de build (Git Bash): `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe' CARGO_TARGET_DIR='E:/workspace/personal/loom/target' CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`. No crear otros `target*`. Formatear solo con `rustfmt --edition 2021 <ruta>`, nunca `cargo fmt --all`. Ficheros en LF (`tr -cd '\r' < f | wc -c` debe dar 0; no conviertas finales de línea).
- Comentarios en español «pijo», sin tacos ni andalucismos. No `git checkout/restore/stash/reset` ni push.
- **Probar contra datos reales cuando se toque el catálogo**: el JSON real está en `C:\Users\negrero\.codeloom\models-dev.json` (solo lectura).
- Un fallo al guardar un apunte nunca rompe ni retrasa el turno.

## Review Focus

- Sin catálogo o sin precio: el apunte se guarda con precio `NULL` y el total se marca parcial (`+`), sin inventar cifras.
- Cambio de modelo a mitad de sesión: cada apunte conserva su precio; el total suma bien.
- La suma de una sesión incluye a sus hijos y no mezcla otras sesiones.
- Una base de datos existente (sin tabla `usage`) se migra sola y no pierde sesiones.
- Recargar una sesión no duplica los apuntes en vivo.

---

### Tarea 1: núcleo — apuntes, almacén y precios

**Ficheros:** `crates/codeloom-core/src/session/mod.rs` (trait y tipos), `crates/codeloom-core/src/session/sqlite.rs`, cualquier otro `impl SessionStore` (localizar con grep, incluidos dobles de test), `crates/codeloom-core/src/catalog.rs`, `crates/codeloom-core/src/agent/mod.rs`.

**Interfaces — produce:**
```rust
// session/mod.rs
#[derive(Debug, Clone, PartialEq)]
pub struct UsageRecord {
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub input_price: Option<f64>,   // dólares por millón, congelado al apuntar
    pub output_price: Option<f64>,
    pub created_at_ms: i64,
}
impl UsageRecord { pub fn cost(&self) -> Option<f64> }   // None si falta algún precio
// trait SessionStore (con implementación en SQLite; tabla `usage`, CREATE TABLE IF NOT EXISTS):
async fn record_usage(&self, session: SessionId, record: &UsageRecord) -> Result<()>;
async fn session_usage(&self, session: SessionId) -> Result<Vec<UsageRecord>>;   // la sesión y sus hijos (tabla `subagents`), orden por id
// catalog.rs
impl Catalog { pub fn prices(&self, base_url: &str, model: &str) -> Option<(f64, f64)> }   // coincidencia de URL sin barra final ni mayúsculas
// agent/mod.rs
impl AgentLoop { pub fn with_catalog(self, catalog: Arc<RwLock<Catalog>>) -> Self }
```
En el agente, justo donde se recibe `StreamChunk::Usage` y se publica `LiveEvent::Usage`, se apunta un `UsageRecord` con el modelo y la `base_url` de la configuración de ese instante y los precios de `catalog.prices` (si hay catálogo); el fallo al guardar se registra con `tracing::warn!` y se sigue. Los hijos reutilizan el mismo catálogo compartido: pasar el `Arc` al lanzador/`AgentLoop` del hijo donde se construya.

- [ ] **Paso 1: tests en rojo** (indicar si el rojo es de compilación o de aserción): `usage_roundtrip`, `usage_is_isolated_per_session`, `session_usage_includes_children`, `usage_record_cost_none_without_prices`, `prices_match_url_ignoring_slash_and_case`, `prices_none_when_model_or_price_missing`, `agent_records_one_usage_row_per_response_with_current_model_and_price`, `agent_records_null_prices_without_catalog`, `model_switch_mid_session_keeps_each_price`, `store_migrates_an_existing_database_without_usage_table`.
- [ ] **Paso 2: implementar. Paso 3: verificar** `cargo test -p codeloom-core`, clippy, rustfmt, LF, y una prueba descartable (borrarla después) de `Catalog::prices` contra el JSON real para `openrouter` + `mistralai/mistral-nemo`. **Paso 4: no commitear.**

---

### Tarea 2: carga del catálogo compartida y entrega de apuntes a la TUI

**Ficheros:** `crates/codeloom-cli/src/main.rs`, `crates/codeloom-tui/src/run.rs`, `crates/codeloom-tui/src/app.rs`.

**Interfaces — consume:** `UsageRecord`, `SessionStore::session_usage`, `AgentLoop::with_catalog`, `Catalog::prices`. **Produce:**
```rust
// run.rs
SessionUpdate::Usage(SessionId, Vec<UsageRecord>)   // el trabajador lo envía junto a `Loaded` (y al crear sesión nueva con vector vacío)
// app.rs
pub usage_records: Vec<UsageRecord>                  // apuntes de la sesión abierta, persistidos + vivos
pub fn load_usage(&mut self, session: SessionId, records: Vec<UsageRecord>);   // solo si es la sesión abierta; sustituye la lista
```
En `main.rs`: crear el `Arc<RwLock<Catalog>>` vacío, pasarlo al agente con `with_catalog`, y que `load_catalog` además de enviar el catálogo a la TUI lo escriba en ese `RwLock`. El trabajador de sesiones envía `SessionUpdate::Usage` tras `Loaded`. En la TUI, `load_session` vacía `usage_records` (la carga posterior los repone).
En vivo: en `App::apply`, al llegar `LiveEvent::Usage` (sesión propia) se añade un `UsageRecord` local con `self.model`, los tokens y `self.model_prices()`; al llegar `ChildEvent` con `Usage` se hace igual atribuyéndolo al modelo actual. Los contadores `input_tokens`/`output_tokens` y `hierarchy_tokens` actuales **no se tocan**.

- [ ] **Paso 1: tests en rojo:** `load_usage_replaces_records_for_the_open_session`, `load_usage_ignores_other_sessions`, `live_usage_appends_a_priced_record`, `live_child_usage_appends_a_record`, `session_load_clears_usage_records`, `reload_does_not_duplicate_live_records`.
- [ ] **Paso 2: implementar. Paso 3: verificar** `cargo build`, `cargo test -p codeloom-tui -p codeloom-cli`, clippy, rustfmt, LF. **Paso 4: no commitear.**

---

### Tarea 3: total y precio por millón en el rail

**Ficheros:** `crates/codeloom-tui/src/app.rs`, `crates/codeloom-tui/src/rail.rs`.

**Interfaces — consume:** `App.usage_records`. **Produce:**
```rust
// App
pub fn estimated_cost(&self) -> Option<(f64, bool)>;   // (suma de los apuntes con precio, parcial = algún apunte sin precio); None si no hay apuntes
// rail.rs
pub fn format_cost(dollars: f64, partial: bool) -> String;   // «≈ $0.0123» / «≈ $0.0123+»
pub fn format_prices(input: f64, output: f64) -> String;     // «$3/$15 /M» (sin decimales si son enteros; hasta 2 si no)
```
`estimated_cost` sustituye al actual (basado en tokens × precio actual); ajustar los tests existentes del plan anterior. En el rail: la línea de coste usa `format_cost`; bajo el modelo se añade, si `model_prices()` es `Some`, la línea tenue `format_prices(..)`.

- [ ] **Paso 1: tests en rojo:** `estimated_cost_sums_each_record_at_its_own_price`, `estimated_cost_marks_partial_when_a_record_has_no_price`, `estimated_cost_none_without_records`, `format_cost_partial_adds_plus`, `format_prices_shapes`, `rail_shows_session_total_and_model_prices`, `rail_hides_prices_line_without_catalog`.
- [ ] **Paso 2: implementar. Paso 3: verificar** `cargo test -p codeloom-tui`, clippy, rustfmt, LF. **Paso 4: no commitear.**

---

### Tarea 4: documentación

Sección `## Estado` de la spec (implementado, decisiones, limitaciones) y una nota al final de `2026-10-21-codeloom-cost-estimate-design.md` diciendo que `estimated_cost` pasó al libro de consumo. Quien supervisa commitea.
