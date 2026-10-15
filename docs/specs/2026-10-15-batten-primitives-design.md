# batten — Primitivas visuales (familia 1)

> Estado: **diseño aprobado**. Siguiente paso: plan de implementación.
>
> Fecha: 2026-10-15
>
> Relacionado: `docs/specs/2026-10-14-batten-design.md` (§4 widgets, §5 theming).
> Referencias de UX: Crush (`internal/ui/common/elements.go`, `dialog/common.go`,
> `model/status.go`), Charm bubbles (`progress`, `help`), Ink UI (`Badge`,
> `StatusMessage`), Textual (`Rule`, `Toast`).

## 0. Propósito

Una paleta de **primitivas** sobre la que se montan los componentes ricos
(diálogos, items del chat, barras de estado). Se hacen primero (enfoque A) para
que todo lo posterior comparta aspecto; después se ensamblan pantallas concretas
(enfoque B: diálogo de permisos, etc.).

## 1. Tipo compartido: `state::Tone`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tone { #[default] Neutral, Info, Success, Warning, Error }
```

Semántico, sin color (regla de la capa `state`). El widget lo resuelve así:

| Tone | Estilo del tema | Icono (`status`) | Etiqueta (`toast`) |
|---|---|---|---|
| Neutral | `muted` | `•` | `NOTE` |
| Info | `accent` | `●` | `HEY!` |
| Success | `success` | `✓` | `OKAY!` |
| Warning | `warning` | `!` | `WARNING` |
| Error | `error` | `×` | `ERROR` |

La resolución vive en `widgets::tone` (`pub(crate) fn tone_style`, `tone_icon`,
`tone_label`).

## 2. Piezas

Las piezas en línea devuelven `Span<'static>` / `Line<'static>` (como `spinner` y
`logo`) para poder componerse dentro de otras. Todos los anchos son **columnas de
display** (`unicode-width`); nada se sale nunca de `width`.

### 2.1 `chip`

`chip(text: &str, tone: Tone, theme: &Theme) -> Span<'static>`

- Contenido: `" " + text + " "`.
- Estilo: fondo = color de primer plano del estilo del tono (`gradient::color_of`),
  texto `Color::Black`, `BOLD`.

### 2.2 `rule`

`rule(title: Option<&str>, width: u16, theme: &Theme) -> Line<'static>`

- Sin título: `─` × `width` en `muted`.
- Con título: `title` (`heading`) + `" "` + `─` hasta completar `width` (`muted`).
  Si el título no cabe, se trunca con `…` a `width` y no hay regla.
- `width == 0` → línea vacía.

### 2.3 `key_help`

`key_help(items: &[(&str, &str)], width: u16, theme: &Theme) -> Line<'static>`

- Cada item: `key` (`heading`) + `" "` + `desc` (`muted`); separador `" · "` (`dim`).
- Si no cabe todo, se quitan items **del final** hasta que quepa lo que queda
  más `" …"` (`dim`). Si ni el primero cabe, solo `"…"`.
- Sin items → línea vacía.

### 2.4 `status`

`status(tone: Tone, title: &str, desc: &str, width: u16, theme: &Theme) -> Line<'static>`

- `icono` (estilo del tono) + `" "` + `title` (`default`) + `"  "` + `desc` (`muted`).
- Recorte: primero se trunca `desc` (con `…`); si no queda sitio para ella, se
  omite junto con su separador; si tampoco cabe `title`, se trunca `title`.
- `desc` vacía → sin separador.

### 2.5 `progress`

`progress(ratio: f32, width: u16, theme: &Theme) -> Line<'static>`

- `ratio` se acota a `[0, 1]` (NaN → 0).
- Sufijo de porcentaje alineado a 4 columnas más espacio: `" 42%"` → siempre
  `format!(" {:>3}%", pct)` (5 columnas). Barra = `width - 5` columnas.
- Relleno: `round(ratio * barra)` celdas `━`, cada una con degradado por columna
  `accent_from → accent_to` (`gradient::lerp_color`). Resto: `─` en `muted`.
- `width < 6` → solo el porcentaje sin espacio inicial, truncado a `width`.

### 2.6 `panel`

```rust
pub struct Panel<'a> { pub title: &'a str, pub info: &'a str, pub tone: Tone }
pub fn render(frame: &mut Frame, area: Rect, panel: &Panel, theme: &Theme) -> Rect
```

```
╭ Permisos ╱╱╱╱╱╱╱╱╱╱╱╱╱╱╱╱╱ bash ╮
│  contenido (Rect devuelto)     │
╰────────────────────────────────╯
```

- Borde redondeado (`ratatui` `Block` + `BorderType::Rounded`); estilo del borde:
  token nuevo `border` si `Tone::Neutral`, si no el estilo del tono.
- Fila superior entre esquinas: `" " + title + " "` (`heading`), relleno `╱`
  con degradado `accent_from → accent_to`, y `" " + info + " "` (`muted`) a la
  derecha si `info` no está vacío. Si no cabe, se descarta primero `info`, luego
  el relleno, y el título se trunca con `…`.
- Devuelve el área interior (`area` menos el borde). Si `area` tiene menos de
  2×2, no pinta nada y devuelve `Rect::default()`.

### 2.7 `toast` (estado + vista)

```rust
// state::toast
pub struct Toast { pub tone: Tone, pub text: String, pub ttl: u32 }
pub struct Toasts { /* VecDeque<Toast> */ }
impl Toasts {
    pub fn push(&mut self, tone: Tone, text: impl Into<String>, ttl_frames: u32);
    pub fn current(&self) -> Option<&Toast>;
    pub fn tick(&mut self) -> bool; // resta 1 al ttl del actual; al llegar a 0 lo saca. true si cambió lo visible.
}
// widgets::toast
pub fn render(toasts: &Toasts, width: u16, theme: &Theme) -> Option<Line<'static>>
```

- Cola FIFO: se muestra solo el primero. Caducidad **por frames contados**
  (determinista, como el spinner): la app llama a `tick()` en su reloj.
- `push` con `ttl_frames == 0` se ignora.
- Vista: `chip(tone_label(tone), tone)` + `" "` + `text` (`default`), truncado a
  `width`. Sin toast → `None`.

## 3. Tema

Un único token nuevo: **`border`** (`Styles.border`, `StyleToken::Border`,
nombre TOML `border`), por defecto `Color::Indexed(240)`. Todo lo demás reusa
tokens existentes.

## 4. Showcase

Una demo por pieza en `examples/showcase/demos/` (`chip`, `rule`, `key_help`,
`status`, `progress` con ratio animado por frames, `panel` con los cinco tonos,
`toast` que se dispara con una tecla), registradas en el índice como las demás.

## 5. Tests

Cada pieza lleva tests unitarios con `Theme::test()` sobre el texto resultante
(concatenando los spans) y los estilos clave; `panel` con `TestBackend`. Casos
mínimos: ancho 0, contenido que no cabe, unicode ancho (CJK), y los cinco tonos
donde aplique. `Toasts`: FIFO, caducidad exacta, `ttl == 0` ignorado.

## 6. No-objetivos (YAGNI)

- `alert` (es `panel` + `status`).
- Progreso indeterminado (para eso está el spinner).
- Foco, teclado o ratón en `panel`; padding configurable.
- Varios toasts visibles a la vez.

## 7. Después

Enfoque B: ensamblar con estas piezas el **diálogo de permisos** (panel + diff +
botones + `key_help`) y la **status bar** (toast + `key_help` + chips de modo).
