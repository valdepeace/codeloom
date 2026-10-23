# Matriz de componentes por tema del showcase: plan de implementación

> Para quien ejecute: una tarea cada vez, con verificación y commit al terminar cada una.

**Meta:** modo «matriz» en el showcase de Batten: filas = componentes, columnas = temas, celdas
`●` / `◐` / `·`.

**Spec:** `docs/specs/2026-10-23-batten-showcase-theme-matrix-design.md` (léase primero).

## Restricciones globales

Entorno de build (Git Bash): `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program
Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe'
CARGO_TARGET_DIR='E:/workspace/personal/loom/target' CARGO_PROFILE_DEV_DEBUG=0
CARGO_INCREMENTAL=0`. No crear otros `target*`. Ficheros en LF. Formatear solo con `rustfmt
--edition 2021 <ruta>` sobre los ficheros tocados (nunca `cargo fmt --all`). No tocar
`codeloom.toml`. `git add` de rutas concretas. Comentarios en español formal. Clippy sin
`--all-targets`: `cargo clippy -p batten -- -D warnings`. Los tests del showcase se ejecutan con
`cargo test -p batten --examples`. Hay lints preexistentes de `clippy --examples` en otros demos:
ajenos a esto. `batten` NO depende de `codeloom-core`. No se modifican los demos existentes ni su
orden en el catálogo.

---

### Tarea 1: derivación pura de la matriz

**Ficheros:** crear `crates/batten/examples/showcase/matrix.rs`; `mod matrix;` en
`crates/batten/examples/showcase/main.rs`; en `demos/mod.rs`, una función nueva
`pub fn strict_provider(variant: &str) -> Option<&'static str>` (los mismos ocho nombres que
`provider_for`, pero `None` si la variante no empieza por ninguno). **`provider_for` y
`CatalogItem` no cambian.**

**Produce (firmas exactas):**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cell { Own(usize), Generic, Missing }

/// Columnas de la matriz, en este orden, y su preset (None = sin preset propio).
pub const COLUMNS: [(&str, Option<ThemePreset>); 7] = [
    ("Batten", Some(ThemePreset::Batten)), ("Martty", Some(ThemePreset::Martty)),
    ("Mindfork", Some(ThemePreset::Mindfork)), ("Pawan", Some(ThemePreset::Pawan)),
    ("Crush", Some(ThemePreset::Crush)), ("MastraCode", Some(ThemePreset::MastraCode)),
    ("CodeLoom", None),
];

pub struct Matrix {
    pub rows: Vec<MatrixRow>,       // en el orden de primera aparición en el catálogo
    pub left_out: Vec<(&'static str, &'static str)>, // (componente, variante) sin proveedor conocido
    pub library: Vec<&'static str>, // componentes solo de dotmax, sin repetir
}
pub struct MatrixRow { pub component: &'static str, pub cells: [Cell; 7] }

pub fn build(items: &[CatalogItem]) -> Matrix;
/// Índice del primer demo del catálogo para esa celda: el del proveedor de la columna si es
/// `Own`, el de Batten si es `Generic`, `None` si es `Missing`.
pub fn target(items: &[CatalogItem], component: &str, column: usize) -> Option<usize>;
```

Reglas de `build`: el proveedor de cada demo sale de `strict_provider(variant)`; los `None` van a
`left_out` y no cuentan en ninguna celda. `Own(n)` si hay `n ≥ 1` demos de ese componente con el
proveedor de la columna. `Generic` si no hay propio **y hay al menos un demo del proveedor Batten**
de ese componente. `Missing` en el resto, **sin promocionar** demos de otros proveedores. Los
componentes cuyos demos son todos de dotmax van a `library` y no a `rows`. La columna Batten de un
componente con demo Batten es `Own`, nunca `Generic`.

**Tests** (con `CatalogItem` reales construidos a mano con demos vacíos, y uno final contra
`demos::catalog()`): `own_when_the_theme_has_a_demo`; `generic_needs_a_batten_demo` (un
componente solo de Crush es `Missing` en Pawan); `generic_when_only_batten_has_it`;
`an_unknown_provider_is_left_out_not_counted_as_batten`; `dotmax_components_go_to_the_library_list`;
`target_prefers_the_column_provider_then_batten`; y contra el catálogo real
`mastracode_has_no_own_cells`, `crush_composer_is_own`, `pawan_composer_is_generic` y
`every_target_points_to_an_existing_demo`.

**Commit:** `feat(batten): derivación de la matriz de componentes por tema`.

---

### Tarea 2: modo matriz en el estado y las teclas

**Ficheros:** `crates/batten/examples/showcase/app.rs`.

- `Focus::Matrix` nuevo. `Showcase` gana `matrix: Matrix` (calculada una vez en `new` a partir del
  catálogo) y el cursor `matrix_cursor: (usize, usize)` (fila, columna), más
  `matrix_return: bool` (si el demo actual se abrió desde la matriz).
- Desde `Focus::Index`, la tecla `m` abre la matriz (`focus = Matrix`).
- `Focus::Matrix`: flechas/`hjkl` mueven el cursor con tope (sin dar la vuelta); `Esc` o `m`
  vuelven al índice; `Enter` sobre la celda: si `target` da `Some(i)`, hace `select(i, true)`,
  pone `matrix_return = true` y, si la columna tiene preset, `theme_preset` = ese preset (la
  columna CodeLoom **no** cambia el tema); si da `None`, no hace nada.
- En `Focus::Demo`, `Esc` con `matrix_return == true` vuelve a `Focus::Matrix` (y limpia la
  marca); sin ella, al índice como hoy. `F7` sigue ciclando el tema en todos los modos.
- Funciones de lectura para el dibujo: `matrix()`, `matrix_cursor()`, `matrix_cell()` (la celda
  bajo el cursor).

**Tests:** `m_opens_the_matrix_and_esc_closes_it`; `cursor_stops_at_the_edges`;
`enter_on_own_opens_the_demo_with_that_theme`; `enter_on_generic_opens_the_batten_demo_with_the_column_theme`;
`enter_on_missing_does_nothing`; `enter_on_codeloom_keeps_the_active_theme`;
`esc_from_a_demo_opened_from_the_matrix_returns_to_it`; `esc_from_a_normal_demo_still_returns_to_the_index`.
Los tests de navegación existentes siguen pasando sin tocarlos.

**Commit:** `feat(batten): modo matriz del showcase`.

---

### Tarea 3: dibujar la matriz

**Ficheros:** `crates/batten/examples/showcase/matrix.rs` (función de dibujo),
`crates/batten/examples/showcase/main.rs` (llamarla desde `draw` cuando `focus == Matrix`, y
añadir la ayuda de `Focus::Matrix` en `draw_header`).

`pub fn draw_matrix(f: &mut Frame, area: Rect, show: &Showcase, theme: &Theme)`:
- Cabecera con los siete nombres de columna alineados; la columna del cursor resaltada.
- Una fila por componente: nombre a la izquierda (recortado con `…` si no cabe) y el símbolo de
  cada celda alineado bajo su columna: `●` con el estilo de éxito/acento del tema, `◐` con el de
  aviso, `·` con el atenuado (mira `theme.styles` y `status_bar.rs`; no inventes colores). La
  celda del cursor, invertida.
- Desplazamiento vertical que mantiene visible la fila del cursor.
- Pie de dos líneas: leyenda `● propio  ◐ genérico  · no existe` y, para la celda bajo el
  cursor, el recuento («Crush · Composer: 1 demo propio»; «sin demo propio, se usa el de
  Batten»; «no existe»), y la nota de dotmax si `library` no está vacío («dotmax es una librería,
  no un tema: …») y de `left_out` («N demos sin proveedor reconocido»).
- Ayuda de cabecera: `←/→/↑/↓: celda · Enter: abrir · Esc: índice · F7: paleta`.

**Tests:** con `TestBackend` 100×40 sobre el catálogo real: `matrix_shows_the_three_symbols_and_all_columns`;
`matrix_marks_the_cursor_column`; `matrix_scrolls_to_keep_the_cursor_visible` (cursor en la última
fila); `matrix_fits_in_a_narrow_terminal_without_panic` (60×20); `footer_names_the_cell_under_the_cursor`.

**Commit:** `feat(batten): dibujo de la matriz de componentes por tema`.

---

### Tarea 4: vista de un componente entre temas

**Ficheros:** `crates/batten/examples/showcase/app.rs`, `crates/batten/examples/showcase/matrix.rs`
(funciones puras de la nota), `crates/batten/examples/showcase/main.rs` (pie).
Requiere las Tareas 2 y 3. Spec: «Vista de un componente entre temas».

- `Showcase` gana `view_column: Option<usize>` (columna actual). `Enter` en la matriz y `F8` la
  fijan; al elegir un demo desde el índice (`select`, `next`, `prev`) se pone en `None` y se
  deduce del proveedor del demo (`strict_provider` → índice en `COLUMNS`; `None` si no es una
  columna).
- `F8` / `Shift+F8`, en `route` como global junto a `F7`, **solo con `Focus::Demo`**: busca la
  siguiente (anterior) columna de la fila del componente actual cuya celda no sea `Missing`, con
  vuelta al final, y aplica lo mismo que `Enter` en la matriz (`select(target)`, preset de la
  columna salvo CodeLoom, `view_column`). Sin fila, o sin otra columna visible, no hace nada. No
  cambia `matrix_return`.
- Funciones puras en `matrix.rs`: `pub fn status_note(component: &str, column: usize, cell:
  Cell) -> String` con los textos exactos de la spec (los símbolos `●`/`◐`; «parcialmente
  implementado» para `Generic`; el texto de CodeLoom para la columna 6 sea cual sea la celda) y
  `pub fn strip(row: &MatrixRow, current: Option<usize>) -> Vec<(String, Cell, bool)>`
  (nombre de columna, celda, si es la actual).
- Pie: si el componente actual tiene fila, el pie ocupa dos líneas (`Constraint::Length(2)` en
  `draw`, solo en ese caso) con la tira y la nota; el nombre del tema activo (`F7/Shift+F7 tema:
  …`) se conserva al final de la nota o de la línea actual si no cabe. Sin fila, una línea como
  hoy. Cabecera de ayuda de `Focus::Demo`: «F8: tema del componente · F7: paleta · Tab/Esc:
  volver».

**Tests:** en `matrix.rs`, `status_note_names_partial_implementation_for_generic`,
`status_note_for_own_mentions_variants_only_when_several`, `codeloom_note_says_no_palette`,
`strip_marks_the_current_column`; en `app.rs`, `f8_switches_theme_and_demo_together`,
`f8_skips_missing_columns`, `f8_on_a_generic_cell_opens_the_batten_demo_with_the_column_theme`,
`f8_does_nothing_for_a_component_without_a_row`, `f8_wraps_around_and_shift_f8_goes_back`,
`selecting_from_the_index_derives_the_column_from_the_demo`; en `main.rs`, con `TestBackend`
100×30, `footer_shows_the_strip_and_the_note_for_a_component_with_a_row` y
`footer_stays_one_line_without_a_row`. Los tests existentes siguen pasando.

**Commit:** `feat(batten): cambiar de tema dentro de un componente y nota de estado`.

---

### Tarea 4b: paleta propia de CodeLoom

**Ficheros:** `crates/batten/src/widgets/theme.rs`; `crates/batten/examples/showcase/matrix.rs`
(`COLUMNS`) y `app.rs` (lo que dependa de que CodeLoom no tenga preset);
`crates/codeloom-tui/src/prefs.rs`, `run.rs`, `render.rs`, `app.rs` solo donde un `match` sobre
`ThemePreset` o una lista de nombres obligue a tratar la variante nueva. Requiere la Tarea 4.
Spec: «Paleta de CodeLoom».

- `ThemePreset::CodeLoom` añadido **al final** de `ThemePreset::ALL` (que pasa a `[Self; 7]`), con
  `name()` = `"CodeLoom"` y su tupla de colores en `Theme::preset` con los valores exactos de la
  tabla de la spec, en el mismo orden de roles que las demás (`bg, fg, muted, accent, user,
  assistant, warning, error, success, border`). `Look` clásico (sin tocar `theme.look`).
- `matrix::COLUMNS[6]` pasa a `("CodeLoom", Some(ThemePreset::CodeLoom))`. Desaparece el caso
  especial «sin preset»: `open_matrix_cell`, `cycle_component_theme` y `status_note` dejan de
  tratar la columna CodeLoom aparte (se elimina el sufijo «sin paleta propia»). Si tras eso nada
  usa ya el `Option` de `COLUMNS`, déjalo como está para no ampliar el cambio.
- Las preferencias guardadas (`prefs.rs`) con nombres de tema existentes siguen cargando; un
  nombre desconocido sigue sin romper el arranque, como hoy. El nuevo se guarda y recarga con su
  nombre.

**Tests:** en `theme.rs`, `codeloom_preset_uses_the_logo_colors` (acento y borde exactos),
`every_preset_in_all_has_a_name_and_a_theme` y `codeloom_is_the_last_preset`; en la matriz, los
tests de `enter_on_codeloom_keeps_the_active_theme` se **reescriben** como
`enter_on_codeloom_applies_its_palette` (con `// ADAPTADO: CodeLoom ya tiene preset`), y los de
`status_note` de CodeLoom se adaptan igual; en `prefs.rs`, un test de ida y vuelta del nombre
«CodeLoom». Los tests existentes de los demás presets siguen pasando sin tocarlos.

**Commit:** `feat(batten): paleta propia de CodeLoom`.

---

### Tarea 5: documentación

«Estado» de la spec (commits, lo que no se ha visto en terminal real, qué celdas salen de la
matriz hoy con cifras reales, las 6 entradas dejadas fuera) y una línea sobre `m` en la ayuda del
showcase si existe un README del showcase. Prosa en español formal. Nada se da por visto en
terminal real.

**Commit:** `docs: estado real de la matriz de componentes por tema`.
