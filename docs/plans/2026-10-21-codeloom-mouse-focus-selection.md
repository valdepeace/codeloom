# Foco por clic y selección en el composer — Plan de implementación

> **Para agentes:** sub-skill requerida: `superpowers:subagent-driven-development` (recomendada) o `superpowers:executing-plans`. Los pasos usan casillas `- [ ]`. Una tarea cada vez, en orden. Al terminar y verificar cada tarea se commitea (regla del usuario).

**Meta:** que un clic dirija el foco a la ventana pulsada y que arrastrar en el composer seleccione solo su texto, con copia al soltar.

**Arquitectura:** selección en el modelo puro del composer (`batten`), resaltado y traducción posición→cursor en sus renderers, mapa de ventanas por fotograma en la `App` y enrutado de eventos de ratón y de Shift+flechas en `run.rs`.

**Stack:** Rust, ratatui, crossterm, `unicode-segmentation`, `unicode-width`.

**Spec:** `docs/specs/2026-10-21-codeloom-mouse-focus-selection-design.md` (leerla antes).

## Restricciones globales

- `batten` no depende de `crossterm`. `codeloom-core` no se toca.
- Comprobaciones obligatorias: `cargo build`, `cargo test`, `cargo fmt --all --check`, `cargo clippy -- -D warnings`.
- Entorno de build (Git Bash): `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe' CARGO_TARGET_DIR='E:/workspace/personal/loom/target' CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`. No crear otros `target*`. Formatear solo con `rustfmt --edition 2021 <ruta>`. Ficheros en LF.
- Comentarios en español «pijo», sin tacos ni andalucismos.
- **La fluidez del composer es prioridad máxima:** nada de trabajo costoso por evento de ratón; teclear al final del texto debe comportarse exactamente como hoy.

## Review Focus

- Una selección nunca debe quedar fuera de rango tras `set_text`, `submit`, `backspace`, pegado o historial: toda operación que cambie el texto la normaliza o la elimina.
- Selección y cursor se miden en grafemas; hay que probar emoji, acentos combinados y caracteres anchos (CJK) sin partir un grafema.
- Texto multilínea y con ajuste de línea (Crush): arrastrar a través de saltos de línea lógicos y visuales selecciona lo esperado.
- El desplazamiento estable no debe cambiar el comportamiento actual al escribir al final del texto (el cursor sigue visible y la vista no «salta»).
- Clic en un hueco entre ventanas, fuera del terminal o con diálogos abiertos no cambia el foco ni selecciona nada.
- Esc y Ctrl+C conservan su significado actual cuando no hay selección; Esc con selección solo la limpia y no cuenta para la salida por doble Esc.
- El arrastre iniciado en el composer no selecciona nada fuera de él (la posición se acota al área del composer).

---

### Tarea 1: modelo de selección en `Composer` (batten, puro)

**Ficheros:** `crates/batten/src/state/composer.rs`.

**Interfaces — produce** (posiciones en **grafemas**, igual que `cursor()`):
```rust
pub fn anchor(&self) -> Option<usize>;                 // Some si hay selección activa (aunque vacía se normaliza a None)
pub fn selection(&self) -> Option<(usize, usize)>;     // rango [inicio, fin) en grafemas, ordenado; None si vacío
pub fn selection_bytes(&self) -> Option<(usize, usize)>; // mismo rango en bytes del buffer
pub fn selected_text(&self) -> Option<&str>;
pub fn select_range(&mut self, anchor: usize, head: usize); // fija ancla y mueve el cursor a head (ambos acotados)
pub fn set_cursor(&mut self, pos: usize);               // mueve el cursor (acotado) y limpia la selección
pub fn select_all(&mut self);
pub fn clear_selection(&mut self);
pub fn extend_left(&mut self); pub fn extend_right(&mut self);
pub fn extend_home(&mut self); pub fn extend_end(&mut self);
pub fn extend_up(&mut self);   pub fn extend_down(&mut self);
```
Semántica: `extend_*` fija el ancla en la posición actual si no hay y mueve el cursor como el movimiento normal homónimo. `selection()` devuelve `None` si ancla == cursor. Los movimientos normales (`move_left`, `move_right`, `move_home`, `move_end`, `move_up`, `move_down`) limpian la selección. `insert_char`, `insert_str`, `newline`, `backspace`, `delete` **reemplazan o eliminan la selección primero** (con selección, `backspace` y `delete` solo borran el rango y no más). `set_text` y `submit` limpian la selección.

- [ ] **Paso 1: tests en rojo** (indicar si el rojo es de compilación o de aserción): `extend_right_selects_graphemes`, `extend_then_move_clears_selection`, `selection_is_ordered_regardless_of_direction`, `typing_replaces_the_selection`, `backspace_and_delete_remove_only_the_selection`, `paste_insert_str_replaces_the_selection`, `newline_replaces_the_selection`, `set_text_and_submit_clear_the_selection`, `select_range_clamps_out_of_range_positions`, `selection_never_splits_a_grapheme` (emoji compuesto `👨‍👩‍👧`, `e` + acento combinado, un carácter CJK), `selected_text_spans_multiple_lines`, `selection_bytes_matches_selection`, `extend_up_down_follow_preferred_column`.
- [ ] **Paso 2: implementar** reutilizando la lógica de movimiento existente (no duplicar). **Paso 3: verificar** `cargo test -p batten`, clippy, fmt. **Paso 4: commit** (español pijo + trailer).

---

### Tarea 2: resaltado y desplazamiento estable (batten, render)

**Ficheros:** `crates/batten/src/widgets/composer.rs`, `crates/batten/src/state/composer.rs` (estado de vista).

**Interfaces — produce:** en `Composer`, estado de vista no observable en la API de edición: `view_top: Cell<usize>` (fila visual superior visible del composer Crush) con `pub fn view_top(&self) -> usize` y `pub fn set_view_top(&self, top: usize)`. `render_crush` calcula `top` como el **mínimo desplazamiento** que mantiene visible la fila del cursor partiendo del `view_top` previo (`top = view_top.clamp(cursor_row.saturating_sub(height-1), cursor_row)`, además acotado a `rows.len().saturating_sub(height)`), lo guarda y lo usa. Un helper compartido `fn split_selected<'a>(text: &'a str, row_start: usize, row_end: usize, sel: Option<(usize, usize)>) -> [(&'a str, bool); 3]` (antes, seleccionado, después) y su uso en `render_crush` y en los renderers Classic (`render_focused_with_model`, `render_martty`, `render_mindfork`) para pintar la parte seleccionada con `theme.styles.selection`.

- [ ] **Paso 1: tests en rojo:** `split_selected_*` (sin selección, selección parcial, completa, fuera de la fila, que empieza antes de la fila y acaba dentro), `crush_view_top_stays_when_cursor_moves_inside_the_viewport`, `crush_view_top_follows_the_cursor_past_the_edges`, `crush_typing_at_the_end_scrolls_like_before` (mismo `top` que el cálculo antiguo `cursor_row.saturating_sub(height-1)` cuando se escribe al final), `crush_selection_is_highlighted_with_the_selection_style` y `classic_selection_is_highlighted` (con `TestBackend`, comprobando el estilo de las celdas seleccionadas).
- [ ] **Paso 2: implementar. Paso 3: verificar** suite completa, clippy, fmt. **Paso 4: commit.**

---

### Tarea 3: traducción posición → cursor (batten)

**Ficheros:** `crates/batten/src/widgets/composer.rs` (o `state/composer_layout.rs` si encaja mejor).

**Interfaces — produce:**
```rust
/// Índice de grafema del composer bajo la celda (x, y) del terminal, o el más
/// cercano si cae fuera del texto; None solo si el área es nula.
pub fn hit_index(theme: &Theme, composer: &Composer, area: Rect, x: u16, y: u16) -> Option<usize>;
```
Debe reproducir **exactamente** la geometría de lo que pinta `render_for_look` para el look del tema: Crush (con prefijo `> ` / `::: ` por fila, ajuste por grafemas y `view_top` de la tarea 2) y Classic (borde, prompt y recorte horizontal que use el renderer; léelo y documenta la geometría en un comentario). Las celdas anteriores al texto de la fila (prefijo) caen en el inicio de la fila; las posteriores al final, en el final de la fila; filas por debajo/encima, acotadas a la última/primera visible. Un clic en la mitad derecha de un carácter ancho elige la posición posterior al carácter.

- [ ] **Paso 1: tests en rojo:** `hit_crush_first_row_start_and_end`, `hit_crush_wrapped_rows`, `hit_crush_respects_view_top`, `hit_crush_wide_chars`, `hit_crush_prefix_cells_map_to_row_start`, `hit_crush_out_of_range_is_clamped`, `hit_classic_single_and_multiline`, `hit_classic_border_cells_are_clamped`, `hit_and_cursor_agree` (para cada posición de cursor posible de un texto de varias líneas, la celda donde se pinta el cursor devuelve esa misma posición al aplicar `hit_index`).
- [ ] **Paso 2: implementar. Paso 3: verificar** suite completa, clippy, fmt. **Paso 4: commit.**

---

### Tarea 4: foco ampliado y mapa de ventanas (tui)

**Ficheros:** `crates/codeloom-tui/src/app.rs`, `crates/codeloom-tui/src/render.rs`, `crates/codeloom-tui/src/rail.rs` si hace falta.

**Interfaces — produce:**
```rust
// app.rs
pub enum Focus { Composer, Transcript, Sidebar, Agent, Activity, Rail }   // sustituye al actual
pub pane_rects: Vec<(Focus, Rect)>,        // reescrito en cada frame por render
pub composer_area: Option<Rect>,
pub fn pane_at(&self, x: u16, y: u16) -> Option<Focus>;   // primera ventana cuyo rect contiene el punto; el composer tiene prioridad sobre Transcript (su área está dentro)
```
`render` limpia y rellena `pane_rects` y `composer_area` en cada frame con los rectángulos reales: Classic → Sidebar (panel izquierdo si se pinta), Transcript (panel central), Agent, Activity (y «Token usage» cuenta como Agent), Composer (`rows[4]`); Crush → Transcript, Composer y Rail (si se pinta). Indicador de foco: en paneles con borde, recolorear las celdas del borde con el estilo de acento del tema cuando `app.focus` coincide; en Crush, marcador de acento en la columna de margen si existe (si no, nada). Esc con foco en `Sidebar|Agent|Activity|Rail` devuelve el foco al composer; una tecla imprimible lo devuelve y escribe. Tab no cambia. Revisar los `match`/comparaciones existentes sobre `Focus` (`app.rs:475`, `:808`, `:1585`, `render.rs:251`, `run.rs:444`, `:697`).

- [ ] **Paso 1: tests en rojo:** `render_records_pane_rects_classic_wide`, `render_records_pane_rects_crush_with_and_without_rail`, `pane_at_prefers_the_composer_over_the_transcript`, `pane_at_returns_none_outside_any_pane`, `focused_panel_border_uses_the_accent_style` (celda del borde con foco vs sin foco), `esc_returns_focus_from_side_panes_to_the_composer`, `printable_key_in_a_side_pane_refocuses_and_types`, `tab_still_cycles_composer_and_transcript_only`.
- [ ] **Paso 2: implementar** sin trabajo extra en el bucle. **Paso 3: verificar** suite completa, clippy, fmt. **Paso 4: commit.**

---

### Tarea 5: enrutado del ratón y del teclado (tui)

**Ficheros:** `crates/codeloom-tui/src/run.rs`, `crates/codeloom-tui/src/app.rs`.

**Interfaces — produce (en `App`):** `pub mouse_drag: Option<usize>` (ancla del arrastre en curso), `pub fn mouse_down(&mut self, x: u16, y: u16)`, `pub fn mouse_drag_to(&mut self, x: u16, y: u16)`, `pub fn mouse_up(&mut self)`.

Reglas: solo se atienden `MouseEventKind::{Down(Left), Drag(Left), Up(Left)}` y solo cuando no hay diálogos abiertos (misma condición que hoy usa el brazo `Event::Mouse` para la rueda; la rueda se conserva). `mouse_down`: `pane_at` → `app.focus = pane`; si es el composer, `set_cursor(hit_index(..))`, `mouse_drag = Some(cursor)` y se limpia la selección; si no, `mouse_drag = None`. `mouse_drag_to`: solo con `mouse_drag` activo; acota (x, y) al `composer_area` y llama a `select_range(ancla, hit_index(..))`. `mouse_up`: si hay selección no vacía, `app.clipboard = Some(texto)` (ya lo vuelca el bucle con OSC52) y `slash_feedback = Some("Copied N chars")` (N en caracteres); limpia `mouse_drag`. Teclado: `Shift+Left/Right/Up/Down/Home/End` → `extend_*` con el foco en el composer; `Ctrl+C` con selección copia (mismo mecanismo) y NO sale; sin selección conserva su comportamiento actual; `Esc` con selección la limpia y no cuenta para la doble Esc (como el diálogo de preguntas); pegar con selección la reemplaza (ya lo da el modelo). Con la captura apagada (`/mouse`) no llegan eventos de ratón: no hace falta tratamiento.

- [ ] **Paso 1: tests en rojo:** `click_in_the_composer_moves_the_cursor_and_focuses_it`, `click_in_another_pane_changes_focus_and_does_not_select`, `drag_in_the_composer_selects_text`, `drag_outside_the_composer_is_clamped_to_it`, `release_copies_the_selection_to_the_clipboard`, `click_without_drag_copies_nothing`, `shift_arrows_extend_the_selection`, `ctrl_c_with_selection_copies_and_does_not_quit`, `ctrl_c_without_selection_keeps_its_behavior`, `esc_with_selection_clears_it_and_does_not_count_for_double_esc`, `mouse_is_ignored_while_a_dialog_is_open`, `wheel_still_scrolls_the_transcript`, `typing_after_selecting_replaces_the_selected_text`.
- [ ] **Paso 2: implementar** (el arrastre se resuelve por evento y es O(filas); no hacer más trabajo). **Paso 3: verificar** suite completa, clippy, fmt. **Paso 4: commit.**

---

### Tarea 6: documentación

- [ ] Sección «Estado» en la spec (qué se implementó, decisiones al ejecutar, límites conocidos, validación visual pendiente) y nota sobre `/mouse` y la selección nativa. **Commit.**

## Autoauditoría

- Cobertura de la spec: objetivos 1 (T4+T5), 2 (T3+T5), 3 (T1+T5), 4 (T5), 5 (T1), 6 (T2+T3).
- Tipos coherentes: `selection`, `selection_bytes`, `select_range`, `set_cursor`, `extend_*`, `view_top`, `hit_index`, `Focus`, `pane_rects`, `composer_area`, `pane_at`, `mouse_down`, `mouse_drag_to`, `mouse_up`.
- Riesgo principal: la geometría del composer Classic al traducir posición→cursor (T3) y el desplazamiento estable de Crush (T2); ambos tienen pruebas de coherencia con el cursor pintado.
