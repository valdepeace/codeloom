# batten sidebar + logo de bloques — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Añadir a batten un logo paramétrico (fuente de bloques) y un sidebar genérico, y hacer que `codeloom-tui` consuma el sidebar de batten en vez de su versión local.

**Architecture:** `state` gana el modelo puro del sidebar (`Logo`, `Section`, `Sidebar`); `widgets` gana `logo` (fuente de bloques 3×5 + `Wordmark` + render con degradado), `sidebar` (render) y un `gradient` interno compartido con el spinner. `codeloom-tui` añade la dependencia `batten` y cablea el sidebar.

**Tech Stack:** Rust (edición 2021), `ratatui` 0.30, `unicode-width`, `serde`/`toml`.

**Spec:** `docs/specs/2026-10-15-batten-sidebar-logo-design.md`

## Global Constraints

- `batten` es miembro del workspace (`crates/batten`); `version`/`edition`/`license` heredados.
- `state` NO importa `ratatui` ni `crossterm`. `batten` NO importa `crossterm`.
- Sin marca CodeLoom ni idioma dentro del crate.
- Nada de agente, proveedores, permisos ni red.
- Comentarios en español (igual que el resto de batten).
- `cargo fmt --check`, `cargo clippy -- -D warnings` y `cargo test` pasan en cada commit.
- Tests headless; widgets con `TestBackend` y `Theme::test()`.
- El wordmark se pinta **solo si cabe** en la anchura disponible; si no, compacto.

## Review Focus

- **Anchura/altura 0 o 1**: logo, compacto y sidebar no deben entrar en pánico. Test en Task 4 y Task 6.
- **Wordmark más ancho que la columna**: no debe envolverse; debe caer al compacto. Test en Task 6.
- **Nombre con caracteres fuera de la tabla** (minúsculas con acento, emoji, símbolos raros): caen al glifo de espacio sin pánico. Test en Task 2.
- **Líneas muy largas (rutas/CJK) en una sección**: truncar por anchura de display con `…`, sin desbordar. Test en Task 6.
- **Frontera de altura**: exactamente `LOGO_MIN_HEIGHT` usa el wordmark; una fila menos usa el compacto. Test en Task 6.

---

## File Structure

```
crates/batten/
  src/
    state/
      transcript.rs   # + StyleToken::{AccentFrom, AccentTo, LogoField}
      sidebar.rs      # NUEVO: Logo, Section, Sidebar (puro)
      mod.rs          # + pub mod sidebar; + re-exports
    widgets/
      gradient.rs     # NUEVO: color_of + lerp_color (compartido)
      spinner.rs      # usa gradient (se le quitan los helpers)
      logo.rs         # NUEVO: FONT_ROWS, glyph, Wordmark, render, compact
      sidebar.rs      # NUEVO: LOGO_MIN_HEIGHT, render
      theme.rs        # + campos/tokens AccentFrom/AccentTo/LogoField
      mod.rs          # + mod gradient; + pub mod logo; + pub mod sidebar
  examples/
    demo.rs           # + columna del sidebar
crates/codeloom-tui/
  Cargo.toml          # + batten (path)
  src/
    app.rs            # + theme; + App::sidebar()
    render.rs         # draw_sidebar -> batten::widgets::sidebar::render
    widgets/
      mod.rs          # - pub mod sidebar
      sidebar.rs      # ELIMINADO
```

---

### Task 1: Tokens de tema `AccentFrom` / `AccentTo` / `LogoField`

**Files:**
- Modify: `crates/batten/src/state/transcript.rs` (enum `StyleToken`)
- Modify: `crates/batten/src/widgets/theme.rs`

**Interfaces:**
- Consumes: nada.
- Produces: `StyleToken::{AccentFrom, AccentTo, LogoField}` y campos equivalentes en `Styles`.

- [ ] **Step 1: Escribir los tests**

En `crates/batten/src/widgets/theme.rs`, dentro de `mod tests`, añade:

```rust
    #[test]
    fn accent_gradient_tokens_resolve() {
        let th = Theme::test();
        assert_eq!(th.style(StyleToken::AccentFrom).fg, Some(Color::Rgb(0, 212, 255)));
        assert_eq!(th.style(StyleToken::AccentTo).fg, Some(Color::Rgb(124, 92, 255)));
        assert_eq!(th.style(StyleToken::LogoField).fg, Some(Color::Indexed(238)));
    }

    #[test]
    fn accent_tokens_can_be_overridden() {
        let th = Theme::from_toml("[styles]\naccent_from = \"magenta\"\n").unwrap();
        assert_eq!(th.style(StyleToken::AccentFrom).fg, Some(Color::Magenta));
    }
```

- [ ] **Step 2: Ejecutar para ver fallar**

Run: `cargo test -p batten widgets::theme`
Expected: FAIL (no compila: `StyleToken::AccentFrom` no existe).

- [ ] **Step 3: Implementar**

En `crates/batten/src/state/transcript.rs`, en `enum StyleToken`, añade tres variantes al final (antes del `}`):

```rust
    Accent,
    AccentFrom,
    AccentTo,
    LogoField,
```

En `crates/batten/src/widgets/theme.rs`:

1. En `struct Styles`, tras `pub accent: Style,`:

```rust
    pub accent: Style,
    pub accent_from: Style,
    pub accent_to: Style,
    pub logo_field: Style,
```

2. En `impl Default for Styles`, tras `accent: Style::default().fg(Color::Rgb(0, 212, 255)),`:

```rust
            accent: Style::default().fg(Color::Rgb(0, 212, 255)),
            accent_from: Style::default().fg(Color::Rgb(0, 212, 255)),
            accent_to: Style::default().fg(Color::Rgb(124, 92, 255)),
            logo_field: Style::default().fg(Color::Indexed(238)),
```

3. En `fn set_named_style`, tras `StyleToken::Accent => s.accent,`:

```rust
            StyleToken::Accent => s.accent,
            StyleToken::AccentFrom => s.accent_from,
            StyleToken::AccentTo => s.accent_to,
            StyleToken::LogoField => s.logo_field,
```

4. En `fn apply_override`, tras `"accent" => s.accent = style,`:

```rust
            "accent" => s.accent = style,
            "accent_from" => s.accent_from = style,
            "accent_to" => s.accent_to = style,
            "logo_field" => s.logo_field = style,
```

5. En `fn token_to_key`, tras `StyleToken::Accent => "accent",`:

```rust
        StyleToken::Accent => "accent",
        StyleToken::AccentFrom => "accent_from",
        StyleToken::AccentTo => "accent_to",
        StyleToken::LogoField => "logo_field",
```

- [ ] **Step 4: Ejecutar para ver pasar**

Run: `cargo test -p batten widgets::theme`
Expected: PASS (7 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/batten
git commit -m "feat(batten): accent gradient and logo-field theme tokens"
```

---

### Task 2: `widgets::logo` — fuente de bloques y `Wordmark`

**Files:**
- Create: `crates/batten/src/widgets/logo.rs`
- Modify: `crates/batten/src/widgets/mod.rs`

**Interfaces:**
- Consumes: nada.
- Produces: `pub const FONT_ROWS: usize`; `fn glyph(c: char) -> [&'static str; FONT_ROWS]` (privada); `pub struct Wordmark` con `new(&str)`, `rows() -> &[String]`, `width() -> usize`, `height() -> usize`.

- [ ] **Step 1: Escribir los tests**

Crea `crates/batten/src/widgets/logo.rs` con:

```rust
//! Logo paramétrico: fuente de bloques 3x5 y wordmark.

/// Filas de la fuente de bloques.
pub const FONT_ROWS: usize = 5;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_supported_char_has_font_rows_of_three_columns() {
        for c in "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 .-/_".chars() {
            let g = glyph(c);
            assert_eq!(g.len(), FONT_ROWS, "glyph {c:?} rows");
            for row in g {
                assert_eq!(row.chars().count(), 3, "glyph {c:?} row width");
            }
        }
    }

    #[test]
    fn unknown_char_falls_back_to_space() {
        assert_eq!(glyph('?'), glyph(' '));
        assert_eq!(glyph('é'), glyph(' '));
        assert_eq!(glyph('🚀'), glyph(' '));
    }

    #[test]
    fn wordmark_of_a_known_name() {
        let wm = Wordmark::new("AB");
        assert_eq!(wm.height(), FONT_ROWS);
        assert_eq!(wm.width(), 6);
        assert_eq!(
            wm.rows(),
            [
                " \u{2588} \u{2588}\u{2588} ",
                "\u{2588} \u{2588}\u{2588} \u{2588}",
                "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} ",
                "\u{2588} \u{2588}\u{2588} \u{2588}",
                "\u{2588} \u{2588}\u{2588}\u{2588} ",
            ]
        );
    }

    #[test]
    fn lowercase_is_upcased() {
        assert_eq!(Wordmark::new("ab").rows(), Wordmark::new("AB").rows());
    }

    #[test]
    fn empty_name_has_no_width() {
        let wm = Wordmark::new("");
        assert_eq!(wm.width(), 0);
        assert_eq!(wm.height(), FONT_ROWS);
    }
}
```

- [ ] **Step 2: Ejecutar para ver fallar**

Run: `cargo test -p batten widgets::logo`
Expected: FAIL (no compila: `glyph`/`Wordmark` no existen).

- [ ] **Step 3: Implementar**

Añade a `crates/batten/src/widgets/logo.rs`, **encima** de `#[cfg(test)]`:

```rust
/// Arte de bloque para un carácter (3 columnas x 5 filas). Un carácter fuera de
/// la tabla devuelve el glifo de espacio.
fn glyph(c: char) -> [&'static str; FONT_ROWS] {
    match c {
        'A' => [" \u{2588} ", "\u{2588} \u{2588}", "\u{2588}\u{2588}\u{2588}", "\u{2588} \u{2588}", "\u{2588} \u{2588}"],
        'B' => ["\u{2588}\u{2588} ", "\u{2588} \u{2588}", "\u{2588}\u{2588} ", "\u{2588} \u{2588}", "\u{2588}\u{2588} "],
        'C' => [" \u{2588}\u{2588}", "\u{2588}  ", "\u{2588}  ", "\u{2588}  ", " \u{2588}\u{2588}"],
        'D' => ["\u{2588}\u{2588} ", "\u{2588} \u{2588}", "\u{2588} \u{2588}", "\u{2588} \u{2588}", "\u{2588}\u{2588} "],
        'E' => ["\u{2588}\u{2588}\u{2588}", "\u{2588}  ", "\u{2588}\u{2588} ", "\u{2588}  ", "\u{2588}\u{2588}\u{2588}"],
        'F' => ["\u{2588}\u{2588}\u{2588}", "\u{2588}  ", "\u{2588}\u{2588} ", "\u{2588}  ", "\u{2588}  "],
        'G' => [" \u{2588}\u{2588}", "\u{2588}  ", "\u{2588} \u{2588}", "\u{2588} \u{2588}", " \u{2588}\u{2588}"],
        'H' => ["\u{2588} \u{2588}", "\u{2588} \u{2588}", "\u{2588}\u{2588}\u{2588}", "\u{2588} \u{2588}", "\u{2588} \u{2588}"],
        'I' => ["\u{2588}\u{2588}\u{2588}", " \u{2588} ", " \u{2588} ", " \u{2588} ", "\u{2588}\u{2588}\u{2588}"],
        'J' => ["  \u{2588}", "  \u{2588}", "  \u{2588}", "\u{2588} \u{2588}", " \u{2588} "],
        'K' => ["\u{2588} \u{2588}", "\u{2588} \u{2588}", "\u{2588}\u{2588} ", "\u{2588} \u{2588}", "\u{2588} \u{2588}"],
        'L' => ["\u{2588}  ", "\u{2588}  ", "\u{2588}  ", "\u{2588}  ", "\u{2588}\u{2588}\u{2588}"],
        'M' => ["\u{2588} \u{2588}", "\u{2588}\u{2588}\u{2588}", "\u{2588} \u{2588}", "\u{2588} \u{2588}", "\u{2588} \u{2588}"],
        'N' => ["\u{2588} \u{2588}", "\u{2588}\u{2588}\u{2588}", "\u{2588}\u{2588}\u{2588}", "\u{2588} \u{2588}", "\u{2588} \u{2588}"],
        'O' => [" \u{2588} ", "\u{2588} \u{2588}", "\u{2588} \u{2588}", "\u{2588} \u{2588}", " \u{2588} "],
        'P' => ["\u{2588}\u{2588} ", "\u{2588} \u{2588}", "\u{2588}\u{2588} ", "\u{2588}  ", "\u{2588}  "],
        'Q' => [" \u{2588} ", "\u{2588} \u{2588}", "\u{2588} \u{2588}", "\u{2588}\u{2588}\u{2588}", " \u{2588}\u{2588}"],
        'R' => ["\u{2588}\u{2588} ", "\u{2588} \u{2588}", "\u{2588}\u{2588} ", "\u{2588} \u{2588}", "\u{2588} \u{2588}"],
        'S' => [" \u{2588}\u{2588}", "\u{2588}  ", " \u{2588} ", "  \u{2588}", "\u{2588}\u{2588} "],
        'T' => ["\u{2588}\u{2588}\u{2588}", " \u{2588} ", " \u{2588} ", " \u{2588} ", " \u{2588} "],
        'U' => ["\u{2588} \u{2588}", "\u{2588} \u{2588}", "\u{2588} \u{2588}", "\u{2588} \u{2588}", " \u{2588} "],
        'V' => ["\u{2588} \u{2588}", "\u{2588} \u{2588}", "\u{2588} \u{2588}", " \u{2588} ", " \u{2588} "],
        'W' => ["\u{2588} \u{2588}", "\u{2588} \u{2588}", "\u{2588}\u{2588}\u{2588}", "\u{2588}\u{2588}\u{2588}", "\u{2588} \u{2588}"],
        'X' => ["\u{2588} \u{2588}", "\u{2588} \u{2588}", " \u{2588} ", "\u{2588} \u{2588}", "\u{2588} \u{2588}"],
        'Y' => ["\u{2588} \u{2588}", "\u{2588} \u{2588}", " \u{2588} ", " \u{2588} ", " \u{2588} "],
        'Z' => ["\u{2588}\u{2588}\u{2588}", "  \u{2588}", " \u{2588} ", "\u{2588}  ", "\u{2588}\u{2588}\u{2588}"],
        '0' => [" \u{2588} ", "\u{2588} \u{2588}", "\u{2588} \u{2588}", "\u{2588} \u{2588}", " \u{2588} "],
        '1' => [" \u{2588} ", "\u{2588}\u{2588} ", " \u{2588} ", " \u{2588} ", "\u{2588}\u{2588}\u{2588}"],
        '2' => ["\u{2588}\u{2588} ", "  \u{2588}", " \u{2588} ", "\u{2588}  ", "\u{2588}\u{2588}\u{2588}"],
        '3' => ["\u{2588}\u{2588} ", "  \u{2588}", " \u{2588} ", "  \u{2588}", "\u{2588}\u{2588} "],
        '4' => ["\u{2588} \u{2588}", "\u{2588} \u{2588}", "\u{2588}\u{2588}\u{2588}", "  \u{2588}", "  \u{2588}"],
        '5' => ["\u{2588}\u{2588}\u{2588}", "\u{2588}  ", "\u{2588}\u{2588} ", "  \u{2588}", "\u{2588}\u{2588} "],
        '6' => [" \u{2588}\u{2588}", "\u{2588}  ", "\u{2588}\u{2588} ", "\u{2588} \u{2588}", " \u{2588} "],
        '7' => ["\u{2588}\u{2588}\u{2588}", "  \u{2588}", " \u{2588} ", " \u{2588} ", " \u{2588} "],
        '8' => [" \u{2588} ", "\u{2588} \u{2588}", " \u{2588} ", "\u{2588} \u{2588}", " \u{2588} "],
        '9' => [" \u{2588} ", "\u{2588} \u{2588}", " \u{2588}\u{2588}", "  \u{2588}", "\u{2588}\u{2588} "],
        ' ' => ["   ", "   ", "   ", "   ", "   "],
        '.' => ["   ", "   ", "   ", "   ", " \u{2588} "],
        '-' => ["   ", "   ", "\u{2588}\u{2588}\u{2588}", "   ", "   "],
        '_' => ["   ", "   ", "   ", "   ", "\u{2588}\u{2588}\u{2588}"],
        '/' => ["  \u{2588}", "  \u{2588}", " \u{2588} ", "\u{2588}  ", "\u{2588}  "],
        ':' => ["   ", " \u{2588} ", "   ", " \u{2588} ", "   "],
        _ => glyph(' '),
    }
}

/// Wordmark de bloques construido a partir de un nombre (denso, sin separación
/// entre glifos; cada glifo ocupa 3 columnas).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wordmark {
    rows: Vec<String>,
}

impl Wordmark {
    pub fn new(name: &str) -> Self {
        let mut rows: Vec<String> = vec![String::new(); FONT_ROWS];
        for c in name.chars().flat_map(char::to_uppercase) {
            let g = glyph(c);
            for (r, row) in rows.iter_mut().enumerate() {
                row.push_str(g[r]);
            }
        }
        Self { rows }
    }

    pub fn rows(&self) -> &[String] {
        &self.rows
    }

    pub fn width(&self) -> usize {
        self.rows.first().map_or(0, |r| r.chars().count())
    }

    pub fn height(&self) -> usize {
        FONT_ROWS
    }
}
```

En `crates/batten/src/widgets/mod.rs`, añade `pub mod logo;` (en orden alfabético, antes de `spinner`).

- [ ] **Step 4: Ejecutar para ver pasar**

Run: `cargo test -p batten widgets::logo`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/batten
git commit -m "feat(batten): parametric 3x5 block font and wordmark"
```

---

### Task 3: Extraer los helpers de degradado (`widgets::gradient`)

Refactor sin cambio de comportamiento: el spinner y el logo comparten el cálculo de color.

**Files:**
- Create: `crates/batten/src/widgets/gradient.rs`
- Modify: `crates/batten/src/widgets/spinner.rs` (quitar helpers locales)
- Modify: `crates/batten/src/widgets/mod.rs` (`mod gradient;`)

**Interfaces:**
- Consumes: nada.
- Produces: `pub(crate) fn color_of(style: Style) -> Color`; `pub(crate) fn lerp_color(a: Color, b: Color, t: f32) -> Color`.

- [ ] **Step 1: Escribir el test**

Crea `crates/batten/src/widgets/gradient.rs` con:

```rust
//! Helpers de color compartidos por los widgets con degradado (spinner, logo).

use ratatui::style::{Color, Style};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_color_hits_both_endpoints() {
        let a = Color::Rgb(0, 0, 0);
        let b = Color::Rgb(10, 20, 30);
        assert_eq!(lerp_color(a, b, 0.0), Color::Rgb(0, 0, 0));
        assert_eq!(lerp_color(a, b, 1.0), Color::Rgb(10, 20, 30));
    }

    #[test]
    fn color_of_falls_back_to_cyan() {
        assert_eq!(color_of(Style::default()), Color::Cyan);
    }
}
```

- [ ] **Step 2: Ejecutar para ver fallar**

Run: `cargo test -p batten widgets::gradient`
Expected: FAIL (no compila: `lerp_color`/`color_of` no existen).

- [ ] **Step 3: Implementar**

Añade a `crates/batten/src/widgets/gradient.rs`, encima de `#[cfg(test)]`:

```rust
/// Color de primer plano del estilo, con un fallback si no tiene.
pub(crate) fn color_of(style: Style) -> Color {
    style.fg.unwrap_or(Color::Cyan)
}

fn lerp(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round() as u8
}

fn to_rgb(c: Color) -> (u8, u8, u8) {
    match c {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Cyan => (0, 200, 255),
        Color::Green => (0, 200, 120),
        _ => (200, 200, 200),
    }
}

/// Interpola entre dos colores; `t` en [0, 1].
pub(crate) fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let (ar, ag, ab) = to_rgb(a);
    let (br, bg, bb) = to_rgb(b);
    Color::Rgb(lerp(ar, br, t), lerp(ag, bg, t), lerp(ab, bb, t))
}
```

En `crates/batten/src/widgets/spinner.rs`:

1. Al inicio, tras los `use` existentes, añade:

```rust
use super::gradient::{color_of, lerp_color};
```

2. **Elimina** las funciones locales `fn color_of`, `fn lerp`, `fn to_rgb` y `fn lerp_color` (las cuatro, al final del archivo antes de `#[cfg(test)]`).

En `crates/batten/src/widgets/mod.rs`, añade `mod gradient;` (privado; antes de `pub mod logo;`).

- [ ] **Step 4: Ejecutar para ver pasar**

Run: `cargo test -p batten`
Expected: PASS (los tests del spinner y los nuevos de gradient siguen verdes; ningún test del spinner cambia).

- [ ] **Step 5: Commit**

```bash
git add crates/batten
git commit -m "refactor(batten): share gradient color helpers between widgets"
```

---

### Task 4: `widgets::logo` — `render` y `compact`

**Files:**
- Modify: `crates/batten/src/widgets/logo.rs`

**Interfaces:**
- Consumes: `Wordmark` (Task 2), `gradient::{color_of, lerp_color}` (Task 3), `Theme`, `StyleToken::{AccentFrom, AccentTo, Heading, LogoField}` (Task 1).
- Produces: `pub fn render(wordmark: &Wordmark, theme: &Theme) -> Vec<Line<'static>>`; `pub fn compact(name: &str, width: usize, theme: &Theme) -> Vec<Line<'static>>`.

- [ ] **Step 1: Escribir los tests**

En `crates/batten/src/widgets/logo.rs`, dentro de `mod tests`, añade:

```rust
    use crate::widgets::Theme;
    use ratatui::text::Line;

    fn text_of(lines: &[Line]) -> String {
        lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect()
    }

    #[test]
    fn render_draws_font_rows_of_blocks() {
        let lines = render(&Wordmark::new("A"), &Theme::test());
        assert_eq!(lines.len(), FONT_ROWS);
        assert!(text_of(&lines).contains('\u{2588}'));
    }

    #[test]
    fn compact_shows_the_name_and_field_fill() {
        let lines = compact("batten", 20, &Theme::test());
        let text = text_of(&lines);
        assert!(text.contains("batten"));
        assert!(text.contains('\u{2571}'));
    }

    #[test]
    fn compact_with_zero_width_is_just_the_name() {
        let lines = compact("batten", 0, &Theme::test());
        assert_eq!(text_of(&lines), "batten");
    }
```

- [ ] **Step 2: Ejecutar para ver fallar**

Run: `cargo test -p batten widgets::logo`
Expected: FAIL (no compila: `render`/`compact` no existen).

- [ ] **Step 3: Implementar**

Añade a `crates/batten/src/widgets/logo.rs`, encima de `#[cfg(test)]` (y tras `impl Wordmark`), estos `use` y funciones:

```rust
use super::gradient::{color_of, lerp_color};
use super::Theme;
use crate::state::StyleToken;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

/// Estilo por columna: degradado de `AccentFrom` (t=0) a `AccentTo` (t=1).
fn gradient_style(col: usize, width: usize, theme: &Theme) -> Style {
    let t = if width <= 1 {
        0.0
    } else {
        col as f32 / (width - 1) as f32
    };
    let from = color_of(theme.style(StyleToken::AccentFrom));
    let to = color_of(theme.style(StyleToken::AccentTo));
    Style::default().fg(lerp_color(from, to, t))
}

/// Wordmark de bloques: `FONT_ROWS` líneas con el degradado por columna. Los
/// huecos van sin estilo.
pub fn render(wordmark: &Wordmark, theme: &Theme) -> Vec<Line<'static>> {
    let width = wordmark.width();
    wordmark
        .rows()
        .iter()
        .map(|row| {
            let spans: Vec<Span> = row
                .chars()
                .enumerate()
                .map(|(col, ch)| {
                    if ch == ' ' {
                        Span::raw(" ")
                    } else {
                        Span::styled(ch.to_string(), gradient_style(col, width, theme))
                    }
                })
                .collect();
            Line::from(spans)
        })
        .collect()
}

/// Fallback de una línea: nombre (estilo `Heading`) + `╱` (estilo `LogoField`)
/// hasta `width`. Con `width` 0 devuelve solo el nombre.
pub fn compact(name: &str, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let used = name.chars().count();
    let fill = width.saturating_sub(used);
    let mut spans = vec![Span::styled(name.to_string(), theme.style(StyleToken::Heading))];
    if fill > 0 {
        spans.push(Span::styled(
            theme.glyphs.field.to_string().repeat(fill),
            theme.style(StyleToken::LogoField),
        ));
    }
    vec![Line::from(spans)]
}
```

- [ ] **Step 4: Ejecutar para ver pasar**

Run: `cargo test -p batten widgets::logo`
Expected: PASS (8 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/batten
git commit -m "feat(batten): render the block wordmark with a column gradient"
```

---

### Task 5: `state::sidebar` — modelo puro

**Files:**
- Create: `crates/batten/src/state/sidebar.rs`
- Modify: `crates/batten/src/state/mod.rs`

**Interfaces:**
- Consumes: nada.
- Produces: `pub struct Logo { pub name: String }`; `pub struct Section { pub title: String, pub lines: Vec<String> }`; `pub struct Sidebar { pub logo: Option<Logo>, pub sections: Vec<Section> }` con `new()`, `with_logo(impl Into<String>)`, `push_section(impl Into<String>, Vec<String>)`.

- [ ] **Step 1: Escribir los tests**

Crea `crates/batten/src/state/sidebar.rs` con:

```rust
//! Modelo puro del sidebar: logo + secciones. Sin ratatui.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_has_no_logo_and_no_sections() {
        let sb = Sidebar::new();
        assert!(sb.logo.is_none());
        assert!(sb.sections.is_empty());
    }

    #[test]
    fn with_logo_sets_the_name() {
        let sb = Sidebar::with_logo("batten");
        assert_eq!(sb.logo.as_ref().unwrap().name, "batten");
    }

    #[test]
    fn push_section_keeps_order_and_content() {
        let mut sb = Sidebar::new();
        sb.push_section("estado", vec!["working".into()]);
        sb.push_section("modelo", vec!["mock".into(), "x".into()]);
        assert_eq!(sb.sections.len(), 2);
        assert_eq!(sb.sections[0].title, "estado");
        assert_eq!(sb.sections[0].lines, vec!["working".to_string()]);
        assert_eq!(sb.sections[1].title, "modelo");
        assert_eq!(sb.sections[1].lines.len(), 2);
    }
}
```

- [ ] **Step 2: Ejecutar para ver fallar**

Run: `cargo test -p batten state::sidebar`
Expected: FAIL (no compila: tipos no existen).

- [ ] **Step 3: Implementar**

Añade a `crates/batten/src/state/sidebar.rs`, encima de `#[cfg(test)]`:

```rust
/// Marca del sidebar. Solo el nombre; el dibujo lo pone `widgets::logo`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Logo {
    pub name: String,
}

/// Sección del sidebar: un título y sus líneas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub title: String,
    pub lines: Vec<String>,
}

/// Sidebar genérico: logo opcional + secciones en orden.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Sidebar {
    pub logo: Option<Logo>,
    pub sections: Vec<Section>,
}

impl Sidebar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_logo(name: impl Into<String>) -> Self {
        Self {
            logo: Some(Logo { name: name.into() }),
            sections: Vec::new(),
        }
    }

    pub fn push_section(&mut self, title: impl Into<String>, lines: Vec<String>) {
        self.sections.push(Section {
            title: title.into(),
            lines,
        });
    }
}
```

En `crates/batten/src/state/mod.rs`, añade `pub mod sidebar;` y `pub use sidebar::{Logo, Section, Sidebar};`.

- [ ] **Step 4: Ejecutar para ver pasar**

Run: `cargo test -p batten state::sidebar`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/batten
git commit -m "feat(batten): sidebar state model (logo plus sections)"
```

---

### Task 6: `widgets::sidebar` — render

**Files:**
- Create: `crates/batten/src/widgets/sidebar.rs`
- Modify: `crates/batten/src/widgets/mod.rs`

**Interfaces:**
- Consumes: `state::{Section, Sidebar, StyleToken}` (Task 5), `widgets::logo::{self, Wordmark, FONT_ROWS}` (Tasks 2/4), `Theme`.
- Produces: `pub const LOGO_MIN_HEIGHT: usize`; `pub fn render(frame: &mut Frame, area: Rect, sidebar: &Sidebar, theme: &Theme)`.

- [ ] **Step 1: Escribir los tests**

Crea `crates/batten/src/widgets/sidebar.rs` con:

```rust
//! Render del sidebar: logo (o compacto) + secciones.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Sidebar;
    use crate::widgets::Theme;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn buffer(sb: &Sidebar, w: u16, h: u16) -> String {
        let backend = TestBackend::new(w, h);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f, f.area(), sb, &Theme::test())).unwrap();
        term.backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn shows_block_logo_and_sections() {
        let mut sb = Sidebar::with_logo("batten");
        sb.push_section("estado", vec!["working".into()]);
        let out = buffer(&sb, 32, 40);
        assert!(out.contains('\u{2588}'));
        assert!(out.contains("estado"));
        assert!(out.contains("working"));
    }

    #[test]
    fn compact_logo_when_the_wordmark_does_not_fit() {
        let sb = Sidebar::with_logo("batten");
        let out = buffer(&sb, 32, LOGO_MIN_HEIGHT as u16 - 1);
        assert!(out.contains("batten"));
    }

    #[test]
    fn compact_logo_when_too_narrow_for_the_wordmark() {
        let sb = Sidebar::with_logo("batten");
        let out = buffer(&sb, 8, 40);
        assert!(out.contains("batten"));
    }

    #[test]
    fn truncates_long_lines_with_an_ellipsis() {
        let mut sb = Sidebar::new();
        sb.push_section("s", vec!["x".repeat(100)]);
        let out = buffer(&sb, 20, 10);
        assert!(out.contains('\u{2026}'));
    }

    #[test]
    fn truncates_on_display_width_not_bytes() {
        // CJK es 2 columnas por carácter: truncar por bytes desbordaría.
        let mut sb = Sidebar::new();
        sb.push_section("s", vec!["\u{661f}".repeat(50)]);
        let _ = buffer(&sb, 20, 10);
    }

    #[test]
    fn zero_and_tiny_sizes_do_not_panic() {
        let sb = Sidebar::with_logo("batten");
        let _ = buffer(&sb, 0, 0);
        let _ = buffer(&sb, 1, 1);
        let _ = buffer(&sb, 1, 40);
    }
}
```

- [ ] **Step 2: Ejecutar para ver fallar**

Run: `cargo test -p batten widgets::sidebar`
Expected: FAIL (no compila: `render`/`LOGO_MIN_HEIGHT` no existen).

- [ ] **Step 3: Implementar**

Añade a `crates/batten/src/widgets/sidebar.rs`, encima de `#[cfg(test)]`:

```rust
use crate::state::{Sidebar, StyleToken};
use crate::widgets::logo::{self, FONT_ROWS, Wordmark};
use crate::widgets::Theme;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Altura mínima para el wordmark de bloques (wordmark + una línea en blanco).
pub const LOGO_MIN_HEIGHT: usize = FONT_ROWS + 2;

pub fn render(frame: &mut Frame, area: Rect, sidebar: &Sidebar, theme: &Theme) {
    let block = Block::default().borders(Borders::LEFT);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let width = inner.width as usize;
    let height = inner.height as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();

    if let Some(log) = &sidebar.logo {
        let wm = Wordmark::new(&log.name);
        if height >= LOGO_MIN_HEIGHT && wm.width() <= width {
            lines.extend(logo::render(&wm, theme));
        } else {
            lines.extend(logo::compact(&log.name, width, theme));
        }
        lines.push(Line::from(""));
    }

    for section in &sidebar.sections {
        lines.push(Line::from(Span::styled(
            section.title.clone(),
            theme.style(StyleToken::Heading),
        )));
        for l in &section.lines {
            lines.push(Line::from(Span::styled(
                truncate(l, width),
                theme.style(StyleToken::Default),
            )));
        }
        lines.push(Line::from(""));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

/// Trunca por anchura de display reservando una columna para `…`.
fn truncate(s: &str, max: usize) -> String {
    if s.width() <= max {
        return s.to_string();
    }
    let mut out = String::new();
    let mut w = 0;
    for c in s.chars() {
        let cw = UnicodeWidthChar::width(c).unwrap_or(0);
        if w + cw > max.saturating_sub(1) {
            break;
        }
        out.push(c);
        w += cw;
    }
    out.push('\u{2026}');
    out
}
```

En `crates/batten/src/widgets/mod.rs`, añade `pub mod sidebar;` (en orden alfabético).

- [ ] **Step 4: Ejecutar para ver pasar**

Run: `cargo test -p batten widgets::sidebar`
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/batten
git commit -m "feat(batten): sidebar widget with block or compact logo"
```

---

### Task 7: El demo muestra el sidebar

**Files:**
- Modify: `crates/batten/examples/demo.rs`

**Interfaces:**
- Consumes: `state::Sidebar`, `widgets::sidebar::render`.
- Produces: nada.

- [ ] **Step 1: Construir el sidebar e integrarlo en el layout**

En `crates/batten/examples/demo.rs`, tras la línea `let started = Instant::now();`, añade:

```rust
    let mut side = batten::state::Sidebar::with_logo("batten");
    side.push_section("state", vec!["working".into()]);
    side.push_section("model", vec!["mock".into()]);
```

Y sustituye el bloque `terminal.draw(...)` completo por:

```rust
        terminal.draw(|f| {
            let area = f.area();
            let rows = ratatui::layout::Layout::vertical([
                ratatui::layout::Constraint::Min(1),
                ratatui::layout::Constraint::Length(3),
            ])
            .split(area);
            let (main, side_area) = if area.width >= 80 {
                let cols = ratatui::layout::Layout::horizontal([
                    ratatui::layout::Constraint::Min(1),
                    ratatui::layout::Constraint::Length(30),
                ])
                .split(rows[0]);
                (cols[0], Some(cols[1]))
            } else {
                (rows[0], None)
            };
            if scenario == "tools" {
                let top = ratatui::layout::Layout::vertical([
                    ratatui::layout::Constraint::Length(2),
                    ratatui::layout::Constraint::Min(1),
                ])
                .split(main);
                widgets::tool_item::render(f, top[0], &tool, &anim, &theme);
                widgets::transcript::render(f, top[1], &transcript, &viewport, &theme);
            } else {
                widgets::transcript::render(f, main, &transcript, &viewport, &theme);
            }
            if let Some(sa) = side_area {
                batten::widgets::sidebar::render(f, sa, &side, &theme);
            }
            widgets::composer::render(f, rows[1], &composer, &theme);
        })?;
```

- [ ] **Step 2: Compilar y formatear**

Run: `cargo build -p batten --example demo && cargo fmt -p batten --check`
Expected: PASS.

- [ ] **Step 3: Verificación manual**

Run: `cargo run -p batten --example demo -- basic`
Expected: transcript + composer + sidebar a la derecha con el wordmark de bloques "BATTEN" y las secciones `state`/`model`; `Esc`/`q` sale.
Run: `cargo run -p batten --example demo -- tools`
Expected: igual más la tool `view` con spinner.

- [ ] **Step 4: Commit**

```bash
git add crates/batten
git commit -m "feat(batten): show the sidebar in the fake-event demo"
```

---

### Task 8: Integración en `codeloom-tui`

**Files:**
- Modify: `crates/codeloom-tui/Cargo.toml`
- Modify: `crates/codeloom-tui/src/app.rs`
- Modify: `crates/codeloom-tui/src/render.rs`
- Modify: `crates/codeloom-tui/src/widgets/mod.rs`
- Delete: `crates/codeloom-tui/src/widgets/sidebar.rs`

**Interfaces:**
- Consumes: `batten::state::Sidebar`, `batten::widgets::{Theme, sidebar::render}`.
- Produces: `App.theme: batten::widgets::Theme`; `App::sidebar(&self) -> batten::state::Sidebar`.

- [ ] **Step 1: Añadir la dependencia**

En `crates/codeloom-tui/Cargo.toml`, dentro de `[dependencies]`:

```toml
batten = { path = "../batten" }
```

- [ ] **Step 2: Escribir el test de `App::sidebar`**

En `crates/codeloom-tui/src/app.rs`, dentro de `mod tests`, añade:

```rust
    #[test]
    fn sidebar_has_logo_and_session_sections() {
        let app = App::with_context("/proj", "mistral-nemo");
        let sb = app.sidebar();
        assert_eq!(sb.logo.as_ref().unwrap().name, "CodeLoom");
        let titles: Vec<&str> = sb.sections.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(
            titles,
            ["estado", "cwd", "modelo", "agente", "subagentes / jobs"]
        );
        assert_eq!(sb.sections[1].lines, vec!["/proj".to_string()]);
    }
```

- [ ] **Step 3: Ejecutar para ver fallar**

Run: `cargo test -p codeloom-tui app::tests::sidebar_has_logo_and_session_sections`
Expected: FAIL (no compila: `App::sidebar` no existe).

- [ ] **Step 4: Implementar `App::sidebar` y el tema**

En `crates/codeloom-tui/src/app.rs`:

1. En `struct App`, añade el campo:

```rust
    pub cwd: String,
    pub model: String,
    pub theme: batten::widgets::Theme,
```

2. En `App::new()`, añade el inicializador (junto a `cwd`/`model`):

```rust
            cwd: String::new(),
            model: String::new(),
            theme: batten::widgets::Theme::default_dark(),
```

3. Añade el método (dentro de `impl App`):

```rust
    /// Construye el sidebar de batten con la marca y las secciones de sesión.
    pub fn sidebar(&self) -> batten::state::Sidebar {
        let mut sb = batten::state::Sidebar::with_logo("CodeLoom");
        sb.push_section("estado", vec![self.status.clone()]);
        sb.push_section("cwd", vec![self.cwd.clone()]);
        sb.push_section("modelo", vec![self.model.clone()]);
        sb.push_section("agente", vec!["steps    —".into()]);
        sb.push_section("subagentes / jobs", vec!["(m2)".into()]);
        sb
    }
```

- [ ] **Step 5: Ejecutar para ver pasar el test de `App`**

Run: `cargo test -p codeloom-tui app::tests`
Expected: PASS.

- [ ] **Step 6: Cablear el sidebar de batten en el render**

En `crates/codeloom-tui/src/render.rs`:

1. Elimina la línea `use crate::widgets::sidebar;`.
2. Elimina la función `fn draw_sidebar` completa.
3. Sustituye la llamada en la rama ancha (`sidebar::render(frame, cols[1], app);`) por:

```rust
        batten::widgets::sidebar::render(frame, cols[1], &app.sidebar(), &app.theme);
```

4. En `mod tests`, en `shows_sidebar_when_wide`, añade una comprobación del wordmark de bloques:

```rust
        assert!(content.contains("mistral-nemo"));
        assert!(content.contains("subagentes"));
        assert!(content.contains('\u{2588}'));
```

- [ ] **Step 7: Borrar el sidebar local**

En `crates/codeloom-tui/src/widgets/mod.rs`, elimina `pub mod sidebar;` (deja solo `pub mod permission;`).

Borra el archivo `crates/codeloom-tui/src/widgets/sidebar.rs`:

```bash
git rm crates/codeloom-tui/src/widgets/sidebar.rs
```

- [ ] **Step 8: Verificar la suite y los lints**

Run: `cargo test -p codeloom-tui && cargo test -p batten`
Expected: PASS (los tests de render de la TUI siguen verdes; el header mantiene "CodeLoom").

Run: `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 9: Verificación manual**

Run: `cargo run -p codeloom-cli --bin loom`
Expected: a la derecha, el sidebar con el wordmark de bloques "CODELOOM" y las secciones estado/cwd/modelo/agente/subagentes. En una terminal estrecha (<90 cols) el sidebar se oculta.

- [ ] **Step 10: Commit**

```bash
git add -A
git commit -m "feat(tui): use the batten sidebar instead of a local copy"
```

---

## Criterios de aceptación

- [ ] `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` pasan en todo el workspace.
- [ ] `batten` no importa `crossterm`; `state` no importa `ratatui`.
- [ ] El sidebar de batten es genérico (sin marca ni idioma).
- [ ] `codeloom-tui` usa el sidebar de batten y ya no tiene `widgets/sidebar.rs`.
