# Coste estimado en el rail — Plan de implementación

> **Para agentes:** sub-skill requerida: `superpowers:subagent-driven-development` o `superpowers:executing-plans`. Al terminar y verificar se commitea (regla del usuario).

**Meta:** mostrar en el rail una estimación en dólares de la sesión, a partir de los precios de models.dev.

**Arquitectura:** `App` calcula el coste con el catálogo ya cargado (`App.catalog`) y los contadores de tokens existentes; `rail.rs` pinta una línea.

**Spec:** `docs/specs/2026-10-21-codeloom-cost-estimate-design.md` (leerla antes).

## Restricciones globales

- `codeloom-core` no depende de `ratatui`. Sin red en tests.
- Comprobaciones: `cargo build`, `cargo test -p codeloom-tui`, `cargo clippy -p codeloom-tui -- -D warnings`, `rustfmt --edition 2021 --check <ficheros>`.
- Entorno de build (Git Bash): `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe' CARGO_TARGET_DIR='E:/workspace/personal/loom/target' CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`. No crear otros `target*`. Formatear solo con `rustfmt --edition 2021 <ruta>`, nunca `cargo fmt --all`. Ficheros en LF.
- Comentarios en español «pijo», sin tacos ni andalucismos. No `git checkout/restore/stash/reset` ni push.

## Review Focus

- Sin precio (catálogo vacío, modelo ausente, precio ausente) no aparece la línea.
- URL base con y sin barra final coinciden.
- Cero tokens: no se muestra.
- Cifras enormes no rompen la anchura del rail.

---

### Tarea única: precios, cálculo, formato y línea del rail

**Ficheros:** `crates/codeloom-core/src/catalog.rs` (si `CatalogModel` ya expone `input_cost`/`output_cost` no se toca), `crates/codeloom-tui/src/app.rs`, `crates/codeloom-tui/src/rail.rs`.

**Interfaces — produce:**
```rust
// App
pub fn model_prices(&self) -> Option<(f64, f64)>;   // (entrada, salida) por millón, ver spec
pub fn estimated_cost(&self) -> Option<f64>;        // None si no hay precios o no hay tokens
// rail.rs
pub fn format_cost(dollars: f64) -> String;         // «≈ $0.0123», «≈ $1.23», «≈ $123», «≈ < $0.0001»
```
Contadores: usar los que ya pinta el rail en la línea `↑{input} ↓{output}` (localizarlos en `rail.rs`; son los totales de la sesión del padre) más `hierarchy_tokens()` para los hijos. En el rail, la línea de coste va justo debajo de la de tokens, con `styles.muted`, recortada a la anchura.

- [ ] **Paso 1: tests en rojo** (indicar si el rojo es de compilación o de aserción): `model_prices_match_provider_by_url_ignoring_trailing_slash`, `model_prices_none_without_catalog`, `model_prices_none_when_a_price_is_missing`, `estimated_cost_adds_children`, `estimated_cost_none_with_zero_tokens`, `format_cost_ranges` (0.00005 → «≈ < $0.0001», 0.0123, 1.234 → «≈ $1.23», 123.4 → «≈ $123»), `rail_shows_cost_line_when_priced`, `rail_has_no_cost_line_without_prices`.
- [ ] **Paso 2: implementar. Paso 3: verificar** `cargo test -p codeloom-tui`, clippy, rustfmt, LF. **Paso 4: NO commitear** (lo hace quien supervisa tras verificar).
