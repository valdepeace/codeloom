# Medidor de contexto: plan de implementación

> Para quien ejecute: una tarea cada vez, con verificación y commit al terminar cada una.

**Meta:** widget de batten con dos estilos, demo en el showcase y barra en el pie de la app.

**Spec:** `docs/specs/2026-10-23-codeloom-context-meter-design.md` (léase primero).

## Restricciones globales

Las de `2026-10-23-codeloom-context-summary.md` (entorno de build, LF, `rustfmt --edition 2021
<ruta>` sobre ficheros concretos, no tocar `codeloom.toml`, `git add` de rutas concretas,
comentarios en español formal, clippy sin `--all-targets`). `batten` NO depende de `codeloom-core`.
La salida del medidor se acota por construcción (ancho fijo de celdas, cifras formateadas).

---

### Tarea 1: widget `context_meter` en batten

**Ficheros:** crear `crates/batten/src/widgets/context_meter.rs`; `pub mod context_meter;` en
`crates/batten/src/widgets/mod.rs` (orden alfabético).

**Produce (firmas exactas):**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeterStyle { Segments, Blocks }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeterLevel { Normal, Warn, Critical }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextMeter {
    pub used: u64,
    pub window: Option<u64>,
    pub mark_percent: u8,   // 80 por defecto en `ContextMeter::new`
}
impl ContextMeter {
    pub fn new(used: u64, window: Option<u64>) -> Self;
    pub fn percent(&self) -> Option<u32>;        // redondeado; None sin ventana; sin tope superior
    pub fn level(&self) -> MeterLevel;           // < mark: Normal; >= mark: Warn; >= 100: Critical; sin ventana: Normal
}

/// Texto y celdas del medidor, sin estilos: para medir ancho y para probar.
pub fn meter_text(meter: &ContextMeter, style: MeterStyle) -> String;
pub fn meter_width(meter: &ContextMeter, style: MeterStyle) -> u16;
/// Spans con estilo según el nivel y el tema.
pub fn meter_spans(meter: &ContextMeter, style: MeterStyle, theme: &Theme) -> Vec<Span<'static>>;
/// Pinta el medidor alineado a la derecha de `area`.
pub fn render(frame: &mut Frame, area: Rect, meter: &ContextMeter, style: MeterStyle, theme: &Theme);
```

**Comportamiento de `meter_text`:**
- `used == 0` → cadena vacía (no se pinta nada).
- `window == None` → `ctx {used} (sin ventana)` con `used` formateado (`142k`, `1.2M`, `850`).
- `Segments` con ventana: 10 celdas `━`; ocupadas = `round(min(pct,100)/10)`; luego dos espacios,
  `{used}/{window}` formateados, dos espacios y `{pct} %`. Ej.: `━━━━━━━───  142k/200k  71 %`
  (las libres son también `━`, distinguidas solo por el estilo; en texto plano son `━` las diez).
  **Atención:** `meter_text` es texto plano, así que las diez celdas salen como `━`; el reparto
  ocupadas/libres se ve únicamente en `meter_spans` (dos spans de estilo distinto).
- `Blocks` con ventana: `ctx {pct}% ` seguido de diez caracteres `█`/`░` (ocupadas `█`).
- Un `pct` superior al 100 se muestra tal cual (`123 %`), la barra se llena entera.

**Estilos de `meter_spans`:** ocupadas con el color del nivel (neutro = `theme.styles.text` o el
más cercano; `Warn` = estilo de aviso del tema; `Critical` = estilo de error del tema), libres con
`theme.styles.muted`, texto con el estilo neutro. Mira `Theme`/`styles` y `status_bar.rs` para
usar los campos reales; no inventes colores sueltos.

**Tests:**
- `percent_and_level_follow_the_mark`: 79 % Normal, 80 % Warn, 99 % Warn, 100 % Critical, 130 %
  Critical, sin ventana Normal.
- `no_usage_renders_nothing` (`used == 0`).
- `unknown_window_renders_no_bar` (contiene «sin ventana» y ningún `━`/`█`).
- `segments_text_has_ten_cells_and_the_fraction` (formato exacto con 142 000/200 000).
- `blocks_text_matches_the_pawan_look` (71 % → `ctx 71% ███████░░░`).
- `over_100_percent_fills_the_bar_and_shows_the_real_number`.
- `spans_split_filled_and_free_cells_with_different_styles`.
- `width_matches_the_text` (`meter_width == meter_text.width()`).
- `render_right_aligns_inside_the_area` (con `TestBackend`, mira cómo prueban otros widgets).

**Commit:** `feat(batten): medidor de contexto con dos estilos`.

---

### Tarea 2: demo en el showcase

**Ficheros:** crear `crates/batten/examples/showcase/demos/context_meter.rs`; registrarlo en
`crates/batten/examples/showcase/demos/mod.rs` (declaración `mod` y un `item("Context meter",
"Batten", ...)` junto a los de «Status bar»).

**Comportamiento:** imita a `pawan_status_bar.rs` (impl de `Demo`). Muestra, apilados y rotulados,
los dos estilos con la misma ventana de 200 000: el uso **avanza solo** de 0 a 120 % y vuelve a
empezar (usa el reloj/animación que ya usen otros demos; mira `fx.rs` o `dashboard.rs`), y debajo
tres filas fijas de referencia: 50 %, 85 % (aviso) y 105 % (crítico), más una con «sin ventana».
Una línea de texto explica: «Warn desde el 80 %: ahí salta la compactación».

**Tests:** el showcase ya tiene pruebas de ejemplos (`cargo test -p batten --examples`); añade al
menos una que renderice el demo en un `TestBackend` de 80×12 y compruebe que aparecen `━` y `█`.

**Commit:** `feat(batten): demo del medidor de contexto en el showcase`.

---

### Tarea 3: la barra en el pie de la app

**Ficheros:** `crates/codeloom-tui/src/app.rs`, `crates/codeloom-tui/src/render.rs`.

**App:**
- Campo `context_tokens: u64` (0 al inicio, y a 0 al abrir otra sesión).
- `LiveEvent::Usage { input_tokens, .. }`: si `input_tokens > 0`, `context_tokens = input_tokens`.
- `restore_messages`: `context_tokens` = `usage.input_tokens` del último mensaje de asistente que
  lo lleve (0 si ninguno).
- `fn context_meter(&self) -> batten::widgets::context_meter::ContextMeter`, con la ventana de
  `catalog.context_window(base_url, model)` por el mismo camino que `model_prices` (cópialo).

**Render:** en los dos pies (`draw_flat_footer` y el pie clásico de la línea ~1394) reservar a la
derecha `meter_width` columnas con `Layout::horizontal([Min(0), Length(w)])` y pintar el medidor
con `MeterStyle::Segments`, **solo** si `area.width >= 80` y el medidor no está vacío. La parte
izquierda se recorta como hoy.

**Tests:** `usage_event_sets_the_context_tokens` (el último gana; un `input_tokens == 0` no lo
pisa); `restore_messages_recovers_context_tokens`; `footer_shows_the_meter_when_wide_enough` y
`footer_hides_the_meter_when_narrow` (80 y 79 columnas, con `TestBackend`); `unknown_window_shows_no_bar`.

**Commit:** `feat(tui): medidor de contexto en el pie de la app`.

---

### Tarea 4: documentación

Sección «Estado» de la spec (commits, lo no comprobado en terminal real) y una línea en el
decálogo (regla 9, observabilidad). Prosa en español formal.

**Commit:** `docs: estado real del medidor de contexto`.
