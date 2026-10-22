# Coste por subagente en el rail — Plan de implementación

> **Para agentes:** sub-skill requerida: `superpowers:subagent-driven-development` o `superpowers:executing-plans`. Tarea única. En esta ejecución quien supervisa commitea, así que **no commitees**.

**Meta:** que cada subagente del rail muestre su coste estimado como un grupo más de su línea de métricas (`2/5 read_file · 1.2k/300 · ≈$0.004 · 7s`).

**Arquitectura:** el apunte de consumo (`UsageRecord`) pasa a llevar la sesión que lo originó; la TUI agrupa los apuntes por sesión y `metrics_line` recibe el coste del hijo ya formateado.

**Spec:** `docs/specs/2026-10-21-codeloom-usage-ledger-design.md` (leerla antes; esto la amplía).

## Restricciones globales

- `codeloom-core` no depende de `ratatui`/`crossterm`. Errores de librería con `thiserror`. **Sin red en tests.**
- Comprobaciones: `cargo build`, `cargo test -p codeloom-core -p codeloom-tui -p codeloom-cli`, `cargo clippy -p codeloom-core -p codeloom-tui -- -D warnings`, `rustfmt --edition 2021 --check <ficheros tocados>`.
- Entorno de build (Git Bash): `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe' CARGO_TARGET_DIR='E:/workspace/personal/loom/target' CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`. No crear otros `target*`. Formatear solo con `rustfmt --edition 2021 <ruta>`, nunca `cargo fmt --all`. Ficheros en LF (`tr -cd '\r' < f | wc -c` debe dar 0).
- Comentarios en español «pijo», sin tacos ni andalucismos. No `git checkout/restore/stash/reset` ni push.
- No tocar `codeloom.toml` de la raíz.

## Review Focus

- La suma de los costes de los hijos más el del propio padre coincide con `estimated_cost()` total.
- Hijo sin apuntes o sin precio: el grupo de coste no aparece (nada de `≈$0`).
- Un apunte `None` de precio en un hijo no inventa cifra: ese hijo no muestra coste si **ninguno** de sus apuntes tiene precio; si algunos sí, muestra la suma con `+`.
- Una base de datos existente (tabla `usage` con `session_id` ya presente) sigue leyéndose sin migración.

---

### Tarea única: sesión en el apunte, coste por hijo y grupo en las métricas

**Ficheros:** `crates/codeloom-core/src/session/mod.rs`, `crates/codeloom-core/src/session/sqlite.rs`, `crates/codeloom-core/src/agent/mod.rs`, `crates/codeloom-tui/src/app.rs`, `crates/codeloom-tui/src/rail.rs`, `crates/codeloom-tui/src/run.rs` solo si hace falta, y cualquier otro literal de `UsageRecord` (localizar con `grep -rn "UsageRecord {" crates`).

**Interfaces — produce:**
```rust
// session/mod.rs: UsageRecord gana un campo (el resto igual)
pub struct UsageRecord { pub session: SessionId, /* model, input_tokens, output_tokens, input_price, output_price, created_at_ms */ }
// App
pub fn child_cost(&self, child: SessionId) -> Option<(f64, bool)>;   // apuntes con record.session == child; mismo criterio que estimated_cost (total, parcial); None sin apuntes o si ninguno tiene precio
// rail.rs
fn metrics_line(stats: &AgentStats, cost: Option<String>, now: Instant, width: usize) -> String;   // `cost` ya formateado; grupo nuevo entre tokens y tiempo
```

**Comportamiento:**
- `sqlite.rs`: `session_usage` selecciona también `session_id` y lo vuelca en `UsageRecord.session`. `record_usage` conserva su firma (el parámetro `session` sigue mandando en la fila).
- `agent/mod.rs` (hacia la línea 119): el `UsageRecord` que apunta el agente lleva la sesión del propio agente.
- `app.rs`: `record_live_usage` recibe la sesión de origen (`self.session_id` para el propio turno —si no hay sesión, se omite el apunte o se usa `SessionId(0)`, decisión del implementador documentada en un comentario— y el `child` para `ChildEvent::Usage`, línea ~2090). `child_cost` filtra `usage_records` por `session`.
- `rail.rs`: al pintar las métricas del hijo, `cost = app.child_cost(subagent.child)` formateado con la `format_cost(dollars, partial)` existente pero **sin** los espacios del prefijo: el texto del grupo es `format_cost(..)` con `"≈ $"` compactado a `"≈$"` (p. ej. `≈$0.004`, `≈$0.004+`). El grupo se inserta tras los tokens y antes del tiempo; solo cuando el hijo tiene `started_at` (donde ya salen los tokens). `None` → no se inserta grupo.
- Actualizar todos los tests y helpers existentes que construyen `UsageRecord` (campo nuevo `session`); los de `metrics_line` pasan `None` como coste.

- [ ] **Paso 1: tests en rojo** (indicar si el rojo es de compilación o de aserción): `usage_record_carries_its_session_after_roundtrip` (sqlite: el apunte del hijo vuelve con `session` del hijo y el del padre con la del padre, vía `session_usage`), `agent_usage_record_carries_agent_session`, `child_cost_sums_only_that_childs_records`, `child_cost_none_without_prices`, `child_cost_partial_marks_plus`, `child_costs_plus_parent_equal_total` (suma de `child_cost` de cada hijo + registros del padre = `estimated_cost()`), `live_child_usage_is_attributed_to_the_child`, `rail_child_metrics_show_cost_group_between_tokens_and_time`, `rail_child_metrics_omit_cost_group_without_price`, `metrics_line_places_cost_between_tokens_and_elapsed`.
- [ ] **Paso 2: implementar. Paso 3: verificar** `cargo build`, tests de los tres crates, clippy, rustfmt, LF. **Paso 4: no commitear.**
- [ ] **Paso 5: docs.** Añadir un párrafo a la sección `## Estado` de `docs/specs/2026-10-21-codeloom-usage-ledger-design.md`: el apunte lleva su sesión; el rail muestra el coste de cada hijo en su línea de métricas (`≈$0.004`, con `+` si es parcial); el no-objetivo «coste por hijo» de la spec de estimación queda superado.
