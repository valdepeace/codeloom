# Aviso de presupuesto: plan de implementación

> Para quien ejecute: una tarea cada vez, con verificación y commit al terminar cada una.

**Meta:** avisar a la persona usuaria cuando el gasto acumulado de la sesión cruza el 80 % y el
100 % de un presupuesto configurado, sin detener nada.

**Spec:** `docs/specs/2026-10-23-codeloom-budget-warning-design.md` (léase primero).

## Restricciones globales

Las del plan `2026-10-23-codeloom-context-summary.md` (entorno de build, LF, `rustfmt --edition
2021 <ruta>`, no tocar `codeloom.toml`, `git add` de rutas concretas, comentarios en español
formal, `codeloom-core` sin `ratatui`/`crossterm`, sin red en tests, clippy sin `--all-targets`).

---

### Tarea 1: configuración y función pura

**Ficheros:** crear `crates/codeloom-core/src/budget.rs`; `pub mod budget;` en
`crates/codeloom-core/src/lib.rs`.

**Produce (firmas exactas):**

```rust
pub const WARN_LEVELS_PERCENT: [u8; 2] = [80, 100];

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct BudgetConfig {
    pub session_tokens: Option<u64>,
    pub session_cost_usd: Option<f64>,
}
impl BudgetConfig {
    pub fn from_toml(text: &str) -> Result<BudgetConfig>;   // sección [budget]; ausente = Default
    pub fn load(path: &Path) -> Result<BudgetConfig>;
    pub fn is_empty(&self) -> bool;                          // ambas claves a None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetMetric { Tokens, CostUsd }

#[derive(Debug, Clone, PartialEq)]
pub struct BudgetWarning {
    pub metric: BudgetMetric,
    pub level_percent: u8,
    pub used: f64,     // acumulado tras el apunte (tokens o dólares)
    pub limit: f64,
    /// Solo CostUsd: `true` si algún apunte no tenía precio y por tanto no suma.
    pub partial: bool,
}

pub fn budget_crossings(config: &BudgetConfig, before: &[UsageRecord], added: &UsageRecord) -> Vec<BudgetWarning>;
```

Sigue el patrón de `AgentsConfig`/`FicheroConAgentes` de `config/mod.rs` (struct privado con
`#[serde(default)] budget: BudgetConfig`; error `CoreError::Config`). Rechaza valores no
positivos o no finitos (`session_tokens = 0`, coste `<= 0`, `NaN`) con `CoreError::Config`.

**Algoritmo de `budget_crossings`:** por cada métrica configurada, `antes` = suma sobre `before`;
`despues` = `antes` + valor de `added`. Tokens: `input_tokens + output_tokens`. Coste: suma de
`UsageRecord::cost()` de los apuntes que lo tienen; `partial` si alguno de `before` o de `added`
devuelve `None`. Para cada nivel de `WARN_LEVELS_PERCENT` (en orden ascendente), umbral =
`limit * nivel / 100`; avisa si `antes < umbral && umbral <= despues`. Devuelve los avisos en
orden de métrica (Tokens, luego CostUsd) y nivel.

**Tests:**
- `from_toml_reads_the_budget_section` y `from_toml_without_section_is_empty`.
- `from_toml_rejects_zero_negative_and_nan_limits`.
- `crossing_80_percent_warns_once`: antes 79 %, apunte que lleva al 81 % → un aviso de nivel 80;
  un segundo apunte 81 %→85 % → ninguno.
- `a_jump_across_both_levels_emits_both`: 70 % → 130 % → niveles 80 y 100, en ese orden.
- `exactly_reaching_the_threshold_counts` (después == umbral avisa; antes == umbral no).
- `cost_ignores_unpriced_records_and_marks_partial`.
- `empty_config_never_warns`.
- `tokens_and_cost_warn_independently`.

**Commit:** `feat(core): configuración y cruce de umbrales del presupuesto de gasto`.

---

### Tarea 2: integrarlo en el bucle

**Ficheros:** `crates/codeloom-core/src/event/mod.rs`, `crates/codeloom-core/src/agent/mod.rs`,
`crates/codeloom-cli/src/main.rs`.

**Produce:**
- `LiveEvent::BudgetWarning { metric: BudgetMetric, level_percent: u8, used: f64, limit: f64,
  partial: bool }`, tratado como `ContextCompacted` en `event/mod.rs` (`is_child_relevant` true).
  Si algún `match` exhaustivo de otra crate deja de compilar, brazo mínimo que lo ignore (el
  pintado es la tarea 3).
- `AgentLoop::with_budget(self, BudgetConfig) -> Self` (campo `budget: BudgetConfig`, por defecto
  vacío). **No** se propaga al lanzador de subagentes (spec, «Fuera de alcance»).
- En `record_usage`: antes de guardar el apunte, leer `self.store.session_usage(session)`; tras
  guardarlo, `budget_crossings(&self.budget, &anteriores, &registro)` y publicar un
  `LiveEvent::BudgetWarning` por aviso. Con `budget.is_empty()` no se lee nada del almacén. Un
  error al leer el consumo se registra con `tracing::warn!` y no afecta a nada más.
- En `codeloom-cli/src/main.rs`: leer `[budget]` del mismo fichero y mismo punto donde se lee
  `AgentsConfig`, con la misma tolerancia a errores (aviso por stderr o `tracing`, y se sigue sin
  presupuesto), y encadenar `.with_budget(..)`. **No** leer el `codeloom.toml` del repo en tests.

**Tests** (módulo de pruebas de `agent/mod.rs`, con `SummaryProvider`/`PlanProvider` como patrón
y un proveedor que declare `Usage`):
- `usage_crossing_the_budget_publishes_a_warning`.
- `the_warning_is_not_repeated_on_the_next_response`.
- `no_budget_means_no_store_read_and_no_warning`.
- `the_turn_is_not_stopped_by_the_warning` (el turno termina con normalidad tras superar el 100 %).

**Commit:** `feat(core): el bucle avisa al cruzar el presupuesto de gasto`.

---

### Tarea 3: mostrarlo en la TUI y documentarlo

**Ficheros:** `crates/codeloom-tui/src/app.rs`, `docs/specs/2026-10-22-codeloom-decalogo-de-contexto-y-coste.md`,
la spec de esta función (sección «Estado»).

**TUI:** el brazo de `LiveEvent::BudgetWarning` en `apply` añade una línea de sistema (mira cómo
se pinta `Error` o un aviso de sistema ya existente y reutiliza ese mecanismo; no inventes otro):
`aviso de presupuesto: {nivel} % de {limite} {unidad} de la sesión ({usado} usados)`; unidad
«tokens» o «USD»; con `partial`, añadir « (según los precios conocidos)». Formatea los tokens con
`format_tokens`. Rótulo de CodeLoom, nunca de A2A.

**Tests:** `budget_warning_renders_as_a_system_line_not_as_user_text`;
`budget_warning_for_cost_mentions_partial_prices`.

**Documentación:** regla 7 del decálogo (pasa a «aviso hecho; sin tope duro por decisión de
producto; los hijos no heredan el presupuesto») y sección «Estado» de la spec, con commits y lo
que no consta. Prosa en español formal.

**Commit:** `feat(tui): aviso de presupuesto de gasto` y `docs: estado real del aviso de presupuesto`.
