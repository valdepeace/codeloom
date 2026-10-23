# Showcase de Batten con el tema primero: plan de implementación

> Para quien ejecute: una tarea cada vez, con verificación y commit al terminar cada una.

**Meta:** selector de tema en la cabecera, barra lateral de componentes del tema elegido y ratón
completo.

**Spec:** `docs/specs/2026-10-23-batten-showcase-theme-first-design.md` (léase primero). Parte de
`2026-10-23-batten-showcase-theme-matrix-design.md` (matriz, `Cell`, `COLUMNS`, `build`, `target`).

## Restricciones globales

Las de `2026-10-23-batten-showcase-theme-matrix.md` (entorno de build, LF, `rustfmt --edition 2021
<ruta>` sobre ficheros concretos —ojo: sobre `main.rs` del showcase sigue los `mod` hijos, comprueba
`git status`—, no tocar `codeloom.toml`, `git add` de rutas concretas, comentarios en español
formal, `cargo test -p batten --examples`, `cargo clippy -p batten -- -D warnings`). No se tocan los
demos ni el orden del catálogo. Los tests de navegación y de matriz que ya existen se adaptan solo
cuando el diseño anterior desaparece (`F8`, `p`, filtro de proveedor), marcándolos
`// ADAPTADO: <motivo>` o eliminándolos con el motivo en el commit; los demás no se tocan.

---

### Tarea 1: derivación de la lista de un tema

**Ficheros:** `crates/batten/examples/showcase/matrix.rs` (o un `theme_view.rs` nuevo si queda más
limpio, con `mod theme_view;` en `main.rs`).

```rust
pub struct ThemeEntry {
    pub component: &'static str,
    pub generic: bool,                 // true = solo el genérico de Batten (◐)
    pub examples: Vec<ThemeExample>,   // al menos uno
}
pub struct ThemeExample { pub catalog_index: usize, pub label: String }
pub struct ThemeView { pub entries: Vec<ThemeEntry>, pub missing: usize }
pub fn theme_view(items: &[CatalogItem], column: usize) -> ThemeView;
```

Reglas: usa `build`/`Cell` ya existentes. `Own(_)` → los demos del proveedor de la columna, en
orden de catálogo; `Generic` → los demos del proveedor Batten de ese componente; `Missing` no
entra y suma a `missing`. `label` = variante sin el prefijo del proveedor y sin separadores
iniciales (` · `, ` - `, espacios); vacío → `principal`. Orden de `entries` = el de las filas de la
matriz.

**Tests:** `crush_lists_its_own_components` (Composer es propio, `generic == false`);
`pawan_lists_composer_as_generic_with_the_batten_demo`; `missing_components_are_not_listed_but_counted`;
`label_strips_the_provider_prefix` (`Crush · pending` → `pending`; `Crush` → `principal`);
`mastracode_has_only_generic_entries`; `every_example_points_to_an_existing_demo`;
`entries_plus_missing_equal_the_matrix_rows`.

**Commit:** `feat(batten): lista de componentes de un tema para el showcase`.

---

### Tarea 2: estado y teclas con el tema primero

**Ficheros:** `crates/batten/examples/showcase/app.rs`.

- `Showcase` gana `theme_column: usize` (índice en `matrix::COLUMNS`, por defecto 0) y la
  `ThemeView` vigente (recalculada al cambiar de tema). `theme_preset` pasa a derivarse de la
  columna. Se **eliminan** `provider_filter`, `Focus::ProviderFilter`, `view_column`,
  `cycle_component_theme`, `F8` y la tecla `p`, y `cycle_provider`/`providers`/`select_provider`.
- Selección = (entrada, ejemplo). Las filas del índice son: por cada entrada, una fila de
  componente; **si tiene más de un ejemplo**, una fila hija por ejemplo debajo (la fila del
  componente no es seleccionable en ese caso o selecciona el primero: decide una y documéntala);
  si tiene uno, la fila del componente es la seleccionable. `index_rows()` devuelve filas con su
  `catalog_index` opcional, como hoy, para que el dibujo y el ratón la reutilicen.
- `set_theme_column(c)` y `cycle_theme_column(backwards)`: recalculan la vista, conservan el
  componente si existe en el nuevo tema (con su primer ejemplo) y, si no, seleccionan el primero.
  `←`/`→` en `Focus::Index` y `Focus::Demo` (en el demo solo si el demo no consume la tecla: mira
  cómo se enruta `handle_key_event` y no rompas los demos que usan flechas; si hay conflicto,
  limita `←`/`→` a `Focus::Index` y deja `F7`/`Shift+F7` para el demo) y `F7`/`Shift+F7` en todas.
- El filtro de texto (`/`) filtra por nombre de componente y de ejemplo dentro del tema.
- La matriz (`m`) y su `Enter` usan `set_theme_column` + selección del ejemplo objetivo.

**Tests:** `changing_theme_keeps_the_component_when_it_exists`;
`changing_theme_selects_the_first_when_it_does_not`; `left_right_cycle_the_theme_with_wrap`;
`f7_still_cycles_the_theme`; `a_component_with_several_examples_has_child_rows`;
`a_component_with_one_example_has_no_child_rows`; `text_filter_matches_component_names_in_the_theme`;
`matrix_enter_switches_theme_and_selects_the_example`. Se adaptan o eliminan los de `F8`, `p` y
proveedor.

**Commit:** `refactor(batten): el showcase elige tema y lista sus componentes`.

---

### Tarea 3: dibujo (cabecera, índice y pie)

**Ficheros:** `crates/batten/examples/showcase/main.rs` (y `matrix.rs` para reutilizar
`status_note`/`glyph`).

- Cabecera, primera fila: pestañas con los siete temas (`COLUMNS`), la activa resaltada (Accent +
  BOLD); en terminal estrecha, recortadas con `…` sin partir caracteres. Segunda fila: el filtro de
  texto de siempre (`Filter: componente · ejemplo`). La fila de proveedores desaparece.
- Índice: una fila por componente con `●` o `◐` delante (estilo Success / Warning de
  `draw_matrix`), las filas hijas sangradas con su etiqueta, la seleccionada marcada como hoy.
- Pie: línea 1, `Tema: {nombre} · {n} componentes: {a} ● · {b} ◐ · faltan {m} (m: matriz)`; línea
  2, solo si el seleccionado es genérico: la nota «◐ … parcialmente implementado (genérico de
  Batten con la paleta de {tema})» (`status_note`). Sin genérico, el pie vuelve a una línea.
  Elimina la tira de F8 y `component_view`.
- Ayuda de cabecera de cada foco actualizada (`←/→: tema`, sin `F8` ni `p`).

**Tests** con `TestBackend` sobre el catálogo real: `tabs_show_the_seven_themes_and_mark_the_active`;
`index_marks_generic_components`; `index_indents_child_examples`;
`footer_counts_components_and_missing`; `footer_notes_partial_implementation_for_generic`;
`footer_is_one_line_for_an_own_component`; `narrow_terminal_does_not_panic` (60×20 y 30×8).

**Commit:** `feat(batten): pestañas de tema, índice de componentes y pie de recuento`.

---

### Tarea 4: ratón

**Ficheros:** `crates/batten/examples/showcase/main.rs` (`handle_mouse` y el bucle),
`app.rs` si hace falta exponer el cursor de la matriz.

- El bucle atiende `ScrollUp`/`ScrollDown` además de `Down(Left)`.
- Clic en una pestaña: `set_theme_column`. Rueda sobre el índice: mueve la selección de 3 en 3
  (con tope). Rueda sobre la matriz: mueve el cursor de fila. Clic en el índice: selecciona la fila
  y abre el demo (fila hija o componente de un ejemplo). Clic en el panel del demo: foco al demo.
- Con `Focus::Matrix`, un clic en una celda mueve el cursor a ella; un clic fuera del cuerpo no
  cierra la matriz ni cambia el foco.
- Las posiciones salen **de la misma geometría que el dibujo**: extrae el cálculo de áreas
  (`filters`, `body`, `index`, `panel`) a una función compartida por `draw` y `handle_mouse` para
  que no puedan divergir.

**Tests:** `click_on_a_tab_switches_the_theme`; `click_on_an_index_row_selects_and_opens_it`;
`click_on_a_child_example_selects_that_example`; `wheel_scrolls_the_index_with_a_cap`;
`wheel_on_the_matrix_moves_the_cursor`; `click_on_a_matrix_cell_moves_the_cursor_and_keeps_the_matrix`;
`click_outside_the_matrix_does_not_close_it`; `mouse_and_draw_share_the_same_geometry` (para varios
tamaños, la fila bajo el clic coincide con la que se pintó).

**Commit:** `feat(batten): ratón completo en el showcase`.

---

### Tarea 5: documentación

«Estado» de la spec nueva (commits, cifras por tema, lo que no se vio en terminal real, y qué acción
del ratón fallaba si la persona usuaria la ha concretado) y nota de reemplazo en la spec de la
matriz (qué parte queda obsoleta: `F8`, `p`, fila de proveedores, tira del pie). Prosa en español
formal.

**Commit:** `docs: estado real del showcase con el tema primero`.
