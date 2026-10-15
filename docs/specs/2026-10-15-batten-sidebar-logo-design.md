# `batten` — sidebar, logo de bloques y su integración en CodeLoom

> Estado: **diseño aprobado** (secciones A-H revisadas en sesión).
>
> Fecha: 2026-10-15
>
> Relacionado: `docs/specs/2026-10-14-batten-design.md` (§2, §4, §5, §6 v1),
> `docs/plans/2026-10-14-batten-v0.md`. Extiende batten v0 (chat core) con el
> **sidebar** y el **logo paramétrico** que el roadmap dejó para v1, y cablea el
> sidebar en `codeloom-tui`.

## 0. Propósito

batten v0 entregó el chat core (theme, transcript, spinner, tool item,
composer). Falta el **sidebar** —logo + info de sesión— que hoy vive cableado y
con marca dentro de `codeloom-tui`. Este trabajo:

1. Añade a batten un **logo paramétrico** basado en una **fuente de bloques**
   propia, pensada para animarse por celda en el futuro.
2. Añade a batten un **sidebar genérico** (logo + secciones), sin marca ni
   idioma incrustados.
3. Hace que `codeloom-tui` consuma el sidebar de batten, pasándole su marca y
   sus secciones.

Regla dura (batten §1, §8): **sin marca CodeLoom dentro del crate**; el logo es
paramétrico y el tema por defecto es neutro.

## 1. Decisiones cerradas

| # | Decisión |
|---|---|
| 1 | La fuente de bloques **vive en batten** (no la dibuja cada app). |
| 2 | Cobertura de glifos: `A-Z`, `0-9`, `espacio`, `. - _ / :`. Una altura (5 filas). La app normaliza a mayúsculas. |
| 3 | El logo está **preparado para animar por celda**, pero en v0 solo se pinta un degradado por columna estático (YAGNI: sin animación). |
| 4 | El sidebar es **genérico**: `logo + secciones`. batten no hardcodea etiquetas, valores ni idioma. |
| 5 | La fuente y el modelo del sidebar son **puros** (sin `ratatui`); el render es un widget. |
| 6 | `codeloom-tui` usa el sidebar de batten y **borra** su sidebar local. |
| 7 | En este trabajo **no** se migran transcript/composer/tool al kit (plan aparte). |

## 2. Arquitectura y archivos

```
crates/batten/
  src/
    state/
      sidebar.rs        # NUEVO: Logo, Section, Sidebar (puro)
      mod.rs            # + pub mod sidebar; + re-exports
    widgets/
      logo.rs           # NUEVO: fuente de bloques + Wordmark + render
      sidebar.rs        # NUEVO: render del sidebar (logo + secciones)
      theme.rs          # + tokens AccentFrom/AccentTo/LogoField
      mod.rs            # + pub mod logo; + pub mod sidebar
crates/codeloom-tui/
  Cargo.toml            # + batten (path)
  src/
    app.rs              # + theme: batten::Theme; + App::sidebar()
    render.rs           # draw_sidebar -> batten::widgets::sidebar::render
    widgets/
      mod.rs            # - pub mod sidebar
      sidebar.rs        # ELIMINADO
```

**Frontera dura** (se mantiene): `state` no importa `ratatui`; `batten` no
importa `crossterm`. La fuente de bloques es código puro dentro de `widgets`
(sin `ratatui`), por lo que se testea sin backend. Se respeta la ubicación de
`widgets/logo.rs` que ya fija `2026-10-14-batten-design.md` §2.

## 3. Fuente de bloques (`widgets::logo`)

```rust
/// Filas de la fuente de bloques.
pub const FONT_ROWS: usize = 5;

/// Arte de bloque para un carácter. Devuelve `FONT_ROWS` filas; un carácter
/// no soportado devuelve el glifo de espacio.
fn glyph(c: char) -> [&'static str; FONT_ROWS];

/// Wordmark construido a partir de un nombre.
pub struct Wordmark { /* filas de anchura uniforme */ }

impl Wordmark {
    pub fn new(name: &str) -> Self;   // mayúsculas; no soportado -> espacio
    pub fn rows(&self) -> &[String];  // FONT_ROWS filas
    pub fn width(&self) -> usize;     // anchura en columnas
    pub fn height(&self) -> usize;    // == FONT_ROWS
}
```

- **Tabla de glifos**: `A-Z`, `0-9`, `espacio`, `.`, `-`, `_`, `/`, `:`. Fuente
  de **3 columnas × 5 filas**; cada glifo ocupa exactamente 3 columnas (los
  glifos estrechos llevan relleno incorporado).
- `Wordmark::new` pasa `name` a mayúsculas y **pega los glifos sin columna de
  separación** (denso: `"CODELOOM"` = 24 columnas, cabe en la columna de 30 del
  sidebar). Carácter fuera de la tabla -> glifo de espacio.
- La rejilla por celda (`rows()[r]`, columna `c`) es **la superficie que el
  futuro animador tocará**; no cambia la API al animar.

### 3.1 Render del logo

```rust
/// Wordmark de bloques: `FONT_ROWS` líneas con el degradado por columna.
pub fn render(wordmark: &Wordmark, theme: &Theme) -> Vec<Line<'static>>;

/// Fallback de una línea: nombre + `╱` (estilo `LogoField`) hasta `width`.
pub fn compact(name: &str, width: usize, theme: &Theme) -> Vec<Line<'static>>;
```

- `render` produce **solo el wordmark** (las `FONT_ROWS` filas); no añade las
  líneas de campo `╱`. `LogoField` se usa solo en `compact`.
- Estilo **por celda**: degradado por columna entre `AccentFrom` (t=0) y
  `AccentTo` (t=1) a lo largo de la anchura; las celdas que son espacio no
  llevan estilo.
- El degradado se calcula en una única función
  `gradient_style(col, width, theme) -> Style`. **Animación futura**: reemplazar
  esa función por una que también reciba el frame; el resto (rejilla, widget) no
  cambia. En v0 **no** se añade parámetro `frame` ni estado de animación.
- `compact` con `width == 0` no produce pánico (devuelve solo el nombre).

## 4. Sidebar

### 4.1 Estado puro (`state::sidebar`)

```rust
pub struct Logo { pub name: String }
pub struct Section { pub title: String, pub lines: Vec<String> }
pub struct Sidebar { pub logo: Option<Logo>, pub sections: Vec<Section> }

impl Sidebar {
    pub fn new() -> Self;
    pub fn with_logo(name: impl Into<String>) -> Self;
    pub fn push_section(&mut self, title: impl Into<String>, lines: Vec<String>);
}
```

Nada de etiquetas ni valores por defecto: la app decide títulos y contenido.

### 4.2 Widget (`widgets::sidebar`)

```rust
pub fn render(frame: &mut Frame, area: Rect, sidebar: &Sidebar, theme: &Theme);
```

- `Block` con borde izquierdo; se calcula el `inner`.
- `const LOGO_MIN_HEIGHT: usize = FONT_ROWS + 2;` (wordmark + una línea en
  blanco). Si hay `logo` y `inner.height >= LOGO_MIN_HEIGHT` **y el wordmark
  cabe** (`wordmark.width() <= inner.width`), se pinta `logo::render`; si no,
  `logo::compact(name, inner.width, theme)`.
- A continuación, por cada `Section`: el título (estilo `Heading`) y sus líneas
  (estilo `Default`), seguidas de una línea en blanco.
- Las líneas que exceden la anchura se truncan por **anchura de display**
  (`unicode-width`), con `…` final, como el sidebar actual.
- Terminal de anchura/altura 0 no entra en pánico.

## 5. Tokens de tema nuevos

En `widgets::theme`:

- `StyleToken::{AccentFrom, AccentTo, LogoField}`.
- Campos correspondientes en `Styles`, con defaults: `AccentFrom = #00d4ff`,
  `AccentTo = #7c5cff`, `LogoField = indexed:238`.
- Parseables desde `theme.toml` con los mismos nombres y sintaxis que el resto
  (`"cyan"`, `"indexed:244"`, `"#00d4ff"`).

`Theme::test()` y `Theme::default_dark()` los resuelven; los tests de tema
existentes siguen pasando.

## 6. Integración en `codeloom-tui`

- `Cargo.toml`: `batten = { path = "../batten" }`.
- `app.rs`:
  - `App` gana `theme: batten::widgets::Theme` (default `Theme::default_dark()`),
    para que la firma de `render(f, &app)` no cambie.
  - `App::sidebar(&self) -> batten::state::Sidebar`: logo `"CodeLoom"` y
    secciones con las etiquetas actuales: `estado` (status), `cwd` (truncado por
    el widget), `modelo` (model), `agente` (steps `—`, hoy), `subagentes / jobs`
    (`(m2)`, hoy).
- `render.rs`:
  - Se elimina `draw_sidebar`; en la rama ancha (>= `SIDEBAR_MIN_WIDTH`) se llama
    a `batten::widgets::sidebar::render(f, cols[1], &app.sidebar(), &app.theme)`.
  - El header y el transcript/composer siguen siendo locales.
- Se elimina `crates/codeloom-tui/src/widgets/sidebar.rs` y su `pub mod sidebar`.

**Impacto visual**: el wordmark pasa de arte a mano para "CodeLoom" a la fuente
genérica ("CODELOOM"), que se verá **distinto**. Es el coste aceptado de tener
fuente paramétrica.

## 7. Tests

- **`widgets::logo`** (puro): cada carácter soportado devuelve `FONT_ROWS` filas;
  un nombre conocido produce la rejilla esperada (anchura y contenido); un
  carácter no soportado cae al glifo de espacio; `Wordmark::new("")` no paniquea.
  Render: sin backend para la rejilla; con `TestBackend`/`Theme::test()` para el
  widget (nombre visible, degradado aplicado, 0×0 sin pánico).
- **`state::sidebar`**: `with_logo`, `push_section`, orden de secciones.
- **`widgets::sidebar`** (`TestBackend`, `Theme::test()`): muestra el nombre y
  los títulos/líneas; altura chica usa el compacto; 0×0 sin pánico; truncado por
  anchura de display.
- **`codeloom-tui`**: `App::sidebar()` arma logo + secciones; el test de render
  que hoy vive en `widgets/sidebar.rs` se traslada a `render.rs` y sigue viendo
  `"CodeLoom"`, `"mistral-nemo"` y `"subagentes"`.
- **Checks**: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`
  pasan en todo el workspace.

## 8. Criterios de aceptación

- [ ] La fuente de bloques cubre `A-Z 0-9 . - _ / :` y espacio; sin pánico con
      entradas raras.
- [ ] `batten` sigue sin importar `crossterm`; `state` sigue sin importar
      `ratatui`.
- [ ] El sidebar de batten es genérico (sin marca ni idioma).
- [ ] `codeloom-tui` usa el sidebar de batten y ya no tiene `widgets/sidebar.rs`.
- [ ] Los cuatro checks del repo pasan.

## 9. Fuera de alcance

- Migrar transcript/composer/tool item al kit (plan aparte).
- Animación por frame del logo (solo se deja el *seam*).
- Minúsculas en la fuente; más signos de puntuación.
- Sidebar colapsable, snapshots `insta`, `NO_COLOR`.

## 10. Preguntas abiertas

- ¿El degradado del logo usa los tokens nuevos o reutiliza `accent`/`tool_running`
  como el spinner? Por ahora, tokens nuevos (`AccentFrom`/`AccentTo`).
- ¿El sidebar debería poder ocultarse por teclado, o solo por anchura?
