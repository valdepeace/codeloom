# Selección en el transcript y bloques — Plan de implementación

> **Para agentes:** sub-skill requerida: `superpowers:subagent-driven-development` (recomendada) o `superpowers:executing-plans`. Los pasos usan casillas `- [ ]`. Una tarea cada vez, en orden. Al terminar y verificar cada tarea se commitea (regla del usuario; lo hace quien orquesta).

**Meta:** arrastrar con el ratón sobre el transcript selecciona y copia texto por caracteres, y se navega y copia por bloques con un bloque activo visible.

**Arquitectura:** selección en coordenadas de documento `(fila, columna)` sobre el `Layout` de filas del transcript; resaltado por parcheo del búfer ya pintado; texto por re-render fuera de pantalla solo al copiar. Bloque activo = índice de entrada del `Transcript`.

**Stack:** Rust, `ratatui`, `crossterm`.

**Spec:** `docs/specs/2026-10-21-codeloom-transcript-selection-blocks-design.md` (leerla antes). Contexto previo: `docs/specs/2026-10-21-codeloom-mouse-focus-selection-design.md` (composer: `mouse_down/mouse_drag_to/mouse_up`, `copy_composer_selection`, `app.clipboard`, `pane_rects`, `pane_at`, `Focus`).

## Restricciones globales

- `codeloom-core` no depende de `ratatui`/`crossterm`. Sin red en tests.
- Comprobaciones obligatorias: `cargo build`, `cargo test`, `cargo fmt --all --check`, `cargo clippy -- -D warnings`.
- Entorno de build (Git Bash): `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe' CARGO_TARGET_DIR='E:/workspace/personal/loom/target' CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`. No crear otros `target*`. Formatear solo con `rustfmt --edition 2021 <ruta>` (nunca `cargo fmt --all` para escribir). Ficheros en LF (`tr -cd '\r' <f | wc -c` = 0). Sin `git checkout/restore/stash/reset`.
- Comentarios en español «pijo», sin tacos ni andalucismos.
- La fluidez del composer y del dibujado es prioridad máxima: el resaltado no re-renderiza; el arrastre no asigna más de lo imprescindible.
- No cambiar el significado de `j/k`, PgUp/PgDn, `u/d`, `g/G`, `v` ni la selección de líneas existente (`app.selection`, `batten::state::Selection`).

## Review Focus

- Selección con rango invertido (arrastrar hacia arriba/izquierda) se normaliza.
- Cambio de ancho del diseño o sesión nueva: la selección se limpia, nunca apunta fuera de rango.
- Caracteres anchos y emoji: no se parten ni se copian a medias.
- Transcript vacío: clic/arrastre/`[`/`]` no entran en pánico.
- Arrastre fuera del área acota y autodesplaza, sin pánico con áreas de 1 fila.
- Con diálogo abierto el ratón se sigue ignorando (guarda existente).

---

### Tarea 1: núcleo en `batten` — modelo, traducción, resaltado y extracción

**Ficheros:** crear `crates/batten/src/state/text_selection.rs` (exportar en `state/mod.rs`); modificar `crates/batten/src/widgets/transcript.rs`.

**Interfaces — produce:**
```rust
// state/text_selection.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellPos { pub row: usize, pub col: u16 }   // coordenadas de documento (fila del Layout, columna del texto)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextSelection { /* anchor, head: CellPos */ }
impl TextSelection {
    pub fn new(at: CellPos) -> Self;                      // ancla = cabeza
    pub fn set_head(&mut self, head: CellPos);
    pub fn is_empty(&self) -> bool;                       // ancla == cabeza
    pub fn ordered(&self) -> (CellPos, CellPos);          // (inicio, fin) por (row, col)
}
// widgets/transcript.rs
pub fn hit_cell(layout: &Layout, viewport: &Viewport, area: Rect, x: u16, y: u16) -> Option<CellPos>;
pub fn paint_text_selection(buf: &mut Buffer, area: Rect, layout: &Layout, viewport: &Viewport, sel: &TextSelection, style: Style);
pub fn selected_text(transcript: &Transcript, layout: &Layout, theme: &Theme, sel: &TextSelection) -> String;
```
Semántica:
- `hit_cell`: `area` es el área en la que se pasó `render`; el ancho de texto es `layout.width()` (puede ser `area.width` o `area.width - 1` por la scrollbar). `row = viewport.top() + (y - area.y)`, `col = x - area.x`; ambos **acotados**: fila a `[0, layout.total_rows()-1]`, columna a `[0, layout.width()-1]`; `y` fuera del área (por encima/debajo) se acota a la primera/última fila visible. `None` si `area` vacía o `layout.total_rows() == 0`. Consulta pura: no modifica el viewport.
- `paint_text_selection`: parchea (`Style::patch`) con `style` las celdas del búfer de `area` que caen dentro de la selección normalizada y de las filas visibles (`viewport.top()..viewport.top()+area.height`). La primera fila desde `start.col`, la última hasta `end.col` (exclusiva), las intermedias enteras (hasta el ancho de texto). Selección vacía: no pinta.
- `selected_text`: renderiza fuera de pantalla las líneas del transcript que cubren las filas de la selección (usar `render_line` y `Paragraph` + `Wrap { trim: false }` sobre un `Buffer` de ancho `layout.width()`, mismo recorrido que `render`), lee las celdas del rango, no parte caracteres anchos (omitir la celda de continuación), recorta espacios finales de cada fila, y une las filas con `\n`. Devuelve `String` vacío si la selección está vacía.
- El rango de filas pedido se acota a `[0, total_rows)`.

- [ ] **Paso 1: tests en rojo** (indicar si el rojo es de compilación o de aserción). En `text_selection.rs`: `ordered_swaps_when_head_precedes_anchor`, `new_selection_is_empty`. En `widgets/transcript.rs` (`mod tests`, con `TestBackend`): `hit_cell_maps_screen_to_document_with_scroll`, `hit_cell_clamps_outside_the_area`, `hit_cell_none_when_empty`, `paint_selects_partial_first_and_last_rows` (comprobar `bg`/estilo de celdas concretas), `paint_selection_ignores_rows_out_of_view`, `selected_text_multirow_trims_trailing_spaces`, `selected_text_includes_wrapped_rows_joined_by_newline`, `selected_text_does_not_split_wide_chars`, `selected_text_empty_selection_is_empty`.
- [ ] **Paso 2: implementar.** **Paso 3: verificar** `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --all --check`. (El commit lo hace quien orquesta.)

---

### Tarea 2: TUI — enrutado de la selección de texto en el transcript

**Ficheros:** `crates/codeloom-tui/src/app.rs`, `render.rs`, `run.rs`.

**Interfaces — consume:** T1; `App.layout`, `App.viewport`, `App.pane_rects`, `pane_at`, `copy_composer_selection` (modelo para copiar y avisar `Copied N chars` vía `app.clipboard`).
**Produce:**
```rust
// App
pub text_sel: Option<batten::state::TextSelection>;
pub text_drag: bool;                       // arrastre del transcript en curso
pub transcript_area: Option<Rect>;         // área pasada a transcript::render, la fija draw_transcript
pub fn transcript_mouse_down(&mut self, x: u16, y: u16);
pub fn transcript_mouse_drag_to(&mut self, x: u16, y: u16);   // incluye autodesplazamiento 1 fila por evento si y <= area.y o y >= area.bottom()-1 ... fuera o en el borde
pub fn transcript_mouse_up(&mut self);     // copia si hay selección no vacía; clic simple limpia
pub fn copy_transcript_selection(&mut self) -> bool;
```
Comportamiento:
- `mouse_down` existente: si el clic cae en `Focus::Transcript` (vía `pane_at`) → `focus = Transcript`, `transcript_mouse_down` (limpia `text_sel` previa; crea `TextSelection::new(hit_cell(..))`; `text_drag = true`); el resto de paneles se comporta como hoy. `mouse_drag_to`/`mouse_up` enrutan al transcript cuando `text_drag`.
- El resaltado se aplica en `draw_transcript` justo tras `transcript::render`, con `theme.styles.selection` sobre `transcript_area`.
- `mouse_up`: si hubo arrastre y la selección no está vacía → `copy_transcript_selection` (texto con `selected_text`, aviso `Copied N chars` con el mismo mecanismo que el composer, `app.clipboard`); la selección **permanece resaltada**. Clic simple (vacía) → `text_sel = None`.
- Se limpia `text_sel`: al cambiar `layout.width()` respecto al valor en que se creó (guardar el ancho en `App` o comparar en `draw_transcript`), al cargar sesión, con Esc (sin contar para la doble Esc; con prioridad sobre el resto de usos de Esc salvo diálogos), y ante cualquier tecla que edite el composer.
- `Ctrl+C` con `text_sel` no vacía copia y no sale (como con selección del composer; la del composer tiene prioridad si ambas existieran, que no debería ocurrir: limpiar la otra al crear una).
- Selección de texto y selección de líneas (`app.selection`) son excluyentes: al crear una se anula la otra.
- Con diálogos abiertos el ratón sigue ignorado y descarta `text_drag`.

- [ ] **Paso 1: tests en rojo:** `click_in_transcript_focuses_and_starts_selection`, `drag_selects_text_and_mouse_up_copies_with_notice`, `simple_click_clears_selection_and_copies_nothing`, `drag_above_area_autoscrolls_and_clamps`, `esc_clears_text_selection_without_double_esc`, `ctrl_c_with_text_selection_copies_and_does_not_quit`, `width_change_clears_text_selection`, `text_selection_excludes_line_selection`, `dialog_open_ignores_transcript_mouse`, `drag_on_empty_transcript_does_not_panic`, y un test de `render` que compruebe que las celdas seleccionadas llevan `theme.styles.selection`.
- [ ] **Paso 2: implementar.** **Paso 3: verificar** suite completa, clippy, fmt.

---

### Tarea 3: TUI — bloque activo y navegación por bloques

**Ficheros:** `crates/codeloom-tui/src/app.rs`, `run.rs`, `render.rs` (y `crates/batten/src/widgets/transcript.rs` si el marcador se pinta allí).

**Interfaces — produce:**
```rust
// App
pub block: Option<usize>;                  // índice de entrada (Transcript::entry) activa
pub fn block_prev(&mut self);              // `[`
pub fn block_next(&mut self);              // `]`
pub fn block_text(&self) -> Option<String>; // texto plano del bloque activo (líneas de la entrada unidas por `\n`)
pub fn copy_block(&mut self) -> bool;      // aviso `Copied N chars`
```
Comportamiento:
- En `transcript_key` (foco Transcript, sin selección de líneas activa): `[` → `block_prev`, `]` → `block_next`; `y`/`c` → `copy_block` si hay bloque activo. Si no hay bloque activo, `block_prev` activa el último y `block_next` el último también (acotado); se acota a `[0, entry_count)`; sin entradas, no hace nada.
- Tras mover, el viewport se desplaza para mostrar el bloque entero (`viewport.ensure_visible(row_inicio, row_fin)` con `layout.row_of(entry_start)` y la suma de alturas de sus líneas); si el bloque es mayor que la vista, queda visible su comienzo. Salir del seguimiento del final (`follow_bottom`) solo cuando se navega hacia atrás.
- Al activar el foco del transcript por clic (T2) el bloque activo pasa a ser la entrada de la línea pulsada (`transcript.locate(line)`); un clic ya no deja de crear la selección de texto.
- El bloque activo se limpia (`None`) al cargar sesión; si `block >= entry_count` se acota al pintar.
- Marcador visible solo con el transcript enfocado: en chrome `Flat` (hay columna de margen libre a la izquierda del área de texto) se pinta `▎` con `theme.styles.accent` en esa columna para cada fila del bloque activo visibles; sin margen, se recolorea con `accent` la primera celda de cada fila del bloque. No altera el texto ni el diseño.
- No cambia nada de `j/k`, `v`, PgUp/PgDn, `u/d`, `g/G`.

- [ ] **Paso 1: tests en rojo:** `bracket_keys_move_the_active_block`, `block_nav_is_clamped_and_noop_on_empty_transcript`, `block_nav_scrolls_block_into_view`, `copy_block_copies_entry_text_with_notice`, `click_on_transcript_activates_the_clicked_block`, `j_k_still_scroll_by_lines`, `marker_is_drawn_for_the_active_block_in_flat_chrome`, `marker_not_drawn_when_transcript_unfocused`, `block_cleared_on_session_load`.
- [ ] **Paso 2: implementar.** **Paso 3: verificar** suite completa, clippy, fmt.

---

### Tarea 4: documentación

- [ ] Sección «Estado» en `docs/specs/2026-10-21-codeloom-transcript-selection-blocks-design.md` (qué se implementó, decisiones al ejecutar, límites conocidos, validación visual pendiente) y línea en el listado de teclas/ayuda si existe (`help_overlay`/`key_help`) con `[`/`]`.

## Autoauditoría

- Cobertura: objetivos 1 (T1+T2), 2 (T2), 3 y 4 (T3), 5 (T2+T3), 6 (T2).
- Tipos coherentes: `CellPos`, `TextSelection`, `hit_cell`, `paint_text_selection`, `selected_text`, `text_sel`, `text_drag`, `transcript_area`, `block` se nombran igual en todas las tareas.
- Riesgo: validación visual en terminal real pendiente.
