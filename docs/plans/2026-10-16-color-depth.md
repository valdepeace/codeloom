# Profundidad de color del tema — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Que los colores generados al pintar (degradados de logo, progress, panel y spinner; fondos de chip y botón) respeten el perfil del terminal: RGB → 256 → 16 → sin color.

**Architecture:** `Theme` gana `depth: ColorDepth` (enum propio de batten, sin dependencias) y `Theme::adapt(Color) -> Option<Color>`. `for_profile` fija la profundidad. Los widgets construyen sus colores dinámicos con dos helpers: `gradient::fg(theme, color)` (estilo con primer plano adaptado o sin color) y `tone::tone_fill(tone, theme)` (fondo del tono, o vídeo inverso sin color).

**Tech Stack:** Rust, `ratatui` 0.30, `termprofile` (feature opcional, activa por defecto).

**Spec:** entrada 4 de `docs/specs/2026-10-16-deuda-tecnica.md` ("Pasar el perfil a `gradient` o al `Theme`").

## Global Constraints

- Sin la feature `termprofile`, `ColorDepth` sigue existiendo y `adapt` solo distingue `TrueColor`/`NoColor` (256/16 se quedan como están): batten debe compilar con `--no-default-features`.
- Comentarios en español. Mensajes de commit: los indicados, tal cual.
- Cada commit pasa: `cargo test -p batten`, `cargo test -p batten --examples`, `cargo build -p batten --no-default-features`, `cargo test -p codeloom-tui`, `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`.
- **Entorno (Git Bash en Windows):** antes de cualquier `cargo`, en el mismo comando:
  `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe'`.
  No ejecutar `cargo build`/`cargo test` sin `-p`. No matar procesos. No crear directorios de target temporales.
- No hacer `git push`. No tocar ficheros fuera de los listados. No editar Rust con scripts.

## Review Focus

- Sin color, los chips y botones siguen distinguiéndose (vídeo inverso). Test en Task 2.
- Con 256 colores, el degradado emite `Indexed`, nunca `Rgb`. Test en Task 2.
- `--no-default-features` compila. Check en ambas tareas.
- Fuera de alcance: los efectos de `fx` (tachyonfx) interpolan en RGB; se registra como deuda nueva (Task 2, Step 6).

---

### Task 1: `ColorDepth` y `Theme::adapt`

**Files:**
- Modify: `crates/batten/src/widgets/theme.rs`, `crates/batten/src/widgets/profile.rs`, `crates/batten/src/widgets/mod.rs`

**Interfaces:**
- Produces: `batten::widgets::ColorDepth { TrueColor, Ansi256, Ansi16, NoColor }` (por defecto `TrueColor`); `Theme.depth: ColorDepth`; `Theme::adapt(&self, Color) -> Option<Color>`.

- [ ] **Step 1: Tipo y campo (`theme.rs`)**

1. Justo antes de `pub struct Theme {` (y su `#[derive(...)]`), añadir:
   ```rust
   /// Colores que admite el terminal. Lo fija `Theme::for_profile`; los
   /// widgets lo usan para los colores que generan al pintar.
   #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
   pub enum ColorDepth {
       #[default]
       TrueColor,
       Ansi256,
       Ansi16,
       NoColor,
   }
   ```
2. En `pub struct Theme`, después de `pub extensions: BTreeMap<String, Style>,`, añadir `pub depth: ColorDepth,`.
3. En `default_dark`, después de `extensions: BTreeMap::new(),`, añadir `depth: ColorDepth::TrueColor,`.
4. Dentro de `impl Theme`, después de `pub fn test() -> Self { ... }`, añadir:
   ```rust
    /// Adapta un color generado al pintar a la profundidad del tema.
    /// `None` = sin color (usar el del terminal).
    pub fn adapt(&self, color: Color) -> Option<Color> {
        match self.depth {
            ColorDepth::TrueColor => Some(color),
            ColorDepth::NoColor => None,
            ColorDepth::Ansi256 | ColorDepth::Ansi16 => adapt_lower(self.depth, color),
        }
    }
   ```
5. Fuera del `impl`, antes de `#[cfg(test)]`, añadir:
   ```rust
   #[cfg(feature = "termprofile")]
   fn adapt_lower(depth: ColorDepth, color: Color) -> Option<Color> {
       let profile = match depth {
           ColorDepth::Ansi256 => termprofile::TermProfile::Ansi256,
           _ => termprofile::TermProfile::Ansi16,
       };
       profile.adapt_color(color)
   }

   /// Sin `termprofile` no hay tablas de conversión: se deja el color.
   #[cfg(not(feature = "termprofile"))]
   fn adapt_lower(_depth: ColorDepth, color: Color) -> Option<Color> {
       Some(color)
   }
   ```
6. En `mod tests`, añadir:
   ```rust
    #[test]
    fn adapt_follows_the_depth() {
        let mut th = Theme::test();
        let rgb = Color::Rgb(0, 212, 255);
        assert_eq!(th.adapt(rgb), Some(rgb));
        th.depth = ColorDepth::NoColor;
        assert_eq!(th.adapt(rgb), None);
    }

    #[cfg(feature = "termprofile")]
    #[test]
    fn adapt_lowers_rgb_with_termprofile() {
        let mut th = Theme::test();
        th.depth = ColorDepth::Ansi256;
        assert!(matches!(th.adapt(Color::Rgb(0, 212, 255)), Some(Color::Indexed(_))));
        th.depth = ColorDepth::Ansi16;
        let c = th.adapt(Color::Rgb(0, 212, 255));
        assert!(c.is_some() && !matches!(c, Some(Color::Rgb(..)) | Some(Color::Indexed(_))));
    }
   ```

- [ ] **Step 2: Export (`widgets/mod.rs`)**

Cambiar `pub use theme::{Glyphs, Styles, Theme, ThemeError};` por `pub use theme::{ColorDepth, Glyphs, Styles, Theme, ThemeError};`.

- [ ] **Step 3: `for_profile` fija la profundidad (`profile.rs`)**

1. Cambiar `use super::theme::{Styles, Theme};` por `use super::theme::{ColorDepth, Styles, Theme};`.
2. Borrar el comentario de dos líneas que empieza por `// limitación: los degradados (logo, progress, panel, spinner)`.
3. En el literal final `Theme { styles, glyphs: self.glyphs, extensions, }`, añadir el campo:
   ```rust
            depth: match profile {
                TermProfile::TrueColor => ColorDepth::TrueColor,
                TermProfile::Ansi256 => ColorDepth::Ansi256,
                TermProfile::Ansi16 => ColorDepth::Ansi16,
                TermProfile::NoColor | TermProfile::NoTty => ColorDepth::NoColor,
            },
   ```
4. En su `mod tests`, añadir:
   ```rust
    #[test]
    fn profile_sets_the_depth() {
        use super::ColorDepth;
        assert_eq!(Theme::test().for_profile(TermProfile::Ansi256).depth, ColorDepth::Ansi256);
        assert_eq!(Theme::test().for_profile(TermProfile::NoTty).depth, ColorDepth::NoColor);
    }
   ```

- [ ] **Step 4: Checks**

Run: `cargo test -p batten && cargo test -p batten --examples && cargo build -p batten --no-default-features && cargo test -p codeloom-tui && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS. Si `TermProfile` tiene variantes distintas de las cinco usadas y el `match` no compila, reportar el error exacto.

- [ ] **Step 5: Commit**

```bash
git add crates/batten/src/widgets
git commit -m "feat(batten): profundidad de color en el tema y adaptación de colores"
```

---

### Task 2: los widgets usan la profundidad

**Files:**
- Modify: `crates/batten/src/widgets/gradient.rs`, `crates/batten/src/widgets/tone.rs`, `crates/batten/src/widgets/logo.rs`, `crates/batten/src/widgets/panel.rs`, `crates/batten/src/widgets/progress.rs`, `crates/batten/src/widgets/spinner.rs`, `crates/batten/src/widgets/chip.rs`, `crates/batten/src/widgets/button.rs`, `docs/specs/2026-10-16-deuda-tecnica.md`

**Interfaces:**
- Consumes: `Theme::adapt`, `ColorDepth` (Task 1).
- Produces: `gradient::fg(theme: &Theme, color: Color) -> Style`; `tone::tone_fill(tone: Tone, theme: &Theme) -> Style` (ambos `pub(crate)`).

- [ ] **Step 1: Helpers**

1. `gradient.rs`: añadir `use super::theme::Theme;` y, después de `lerp_color`, la función:
   ```rust
   /// Estilo con primer plano `color` adaptado a la profundidad del tema
   /// (sin color si el terminal no lo admite).
   pub(crate) fn fg(theme: &Theme, color: Color) -> Style {
       match theme.adapt(color) {
           Some(c) => Style::default().fg(c),
           None => Style::default(),
       }
   }
   ```
2. `tone.rs`: cambiar `use ratatui::style::Style;` por `use ratatui::style::{Color, Modifier, Style};`, añadir `use super::gradient::color_of;` y, después de `tone_style`, la función:
   ```rust
   /// Relleno de chip/botón: fondo del color del tono con texto oscuro; sin
   /// color, vídeo inverso para que se siga distinguiendo.
   pub(crate) fn tone_fill(tone: Tone, theme: &Theme) -> Style {
       match theme.adapt(color_of(tone_style(tone, theme))) {
           Some(bg) => Style::default()
               .bg(bg)
               .fg(Color::Black)
               .add_modifier(Modifier::BOLD),
           None => Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD),
       }
   }
   ```

- [ ] **Step 2: Degradados**

Sustituir cada construcción `Style::default().fg(lerp_color(from, to, t))` por `fg(theme, lerp_color(from, to, t))` y añadir `fg` al `use super::gradient::{...}` del fichero:
1. `logo.rs`, en `gradient_style` (última línea de la función).
2. `panel.rs`, en `header` (el `Style::default().fg(lerp_color(from, to, t))` del relleno `╱`).
3. `progress.rs`, en `progress` (relleno `━`) y en `progress_smooth` (closure `color`).
4. `spinner.rs`, en `glyph` (`Span::styled(shown, Style::default().fg(lerp_color(from, to, t)))` → `Span::styled(shown, fg(theme, lerp_color(from, to, t)))`) y en `render`: sustituir `Span::styled(shown.to_string(), Style::default().fg(color))` por `Span::styled(shown.to_string(), fg(theme, color))`.

Si tras el cambio algún fichero deja de usar `Style`, quitar ese import (clippy/compilador lo indicarán).

- [ ] **Step 3: Chip y botón**

1. `chip.rs`: sustituir el cuerpo de `chip` por:
   ```rust
    Span::styled(format!(" {text} "), tone_fill(tone, theme))
   ```
   cambiando los imports a `use super::theme::Theme;`, `use super::tone::tone_fill;`, `use crate::state::Tone;`, `use ratatui::text::Span;` (quitar los que queden sin usar). Los tests del fichero que usen `Color` deben importarlo en su `mod tests` (`use ratatui::style::Color;`).
2. `button.rs`: en `button`, sustituir el estilo enfocado
   `Style::default().bg(color_of(tone_style(tone, theme))).fg(Color::Black).add_modifier(Modifier::BOLD)` por `tone_fill(tone, theme)`, importar `super::tone::tone_fill` y quitar imports sin uso (los tests que usen `Color` lo importan en su `mod tests`).

- [ ] **Step 4: Tests**

1. `progress.rs`, en `mod tests`:
   ```rust
    #[test]
    fn no_color_depth_drops_the_gradient() {
        let mut th = Theme::test();
        th.depth = crate::widgets::ColorDepth::NoColor;
        let l = progress(1.0, 10, &th);
        assert_eq!(l.spans[0].style.fg, None);
    }

    #[cfg(feature = "termprofile")]
    #[test]
    fn ansi256_depth_never_emits_rgb() {
        let mut th = Theme::test();
        th.depth = crate::widgets::ColorDepth::Ansi256;
        for s in progress_smooth(0.7, 20, &th).spans {
            assert!(!matches!(s.style.fg, Some(ratatui::style::Color::Rgb(..))));
        }
    }
   ```
2. `chip.rs`, en `mod tests`:
   ```rust
    #[test]
    fn no_color_chip_uses_reverse_video() {
        use ratatui::style::Modifier;
        let mut th = Theme::test();
        th.depth = crate::widgets::ColorDepth::NoColor;
        let c = chip("ERROR", Tone::Error, &th);
        assert_eq!(c.style.bg, None);
        assert!(c.style.add_modifier.contains(Modifier::REVERSED));
    }
   ```
3. `spinner.rs`, en `mod glyph_tests`:
   ```rust
    #[test]
    fn no_color_glyph_has_no_fg() {
        let mut th = Theme::test();
        th.depth = crate::widgets::ColorDepth::NoColor;
        assert_eq!(Anim { frame: 0 }.glyph(BRAILLE, &th).style.fg, None);
    }
   ```

- [ ] **Step 5: Checks**

Run: `cargo test -p batten && cargo test -p batten --examples && cargo build -p batten --no-default-features && cargo test -p codeloom-tui && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS. Los tests existentes con `TrueColor` no cambian de resultado (`adapt` es la identidad).

- [ ] **Step 6: Deuda técnica**

En `docs/specs/2026-10-16-deuda-tecnica.md`:
1. Borrar la fila que empieza por `| 4 |`.
2. Añadir al final de la tabla la fila:
   `| 10 | `batten/src/widgets/fx.rs` | Los efectos de tachyonfx interpolan en RGB aunque el tema tenga menos profundidad. | Colores RGB transitorios en terminales de 256/16 colores o `NO_COLOR`. | Desactivar los presets con `NoColor` y aplicar el efecto `ansi256` de tachyonfx con profundidades reducidas. |`
3. Cambiar la línea `Resueltas el 2026-10-16: 1, 2 y 3 (...)` por `Resueltas el 2026-10-16: 1, 2, 3 (ver 2026-10-16-transcript-virtual-design.md) y 4 (ver docs/plans/2026-10-16-color-depth.md).`

- [ ] **Step 7: Commit**

```bash
git add crates/batten/src/widgets docs/specs/2026-10-16-deuda-tecnica.md
git commit -m "feat(batten): degradados, chips y botones respetan la profundidad de color"
```
