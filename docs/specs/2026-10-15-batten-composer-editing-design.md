# batten — Composer editable (cursor, multilínea e historial)

> Estado: **diseño aprobado** (secciones 1-7 revisadas en sesión).
>
> Fecha: 2026-10-15
>
> Relacionado: `docs/specs/2026-10-14-batten-design.md` (§3 `Composer`, §4
> `composer`), `docs/specs/2026-10-15-batten-sidebar-logo-design.md` (la TUI ya
> consume batten). Referencia de UX: el input de Crush.

## 0. Propósito

Convertir el input del prompt en un **editor de texto de verdad**: cursor,
movimiento y edición en medio, varias líneas e **historial** de lo enviado.
Hoy la TUI usa un `String` plano (sin cursor) y el `state::Composer` de batten
es solo "append".

## 1. Decisiones cerradas

| # | Decisión |
|---|---|
| 1 | Cursor con `←/→`, `Home/End`, `↑/↓` entre líneas; borrar con `Backspace`/`Delete`. |
| 2 | Multilínea: **`Ctrl+J`** inserta salto; **`Enter` envía siempre** (predecible; `Shift+Enter` no se distingue en todos los terminales). |
| 3 | Historial con `↑/↓`, guardando el borrador en curso. |
| 4 | El cursor es el **cursor real del terminal** (`frame.set_cursor_position`), no un bloque pintado. |
| 5 | El historial vive en batten (`state::History`), genérico y testeable; la app aporta la instancia. |
| 6 | La TUI migra a `state::Composer` + `widgets::composer`. |
| 7 | v0 sin wrap horizontal: las líneas lógicas se **recortan** al ancho. |

## 2. Arquitectura y archivos

```
crates/batten/src/state/composer.rs   # ampliar: cursor + edicion + lineas
crates/batten/src/state/history.rs    # NUEVO
crates/batten/src/state/mod.rs        # + pub mod history; + re-export History
crates/batten/src/widgets/composer.rs # multilinea + cursor real
crates/codeloom-tui/src/app.rs        # composer + history; submit
crates/codeloom-tui/src/run.rs        # teclas de edicion/historial
crates/codeloom-tui/src/render.rs     # usa el composer de batten
```

`state` sigue sin `ratatui`; `batten` sin crossterm.

## 3. `state::Composer` (ampliar)

El cursor es un **índice de carácter** (`0..=nº de chars`), no de bytes.

```rust
impl Composer {
    pub fn new() -> Self;
    pub fn insert_char(&mut self, c: char);   // en el cursor
    pub fn newline(&mut self);                // == insert_char('\n')
    pub fn insert_str(&mut self, s: &str);
    pub fn backspace(&mut self);              // borra el char anterior
    pub fn delete(&mut self);                 // borra el char del cursor
    pub fn move_left(&mut self);
    pub fn move_right(&mut self);
    pub fn move_home(&mut self);              // inicio de la linea
    pub fn move_end(&mut self);               // fin de la linea
    pub fn move_up(&mut self);                // linea arriba (columna recordada)
    pub fn move_down(&mut self);
    pub fn cursor(&self) -> usize;            // indice de caracter
    pub fn lines(&self) -> usize;             // nº de lineas logicas
    pub fn cursor_row_col(&self) -> (usize, usize); // (fila, columna en display)
    pub fn set_text(&mut self, s: &str);      // reemplaza; cursor al final
    pub fn text(&self) -> &str;
    pub fn is_empty(&self) -> bool;
    pub fn push_char(&mut self, c: char);     // == insert_char (compat)
    pub fn submit(&mut self) -> Option<String>; // como hoy: vacio/espacios -> None
}
```

- `move_up`/`move_down` recuerdan la **columna de carácter** preferida para
  columnas cortas; `cursor_row_col` usa anchura de display (`unicode-width`).
- `submit` limpia buffer y cursor.

## 4. `state::History` (nuevo)

```rust
impl History {
    pub fn new() -> Self;                 // tope por defecto (p. ej. 100)
    pub fn push(&mut self, sent: impl Into<String>); // añade y resetea navegacion
    pub fn prev(&mut self, current: &str) -> Option<&str>; // ↑ (guarda borrador)
    pub fn next(&mut self) -> Option<&str>;                // ↓ (vuelve al borrador)
    pub fn reset(&mut self);
}
```

- `prev` con navegación vacía guarda `current` como borrador y devuelve la
  última entrada; sucesivos `prev` suben. `next` baja y, al pasar del final,
  devuelve el borrador y termina la navegación.
- Vacío -> `None`. Tope: al exceder, se descarta la entrada más antigua.

## 5. `widgets::composer` (ampliar)

`render(frame, area, composer, theme)`:

- Borde + título `prompt` (como hoy).
- Renderiza el texto multilínea con `Paragraph` **sin wrap** (recorta al ancho).
- Coloca el **cursor real** en `(area.x + 1 + col, area.y + 1 + row)` a partir de
  `cursor_row_col()`; si cae fuera del área interior, no se coloca.
- Área 0×0 no entra en pánico.

## 6. codeloom-tui

- `App` gana `composer: batten::state::Composer` (sustituye `input: String`) y
  `history: batten::state::History`.
- Teclas (`handle_key`):
  - `Enter` -> `submit` -> envía (y `history.push`) y `history.reset`.
  - `Ctrl+J` -> `newline`.
  - `Backspace`/`Delete` -> borrar; `←/→` -> mover; `Home/End` -> inicio/fin.
  - `↑/↓` -> **si `composer.lines() > 1`** mueven el cursor entre líneas; si no,
    navegan el historial (`prev`/`next` + `set_text`).
  - `Tab`/`Esc`/`Ctrl+C` mantienen su comportamiento actual.
- `render.rs`: `draw_composer` usa `batten::widgets::composer::render`.
- Tests existentes que usan `App::input`/`push_char`/`submit` se actualizan al
  nuevo modelo sin cambiar su intención.

## 7. Tests

- **Composer**: insertar en medio (`insert_char` con cursor movido), `backspace`
  y `delete`, `move_left/right` con topes, `home/end`, `up/down` entre líneas con
  columna recordada, `cursor_row_col` (incluye líneas y ancho de display),
  `submit` vacío/solo-espacios -> `None` y limpieza.
- **History**: `push` + `prev`/`next` con borrador, `reset`, tope descarta la más
  antigua, vacío -> `None`.
- **Widget**: renderiza varias líneas; el cursor queda en la posición esperada
  (comprobable con `TestBackend`); 0×0 sin pánico.
- **TUI**: `Enter` envía y guarda historial; `Ctrl+J` añade línea sin enviar;
  con una línea `↑` recuerda lo enviado y `↓` vuelve al borrador; con varias
  líneas `↑/↓` mueven el cursor.

## 8. Criterios de aceptación

- [ ] El prompt permite escribir, moverse con flechas, borrar en medio y varias líneas (`Ctrl+J`); `Enter` envía.
- [ ] `↑/↓` recuerdan lo enviado cuando el prompt es de una línea.
- [ ] El cursor se ve en la posición correcta.
- [ ] `cargo test -p batten --examples`, `cargo test -p batten -p codeloom-core -p codeloom-tui`, `fmt` y `clippy` pasan.

## 9. Fuera de alcance (v0)

- Pegado (bracketed paste), ratón, selección y corte/copia dentro del prompt.
- Movimiento por palabras (`Ctrl+←/→`) y `Ctrl+U`/`Ctrl+K`.
- Wrap horizontal del texto (se recorta) y desplazamiento horizontal del cursor.
- Persistir el historial en disco o entre sesiones.

## 10. Preguntas abiertas

- ¿Tope del historial (100?) y si debe ser configurable.
- ¿`↑/↓` con varias líneas debería además recordar la columna **en display** en
  vez de en caracteres? v0 usa columna de carácter.
