# Primitivas visuales de batten — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Añadir a batten siete primitivas visuales (`chip`, `rule`, `key_help`, `status`, `progress`, `panel`, `toast`) con un tipo compartido `Tone`, un token de tema `border` y una demo por pieza en el showcase.

**Architecture:** `state::Tone` (semántico, sin color) y `state::Toasts` (cola con caducidad por frames) en la capa pura; un fichero por pieza en `widgets/`, que devuelven `Span`/`Line` (salvo `panel`, que pinta sobre un `Rect` y devuelve el interior). `widgets::tone` resuelve tono → estilo/icono/etiqueta.

**Tech Stack:** Rust (edición 2021), `ratatui` 0.30, `unicode-width`.

**Spec:** `docs/specs/2026-10-15-batten-primitives-design.md`

## Global Constraints

- `state` no depende de `ratatui` ni `crossterm`; `batten` no depende de crossterm.
- Ningún widget se sale nunca del `width` recibido (columnas de display, `unicode-width`).
- Comentarios en español. Mensajes de commit: los indicados en cada tarea, tal cual.
- Cada commit pasa: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p batten`.
- **Entorno (Git Bash en Windows):** antes de cualquier `cargo`, ejecutar en el mismo comando
  `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe'`.
  Sin esto el enlazado falla con `/usr/bin/link: missing operand`; no es un error del código.
- No hacer `git push`. No tocar ficheros fuera de los listados en la tarea.

## Review Focus

- **Ancho 0** en cada pieza en línea: línea vacía, sin pánico (`text::truncate` devuelve `"…"` con `max == 0`, por eso cada pieza comprueba `w == 0` antes). Tests en Tasks 2-5 y 7.
- **Unicode ancho (CJK)**: el relleno se calcula con ancho de display, no con `len()`. Tests en Tasks 2 y 6.
- **`progress` con NaN o fuera de rango**: se acota, nunca pánico. Test en Task 5.
- **`panel` en áreas mínimas** (`0×0`, `1×5`): devuelve `Rect::default()` sin pintar. Test en Task 6 y `every_demo_renders_without_panicking` (Task 8).
- **`Toasts::tick` en cola vacía o con `ttl == 0`** no hace underflow. Test en Task 7.

---

## File Structure

```
crates/batten/src/state/tone.rs          # NUEVO: Tone
crates/batten/src/state/toast.rs         # NUEVO: Toast, Toasts
crates/batten/src/state/mod.rs           # + mods y re-exports
crates/batten/src/state/transcript.rs    # + StyleToken::Border
crates/batten/src/widgets/theme.rs       # + token border
crates/batten/src/widgets/tone.rs        # NUEVO: tone_style/tone_icon/tone_label
crates/batten/src/widgets/chip.rs        # NUEVO
crates/batten/src/widgets/rule.rs        # NUEVO
crates/batten/src/widgets/key_help.rs    # NUEVO
crates/batten/src/widgets/status.rs      # NUEVO
crates/batten/src/widgets/progress.rs    # NUEVO
crates/batten/src/widgets/panel.rs       # NUEVO
crates/batten/src/widgets/toast.rs       # NUEVO
crates/batten/src/widgets/mod.rs         # + mods y re-exports
crates/batten/examples/showcase/demos/{chip,rule,key_help,status,progress,panel,toast}.rs  # NUEVOS
crates/batten/examples/showcase/demos/mod.rs  # registrar demos
```

---

### Task 1: `Tone`, token `border` y `widgets::tone`

**Files:**
- Create: `crates/batten/src/state/tone.rs`, `crates/batten/src/widgets/tone.rs`
- Modify: `crates/batten/src/state/mod.rs`, `crates/batten/src/state/transcript.rs`, `crates/batten/src/widgets/theme.rs`, `crates/batten/src/widgets/mod.rs`

**Interfaces:**
- Consumes: `Theme`, `Styles`, `StyleToken` existentes.
- Produces: `batten::state::Tone { Neutral, Info, Success, Warning, Error }` con `Tone::ALL: [Tone; 5]`; `Styles.border`; `StyleToken::Border`; `batten::widgets::tone::{tone_style(Tone, &Theme) -> Style, tone_icon(Tone) -> char, tone_label(Tone) -> &'static str}`.

- [ ] **Step 1: Crear `crates/batten/src/state/tone.rs`**

```rust
//! Tono semántico compartido por las primitivas (sin color).

/// Intención de un elemento; el widget la resuelve a estilo, icono y etiqueta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tone {
    #[default]
    Neutral,
    Info,
    Success,
    Warning,
    Error,
}

impl Tone {
    /// Los cinco tonos, en orden (para demos y tests).
    pub const ALL: [Tone; 5] = [
        Tone::Neutral,
        Tone::Info,
        Tone::Success,
        Tone::Warning,
        Tone::Error,
    ];
}
```

- [ ] **Step 2: Registrar en `crates/batten/src/state/mod.rs`**

Añadir `pub mod tone;` justo después de `pub mod tool;`, y `pub use tone::Tone;` justo después de `pub use tool::{ToolItem, ToolStatus};`.

- [ ] **Step 3: Escribir el test del token `border` (falla)**

En `crates/batten/src/widgets/theme.rs`, dentro de `mod tests`, después del test `scrollbar_token_resolves_and_overrides`, añadir:

```rust
    #[test]
    fn border_token_resolves_and_overrides() {
        let th = Theme::test();
        assert_eq!(th.style(StyleToken::Border).fg, Some(Color::Indexed(240)));
        let th = Theme::from_toml("[styles]\nborder = \"magenta\"\n").unwrap();
        assert_eq!(th.style(StyleToken::Border).fg, Some(Color::Magenta));
    }
```

- [ ] **Step 4: Ejecutar y comprobar que falla**

Run: `cargo test -p batten border_token`
Expected: error de compilación `no variant ... named Border`.

- [ ] **Step 5: Implementar el token**

1. `crates/batten/src/state/transcript.rs`: en `enum StyleToken`, añadir `Border,` después de `Scrollbar,`.
2. `crates/batten/src/widgets/theme.rs`:
   - en `struct Styles`, después de `pub scrollbar: Style,` añadir `pub border: Style,`;
   - en `impl Default for Styles`, después de `scrollbar: Style::default().fg(Color::Indexed(240)),` añadir `border: Style::default().fg(Color::Indexed(240)),`;
   - en `set_named_style`, después de `StyleToken::Scrollbar => s.scrollbar,` añadir `StyleToken::Border => s.border,`;
   - en `apply_override`, después de `"scrollbar" => s.scrollbar = style,` añadir `"border" => s.border = style,`;
   - en `token_to_key`, después de `StyleToken::Scrollbar => "scrollbar",` añadir `StyleToken::Border => "border",`.

- [ ] **Step 6: Crear `crates/batten/src/widgets/tone.rs`**

```rust
//! Resolución de `Tone` a estilo, icono y etiqueta según el tema.

use super::theme::Theme;
use crate::state::Tone;
use ratatui::style::Style;

/// Estilo del tema asociado al tono.
pub fn tone_style(tone: Tone, theme: &Theme) -> Style {
    let s = &theme.styles;
    match tone {
        Tone::Neutral => s.muted,
        Tone::Info => s.accent,
        Tone::Success => s.success,
        Tone::Warning => s.warning,
        Tone::Error => s.error,
    }
}

/// Icono de una columna para `status`.
pub fn tone_icon(tone: Tone) -> char {
    match tone {
        Tone::Neutral => '•',
        Tone::Info => '●',
        Tone::Success => '✓',
        Tone::Warning => '!',
        Tone::Error => '×',
    }
}

/// Etiqueta corta para el chip de `toast`.
pub fn tone_label(tone: Tone) -> &'static str {
    match tone {
        Tone::Neutral => "NOTE",
        Tone::Info => "HEY!",
        Tone::Success => "OKAY!",
        Tone::Warning => "WARNING",
        Tone::Error => "ERROR",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_maps_to_theme_tokens() {
        let th = Theme::test();
        assert_eq!(tone_style(Tone::Error, &th), th.styles.error);
        assert_eq!(tone_style(Tone::Neutral, &th), th.styles.muted);
        assert_eq!(tone_style(Tone::Info, &th), th.styles.accent);
    }

    #[test]
    fn icons_and_labels_are_distinct() {
        let mut icons: Vec<char> = Tone::ALL.iter().map(|t| tone_icon(*t)).collect();
        icons.dedup();
        assert_eq!(icons.len(), 5);
        assert_eq!(tone_label(Tone::Success), "OKAY!");
    }
}
```

- [ ] **Step 7: Registrar en `crates/batten/src/widgets/mod.rs`**

Añadir `pub mod tone;` después de `pub mod tool_item;`.

- [ ] **Step 8: Ejecutar los checks**

Run: `cargo test -p batten && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS. Si `cargo fmt --check` falla, ejecutar `cargo fmt` y repetir.

- [ ] **Step 9: Commit**

```bash
git add crates/batten/src
git commit -m "feat(batten): tono semántico y token de borde para las primitivas"
```

---

### Task 2: `chip` y `rule`

**Files:**
- Create: `crates/batten/src/widgets/chip.rs`, `crates/batten/src/widgets/rule.rs`
- Modify: `crates/batten/src/widgets/mod.rs`

**Interfaces:**
- Consumes: `Tone`, `tone_style` (Task 1); `gradient::color_of`, `text::truncate` (existentes, `pub(crate)`).
- Produces: `batten::widgets::chip(text: &str, tone: Tone, theme: &Theme) -> Span<'static>`; `batten::widgets::rule(title: Option<&str>, width: u16, theme: &Theme) -> Line<'static>`.

- [ ] **Step 1: Crear `crates/batten/src/widgets/chip.rs` con tests**

```rust
//! Chip: etiqueta corta con el color del tono de fondo (` ERROR `).

use super::gradient::color_of;
use super::theme::Theme;
use super::tone::tone_style;
use crate::state::Tone;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

/// `" text "` con fondo del color del tono y texto oscuro en negrita.
pub fn chip(text: &str, tone: Tone, theme: &Theme) -> Span<'static> {
    let bg = color_of(tone_style(tone, theme));
    Span::styled(
        format!(" {text} "),
        Style::default()
            .bg(bg)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pads_text_and_uses_tone_as_background() {
        let th = Theme::test();
        let c = chip("ERROR", Tone::Error, &th);
        assert_eq!(c.content, " ERROR ");
        assert_eq!(c.style.bg, Some(Color::Red));
        assert_eq!(c.style.fg, Some(Color::Black));
    }
}
```

- [ ] **Step 2: Crear `crates/batten/src/widgets/rule.rs` con tests**

```rust
//! Regla horizontal, opcionalmente con título: `Archivos ──────`.

use super::text::truncate;
use super::theme::Theme;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

/// Título (`heading`) + espacio + `─` (`muted`) hasta `width`.
pub fn rule(title: Option<&str>, width: u16, theme: &Theme) -> Line<'static> {
    let w = width as usize;
    if w == 0 {
        return Line::default();
    }
    let s = &theme.styles;
    let Some(title) = title else {
        return Line::from(Span::styled("─".repeat(w), s.muted));
    };
    let tw = title.width();
    if tw + 1 >= w {
        return Line::from(Span::styled(truncate(title, w), s.heading));
    }
    Line::from(vec![
        Span::styled(title.to_string(), s.heading),
        Span::raw(" "),
        Span::styled("─".repeat(w - tw - 1), s.muted),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(l: &Line) -> String {
        l.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn plain_rule_fills_width() {
        assert_eq!(text(&rule(None, 5, &Theme::test())), "─────");
    }

    #[test]
    fn titled_rule_fills_rest() {
        assert_eq!(
            text(&rule(Some("Archivos"), 12, &Theme::test())),
            "Archivos ───"
        );
    }

    #[test]
    fn wide_chars_count_by_display_width() {
        let l = rule(Some("日本"), 6, &Theme::test());
        assert_eq!(text(&l), "日本 ─");
        assert_eq!(text(&l).width(), 6);
    }

    #[test]
    fn long_title_is_truncated_without_rule() {
        assert_eq!(text(&rule(Some("Archivos"), 4, &Theme::test())), "Arc…");
    }

    #[test]
    fn zero_width_is_empty() {
        assert_eq!(text(&rule(Some("x"), 0, &Theme::test())), "");
    }
}
```

- [ ] **Step 3: Registrar en `crates/batten/src/widgets/mod.rs`**

Añadir `pub mod chip;` después de `pub mod composer;` (orden alfabético de los `mod`), `pub mod rule;` después de `pub mod logo;`, y al final del bloque de `pub use`:

```rust
pub use chip::chip;
pub use rule::rule;
```

- [ ] **Step 4: Ejecutar los checks**

Run: `cargo test -p batten chip rule && cargo test -p batten && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (si `fmt --check` falla, `cargo fmt` y repetir).

- [ ] **Step 5: Commit**

```bash
git add crates/batten/src/widgets
git commit -m "feat(batten): chip por tono y regla horizontal con título"
```

---

### Task 3: `key_help`

**Files:**
- Create: `crates/batten/src/widgets/key_help.rs`
- Modify: `crates/batten/src/widgets/mod.rs`

**Interfaces:**
- Consumes: `Theme`.
- Produces: `batten::widgets::key_help(items: &[(&str, &str)], width: u16, theme: &Theme) -> Line<'static>`.

- [ ] **Step 1: Crear `crates/batten/src/widgets/key_help.rs`**

```rust
//! Fila de ayuda de teclas: `enter send · ↑↓ history · ctrl+c quit`.

use super::theme::Theme;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

/// Pinta `key desc` separados por ` · `. Si no cabe, quita items del final
/// y añade ` …`; si ni el primero cabe, solo `…`.
pub fn key_help(items: &[(&str, &str)], width: u16, theme: &Theme) -> Line<'static> {
    let w = width as usize;
    if items.is_empty() || w == 0 {
        return Line::default();
    }
    let s = &theme.styles;
    let total = |n: usize| -> usize {
        let body: usize = items[..n].iter().map(|(k, d)| k.width() + 1 + d.width()).sum();
        body + 3 * n.saturating_sub(1)
    };
    let fits = |n: usize| total(n) + if n < items.len() { 2 } else { 0 } <= w;
    let Some(n) = (1..=items.len()).rev().find(|&n| fits(n)) else {
        return Line::from(Span::styled("…", s.dim));
    };
    let mut spans = Vec::new();
    for (i, (k, d)) in items[..n].iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", s.dim));
        }
        spans.push(Span::styled(k.to_string(), s.heading));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(d.to_string(), s.muted));
    }
    if n < items.len() {
        spans.push(Span::styled(" …", s.dim));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ITEMS: [(&str, &str); 3] = [("enter", "send"), ("↑↓", "history"), ("ctrl+c", "quit")];

    fn text(l: &Line) -> String {
        l.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn everything_fits() {
        assert_eq!(
            text(&key_help(&ITEMS, 40, &Theme::test())),
            "enter send · ↑↓ history · ctrl+c quit"
        );
    }

    #[test]
    fn drops_items_from_the_end() {
        assert_eq!(
            text(&key_help(&ITEMS, 25, &Theme::test())),
            "enter send · ↑↓ history …"
        );
        assert_eq!(text(&key_help(&ITEMS, 24, &Theme::test())), "enter send …");
    }

    #[test]
    fn nothing_fits_gives_ellipsis() {
        assert_eq!(text(&key_help(&ITEMS, 5, &Theme::test())), "…");
    }

    #[test]
    fn empty_or_zero_width_is_empty() {
        assert_eq!(text(&key_help(&[], 40, &Theme::test())), "");
        assert_eq!(text(&key_help(&ITEMS, 0, &Theme::test())), "");
    }

    #[test]
    fn never_exceeds_width() {
        for w in 0..45u16 {
            assert!(text(&key_help(&ITEMS, w, &Theme::test())).width() <= w as usize);
        }
    }
}
```

- [ ] **Step 2: Registrar en `crates/batten/src/widgets/mod.rs`**

Añadir `pub mod key_help;` después de `pub mod file_changes;` y `pub use key_help::key_help;` al final de los `pub use`.

- [ ] **Step 3: Ejecutar los checks**

Run: `cargo test -p batten key_help && cargo test -p batten && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (si `fmt --check` falla, `cargo fmt` y repetir).

- [ ] **Step 4: Commit**

```bash
git add crates/batten/src/widgets
git commit -m "feat(batten): fila de ayuda de teclas que se recorta con elegancia"
```

---

### Task 4: `status`

**Files:**
- Create: `crates/batten/src/widgets/status.rs`
- Modify: `crates/batten/src/widgets/mod.rs`

**Interfaces:**
- Consumes: `Tone`, `tone_style`, `tone_icon` (Task 1); `text::truncate`.
- Produces: `batten::widgets::status(tone: Tone, title: &str, desc: &str, width: u16, theme: &Theme) -> Line<'static>`.

- [ ] **Step 1: Crear `crates/batten/src/widgets/status.rs`**

```rust
//! Mensaje de estado de una línea: `✓ Guardado  3 archivos`.

use super::text::truncate;
use super::theme::Theme;
use super::tone::{tone_icon, tone_style};
use crate::state::Tone;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

/// Icono del tono + título + descripción atenuada. Se recorta primero la
/// descripción; si no cabe, se omite; luego se trunca el título.
pub fn status(tone: Tone, title: &str, desc: &str, width: u16, theme: &Theme) -> Line<'static> {
    let w = width as usize;
    if w == 0 {
        return Line::default();
    }
    let s = &theme.styles;
    let icon = Span::styled(tone_icon(tone).to_string(), tone_style(tone, theme));
    if w <= 2 {
        return Line::from(icon);
    }
    let avail = w - 2;
    let mut spans = vec![icon, Span::raw(" ")];
    let tw = title.width();
    if tw >= avail || desc.is_empty() {
        spans.push(Span::styled(truncate(title, avail), s.default));
        return Line::from(spans);
    }
    spans.push(Span::styled(title.to_string(), s.default));
    let rest = avail - tw;
    if rest >= 3 {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(truncate(desc, rest - 2), s.muted));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(l: &Line) -> String {
        l.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn full_line() {
        let th = Theme::test();
        let l = status(Tone::Success, "Guardado", "3 archivos", 40, &th);
        assert_eq!(text(&l), "✓ Guardado  3 archivos");
        assert_eq!(l.spans[0].style, th.styles.success);
    }

    #[test]
    fn truncates_desc_first_then_drops_it() {
        let th = Theme::test();
        assert_eq!(text(&status(Tone::Success, "Guardado", "3 archivos", 15, &th)), "✓ Guardado  3 …");
        assert_eq!(text(&status(Tone::Success, "Guardado", "3 archivos", 11, &th)), "✓ Guardado");
    }

    #[test]
    fn truncates_title_last() {
        assert_eq!(
            text(&status(Tone::Error, "Guardado", "x", 6, &Theme::test())),
            "× Gua…"
        );
    }

    #[test]
    fn empty_desc_has_no_separator() {
        assert_eq!(text(&status(Tone::Info, "Hola", "", 40, &Theme::test())), "● Hola");
    }

    #[test]
    fn never_exceeds_width() {
        for w in 0..30u16 {
            let l = status(Tone::Warning, "Guardado", "3 archivos", w, &Theme::test());
            assert!(text(&l).width() <= w as usize);
        }
    }
}
```

- [ ] **Step 2: Registrar en `crates/batten/src/widgets/mod.rs`**

Añadir `pub mod status;` después de `pub mod spinner;` y `pub use status::status;` al final de los `pub use`.

- [ ] **Step 3: Ejecutar los checks**

Run: `cargo test -p batten status && cargo test -p batten && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (si `fmt --check` falla, `cargo fmt` y repetir).

- [ ] **Step 4: Commit**

```bash
git add crates/batten/src/widgets
git commit -m "feat(batten): mensaje de estado con icono por tono"
```

---

### Task 5: `progress`

**Files:**
- Create: `crates/batten/src/widgets/progress.rs`
- Modify: `crates/batten/src/widgets/mod.rs`

**Interfaces:**
- Consumes: `gradient::{color_of, lerp_color}`, `text::truncate`.
- Produces: `batten::widgets::progress(ratio: f32, width: u16, theme: &Theme) -> Line<'static>`.

- [ ] **Step 1: Crear `crates/batten/src/widgets/progress.rs`**

```rust
//! Barra de progreso determinada: `━━━━━━━━────────  42%`.

use super::gradient::{color_of, lerp_color};
use super::text::truncate;
use super::theme::Theme;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

/// Relleno `━` con degradado `accent_from → accent_to`, resto `─` y porcentaje
/// en 5 columnas. `ratio` se acota a `[0, 1]`; NaN cuenta como 0.
pub fn progress(ratio: f32, width: u16, theme: &Theme) -> Line<'static> {
    let w = width as usize;
    if w == 0 {
        return Line::default();
    }
    let s = &theme.styles;
    let r = if ratio.is_nan() { 0.0 } else { ratio.clamp(0.0, 1.0) };
    let pct = (r * 100.0).round() as u32;
    if w < 6 {
        return Line::from(Span::styled(truncate(&format!("{pct}%"), w), s.muted));
    }
    let bar = w - 5;
    let filled = (r * bar as f32).round() as usize;
    let from = color_of(s.accent_from);
    let to = color_of(s.accent_to);
    let mut spans: Vec<Span<'static>> = (0..filled)
        .map(|i| {
            let t = if bar > 1 { i as f32 / (bar - 1) as f32 } else { 0.0 };
            Span::styled("━", Style::default().fg(lerp_color(from, to, t)))
        })
        .collect();
    spans.push(Span::styled("─".repeat(bar - filled), s.muted));
    spans.push(Span::styled(format!(" {pct:>3}%"), s.muted));
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    fn text(l: &Line) -> String {
        l.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn partial_bar() {
        let l = progress(0.42, 25, &Theme::test());
        assert_eq!(text(&l), format!("{}{}  42%", "━".repeat(8), "─".repeat(12)));
        assert_eq!(text(&l).width(), 25);
    }

    #[test]
    fn first_cell_uses_accent_from() {
        let th = Theme::test();
        let l = progress(1.0, 10, &th);
        assert_eq!(l.spans[0].style.fg, Some(color_of(th.styles.accent_from)));
    }

    #[test]
    fn clamps_and_handles_nan() {
        let th = Theme::test();
        assert_eq!(text(&progress(1.5, 10, &th)), "━━━━━ 100%");
        assert_eq!(text(&progress(f32::NAN, 10, &th)), "─────   0%");
        assert_eq!(text(&progress(-3.0, 10, &th)), "─────   0%");
    }

    #[test]
    fn narrow_shows_only_percent() {
        assert_eq!(text(&progress(0.5, 4, &Theme::test())), "50%");
        assert_eq!(text(&progress(0.5, 0, &Theme::test())), "");
    }

    #[test]
    fn never_exceeds_width() {
        for w in 0..30u16 {
            assert!(text(&progress(0.37, w, &Theme::test())).width() <= w as usize);
        }
    }
}
```

- [ ] **Step 2: Registrar en `crates/batten/src/widgets/mod.rs`**

Añadir `pub mod progress;` después de `pub mod logo;` (antes de `pub mod rule;`) y `pub use progress::progress;` al final de los `pub use`.

- [ ] **Step 3: Ejecutar los checks**

Run: `cargo test -p batten progress && cargo test -p batten && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (si `fmt --check` falla, `cargo fmt` y repetir).

- [ ] **Step 4: Commit**

```bash
git add crates/batten/src/widgets
git commit -m "feat(batten): barra de progreso con degradado de acento"
```

---

### Task 6: `panel`

**Files:**
- Create: `crates/batten/src/widgets/panel.rs`
- Modify: `crates/batten/src/widgets/mod.rs`

**Interfaces:**
- Consumes: `Tone`, `tone_style` (Task 1); `Styles.border` (Task 1); `gradient::{color_of, lerp_color}`, `text::truncate`; `theme.glyphs.field` (`╱`).
- Produces: `batten::widgets::Panel<'a> { title: &'a str, info: &'a str, tone: Tone }`; `batten::widgets::render_panel(frame: &mut Frame, area: Rect, panel: &Panel, theme: &Theme) -> Rect` (área interior).

- [ ] **Step 1: Crear `crates/batten/src/widgets/panel.rs`**

```rust
//! Panel con borde redondeado y título `╭ Título ╱╱╱╱ info ╮`.

use super::gradient::{color_of, lerp_color};
use super::text::truncate;
use super::theme::Theme;
use super::tone::tone_style;
use crate::state::Tone;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

/// Contenido de la cabecera de un panel.
#[derive(Debug, Clone, Copy, Default)]
pub struct Panel<'a> {
    pub title: &'a str,
    pub info: &'a str,
    pub tone: Tone,
}

/// Pinta el marco y la cabecera; devuelve el área interior. Con menos de
/// 2×2 no pinta nada y devuelve `Rect::default()`.
pub fn render(frame: &mut Frame, area: Rect, panel: &Panel, theme: &Theme) -> Rect {
    if area.width < 2 || area.height < 2 {
        return Rect::default();
    }
    let border = if panel.tone == Tone::Neutral {
        theme.styles.border
    } else {
        tone_style(panel.tone, theme)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let top = Rect {
        x: area.x + 1,
        y: area.y,
        width: area.width - 2,
        height: 1,
    };
    frame.render_widget(Paragraph::new(header(panel, top.width as usize, theme)), top);
    inner
}

/// Cabecera entre esquinas: se descarta primero `info`, luego el relleno, y
/// por último se trunca el título.
fn header(panel: &Panel, w: usize, theme: &Theme) -> Line<'static> {
    if w == 0 {
        return Line::default();
    }
    let s = &theme.styles;
    let title = format!(" {} ", panel.title);
    let tw = title.width();
    if tw >= w {
        return Line::from(Span::styled(truncate(&title, w), s.heading));
    }
    let info = if panel.info.is_empty() {
        String::new()
    } else {
        format!(" {} ", panel.info)
    };
    let iw = if tw + info.width() < w { info.width() } else { 0 };
    let fill = w - tw - iw;
    let from = color_of(s.accent_from);
    let to = color_of(s.accent_to);
    let mut spans = vec![Span::styled(title, s.heading)];
    for i in 0..fill {
        let t = if fill > 1 { i as f32 / (fill - 1) as f32 } else { 0.0 };
        spans.push(Span::styled(
            theme.glyphs.field.to_string(),
            Style::default().fg(lerp_color(from, to, t)),
        ));
    }
    if iw > 0 {
        spans.push(Span::styled(info, s.muted));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;
    use ratatui::Terminal;

    fn draw(w: u16, h: u16, panel: Panel) -> (Terminal<TestBackend>, Rect) {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        let mut inner = Rect::default();
        term.draw(|f| inner = render(f, f.area(), &panel, &Theme::test()))
            .unwrap();
        (term, inner)
    }

    fn row(term: &Terminal<TestBackend>, y: u16) -> String {
        let buf = term.backend().buffer();
        (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect()
    }

    const P: Panel = Panel {
        title: "Permisos",
        info: "bash",
        tone: Tone::Neutral,
    };

    #[test]
    fn header_has_title_fill_and_info() {
        let (term, inner) = draw(30, 4, P);
        assert_eq!(row(&term, 0), format!("╭ Permisos {} bash ╮", "╱".repeat(12)));
        assert!(row(&term, 3).starts_with('╰') && row(&term, 3).ends_with('╯'));
        assert_eq!(inner, Rect::new(1, 1, 28, 2));
    }

    #[test]
    fn narrow_drops_info_and_fill() {
        let (term, _) = draw(12, 3, P);
        assert_eq!(row(&term, 0), "╭ Permisos ╮");
    }

    #[test]
    fn tone_colors_the_border() {
        let (term, _) = draw(20, 3, Panel { tone: Tone::Error, ..P });
        assert_eq!(term.backend().buffer()[(0, 1)].fg, Color::Red);
    }

    #[test]
    fn wide_title_keeps_frame_width() {
        // Las celdas de continuación de un carácter ancho no son fiables como
        // texto; basta con que las esquinas sigan en su sitio.
        let (term, _) = draw(14, 3, Panel { title: "日本語", info: "", tone: Tone::Neutral });
        let top = row(&term, 0);
        assert!(top.starts_with('╭') && top.ends_with('╮'));
        assert!(top.contains('日'));
    }

    #[test]
    fn tiny_area_draws_nothing() {
        for (w, h) in [(0, 0), (1, 5), (5, 1)] {
            let (_, inner) = draw(w, h, P);
            assert_eq!(inner, Rect::default());
        }
    }
}
```

- [ ] **Step 2: Registrar en `crates/batten/src/widgets/mod.rs`**

Añadir `pub mod panel;` después de `pub mod logo;` (antes de `pub mod progress;`) y al final de los `pub use`:

```rust
pub use panel::{render as render_panel, Panel};
```

- [ ] **Step 3: Ejecutar los checks**

Run: `cargo test -p batten panel && cargo test -p batten && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (si `fmt --check` falla, `cargo fmt` y repetir).

- [ ] **Step 4: Commit**

```bash
git add crates/batten/src/widgets
git commit -m "feat(batten): panel con borde redondeado y cabecera en degradado"
```

---

### Task 7: `toast` (estado y vista)

**Files:**
- Create: `crates/batten/src/state/toast.rs`, `crates/batten/src/widgets/toast.rs`
- Modify: `crates/batten/src/state/mod.rs`, `crates/batten/src/widgets/mod.rs`

**Interfaces:**
- Consumes: `Tone` (Task 1); `chip` (Task 2); `tone_label` (Task 1); `text::truncate`.
- Produces: `batten::state::{Toast { tone, text, ttl }, Toasts}` con `Toasts::new()`, `push(&mut self, Tone, impl Into<String>, u32)`, `current(&self) -> Option<&Toast>`, `tick(&mut self) -> bool`; `batten::widgets::render_toast(toasts: &Toasts, width: u16, theme: &Theme) -> Option<Line<'static>>`.

- [ ] **Step 1: Crear `crates/batten/src/state/toast.rs`**

```rust
//! Avisos temporales (toasts): cola FIFO con caducidad por frames contados.

use super::tone::Tone;
use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq)]
pub struct Toast {
    pub tone: Tone,
    pub text: String,
    /// Frames que le quedan visible.
    pub ttl: u32,
}

/// Cola de avisos; solo se muestra el primero.
#[derive(Debug, Default)]
pub struct Toasts {
    queue: VecDeque<Toast>,
}

impl Toasts {
    pub fn new() -> Self {
        Self::default()
    }

    /// Encola un aviso. Con `ttl_frames == 0` no hace nada.
    pub fn push(&mut self, tone: Tone, text: impl Into<String>, ttl_frames: u32) {
        if ttl_frames == 0 {
            return;
        }
        self.queue.push_back(Toast {
            tone,
            text: text.into(),
            ttl: ttl_frames,
        });
    }

    pub fn current(&self) -> Option<&Toast> {
        self.queue.front()
    }

    /// Consume un frame del aviso visible; al caducar lo saca de la cola.
    /// Devuelve `true` si cambió lo que se ve.
    pub fn tick(&mut self) -> bool {
        let Some(t) = self.queue.front_mut() else {
            return false;
        };
        t.ttl -= 1;
        if t.ttl == 0 {
            self.queue.pop_front();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifo_with_exact_expiry() {
        let mut t = Toasts::new();
        t.push(Tone::Success, "uno", 2);
        t.push(Tone::Error, "dos", 1);
        assert_eq!(t.current().unwrap().text, "uno");
        assert!(!t.tick());
        assert!(t.tick());
        assert_eq!(t.current().unwrap().text, "dos");
        assert!(t.tick());
        assert!(t.current().is_none());
    }

    #[test]
    fn zero_ttl_is_ignored_and_empty_tick_is_noop() {
        let mut t = Toasts::new();
        t.push(Tone::Info, "nada", 0);
        assert!(t.current().is_none());
        assert!(!t.tick());
    }
}
```

- [ ] **Step 2: Registrar en `crates/batten/src/state/mod.rs`**

Añadir `pub mod toast;` justo antes de `pub mod tone;`, y `pub use toast::{Toast, Toasts};` justo antes de `pub use tone::Tone;`.

- [ ] **Step 3: Crear `crates/batten/src/widgets/toast.rs`**

```rust
//! Vista del aviso visible: `[ OKAY! ] Copiado al portapapeles`.

use super::chip::chip;
use super::text::truncate;
use super::theme::Theme;
use super::tone::tone_label;
use crate::state::Toasts;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

/// Chip con la etiqueta del tono + texto, truncado a `width`. `None` si no
/// hay aviso.
pub fn render(toasts: &Toasts, width: u16, theme: &Theme) -> Option<Line<'static>> {
    let t = toasts.current()?;
    let w = width as usize;
    if w == 0 {
        return Some(Line::default());
    }
    let s = &theme.styles;
    let c = chip(tone_label(t.tone), t.tone, theme);
    let cw = c.content.width();
    if w <= cw + 1 {
        return Some(Line::from(Span::styled(truncate(&t.text, w), s.default)));
    }
    Some(Line::from(vec![
        c,
        Span::raw(" "),
        Span::styled(truncate(&t.text, w - cw - 1), s.default),
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Tone;
    use ratatui::style::Color;

    fn text(l: &Line) -> String {
        l.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn shows_chip_and_text() {
        let mut t = Toasts::new();
        t.push(Tone::Success, "Copiado", 10);
        let l = render(&t, 40, &Theme::test()).unwrap();
        assert_eq!(text(&l), " OKAY!  Copiado");
        assert_eq!(l.spans[0].style.bg, Some(Color::Green));
    }

    #[test]
    fn truncates_text_to_width() {
        let mut t = Toasts::new();
        t.push(Tone::Success, "Copiado", 10);
        assert_eq!(text(&render(&t, 10, &Theme::test()).unwrap()), " OKAY!  C…");
    }

    #[test]
    fn none_without_toast() {
        assert!(render(&Toasts::new(), 40, &Theme::test()).is_none());
    }

    #[test]
    fn never_exceeds_width() {
        let mut t = Toasts::new();
        t.push(Tone::Warning, "Algo largo que no cabe", 10);
        for w in 0..30u16 {
            let l = render(&t, w, &Theme::test()).unwrap();
            assert!(text(&l).width() <= w as usize);
        }
    }
}
```

- [ ] **Step 4: Registrar en `crates/batten/src/widgets/mod.rs`**

Añadir `pub mod toast;` justo antes de `pub mod tone;`, y `pub use toast::render as render_toast;` al final de los `pub use`.

- [ ] **Step 5: Ejecutar los checks**

Run: `cargo test -p batten toast && cargo test -p batten && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (si `fmt --check` falla, `cargo fmt` y repetir).

- [ ] **Step 6: Commit**

```bash
git add crates/batten/src
git commit -m "feat(batten): avisos temporales con caducidad por frames"
```

---

### Task 8: Demos en el showcase

**Files:**
- Create: `crates/batten/examples/showcase/demos/{chip,rule,key_help,status,progress,panel,toast}.rs`
- Modify: `crates/batten/examples/showcase/demos/mod.rs`

**Interfaces:**
- Consumes: todo lo producido en Tasks 1-7; el trait `crate::demo::Demo` (`title`, `hint`, `render`, `tick(&mut self, &Anim)`, `handle_key(&mut self, KeyCode) -> bool`).
- Produces: siete demos registradas en `catalog()`.

- [ ] **Step 1: Crear `demos/chip.rs`**

```rust
//! Demo de chips: uno por tono.

use crate::demo::Demo;
use batten::state::Tone;
use batten::widgets::{chip, Theme};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct ChipDemo;

impl ChipDemo {
    pub fn new() -> Self {
        Self
    }
}

impl Demo for ChipDemo {
    fn title(&self) -> &str {
        "chip"
    }

    fn hint(&self) -> &str {
        "un chip por tono"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let labels = ["NOTE", "INFO", "DONE", "WARN", "ERROR"];
        let mut spans = Vec::new();
        for (tone, label) in Tone::ALL.into_iter().zip(labels) {
            spans.push(chip(label, tone, theme));
            spans.push(Span::raw(" "));
        }
        f.render_widget(Paragraph::new(Line::from(spans)), area);
    }
}
```

- [ ] **Step 2: Crear `demos/rule.rs`**

```rust
//! Demo de reglas: sin título, con título y con título ancho.

use crate::demo::Demo;
use batten::widgets::{rule, Theme};
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct RuleDemo;

impl RuleDemo {
    pub fn new() -> Self {
        Self
    }
}

impl Demo for RuleDemo {
    fn title(&self) -> &str {
        "rule"
    }

    fn hint(&self) -> &str {
        "reglas al ancho del área"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let w = area.width;
        let lines = vec![
            rule(None, w, theme),
            rule(Some("Archivos"), w, theme),
            rule(Some("日本語のタイトル"), w, theme),
        ];
        f.render_widget(Paragraph::new(lines), area);
    }
}
```

- [ ] **Step 3: Crear `demos/key_help.rs`**

```rust
//! Demo de la fila de ayuda a ancho completo y a la mitad.

use crate::demo::Demo;
use batten::widgets::{key_help, Theme};
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const ITEMS: [(&str, &str); 4] = [
    ("enter", "send"),
    ("ctrl+j", "newline"),
    ("↑↓", "history"),
    ("ctrl+c", "quit"),
];

pub struct KeyHelpDemo;

impl KeyHelpDemo {
    pub fn new() -> Self {
        Self
    }
}

impl Demo for KeyHelpDemo {
    fn title(&self) -> &str {
        "key_help"
    }

    fn hint(&self) -> &str {
        "ancho completo y mitad"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let lines = vec![
            key_help(&ITEMS, area.width, theme),
            key_help(&ITEMS, area.width / 2, theme),
        ];
        f.render_widget(Paragraph::new(lines), area);
    }
}
```

- [ ] **Step 4: Crear `demos/status.rs`**

```rust
//! Demo de mensajes de estado, uno por tono.

use crate::demo::Demo;
use batten::state::Tone;
use batten::widgets::{status, Theme};
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct StatusDemo;

impl StatusDemo {
    pub fn new() -> Self {
        Self
    }
}

impl Demo for StatusDemo {
    fn title(&self) -> &str {
        "status"
    }

    fn hint(&self) -> &str {
        "un mensaje por tono"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let rows = [
            ("Listo", "esperando instrucciones"),
            ("Conectado", "deepseek via openrouter"),
            ("Guardado", "3 archivos"),
            ("Contexto al 85%", "considera compactar"),
            ("Falló el build", "error[E0425] en main.rs"),
        ];
        let lines: Vec<_> = Tone::ALL
            .into_iter()
            .zip(rows)
            .map(|(tone, (title, desc))| status(tone, title, desc, area.width, theme))
            .collect();
        f.render_widget(Paragraph::new(lines), area);
    }
}
```

- [ ] **Step 5: Crear `demos/progress.rs`**

```rust
//! Demo de la barra de progreso, animada por frames.

use crate::demo::Demo;
use batten::widgets::{progress, Anim, Theme};
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct ProgressDemo {
    frame: u32,
}

impl ProgressDemo {
    pub fn new() -> Self {
        Self { frame: 0 }
    }
}

impl Demo for ProgressDemo {
    fn title(&self) -> &str {
        "progress"
    }

    fn hint(&self) -> &str {
        "avanza sola"
    }

    fn tick(&mut self, _anim: &Anim) {
        self.frame = self.frame.wrapping_add(1);
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let ratio = (self.frame % 101) as f32 / 100.0;
        let lines = vec![
            progress(ratio, area.width, theme),
            progress(0.0, area.width, theme),
            progress(1.0, area.width, theme),
        ];
        f.render_widget(Paragraph::new(lines), area);
    }
}
```

- [ ] **Step 6: Crear `demos/panel.rs`**

```rust
//! Demo de paneles, uno por tono.

use crate::demo::Demo;
use batten::state::Tone;
use batten::widgets::{render_panel, Panel, Theme};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct PanelDemo;

impl PanelDemo {
    pub fn new() -> Self {
        Self
    }
}

impl Demo for PanelDemo {
    fn title(&self) -> &str {
        "panel"
    }

    fn hint(&self) -> &str {
        "un panel por tono"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let rows = Layout::vertical([Constraint::Length(3); 5]).split(area);
        for (tone, row) in Tone::ALL.into_iter().zip(rows.iter()) {
            let panel = Panel {
                title: "Permisos",
                info: "bash",
                tone,
            };
            let inner = render_panel(f, *row, &panel, theme);
            f.render_widget(Paragraph::new(format!("{tone:?}")), inner);
        }
    }
}
```

- [ ] **Step 7: Crear `demos/toast.rs`**

```rust
//! Demo de avisos: `t` lanza uno (rota de tono), caduca solo.

use crate::demo::Demo;
use batten::state::{Toasts, Tone};
use batten::widgets::{render_toast, Anim, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct ToastDemo {
    toasts: Toasts,
    next: usize,
}

impl ToastDemo {
    pub fn new() -> Self {
        Self {
            toasts: Toasts::new(),
            next: 0,
        }
    }
}

impl Demo for ToastDemo {
    fn title(&self) -> &str {
        "toast"
    }

    fn hint(&self) -> &str {
        "t: lanzar aviso"
    }

    fn tick(&mut self, _anim: &Anim) {
        self.toasts.tick();
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        if code != KeyCode::Char('t') {
            return false;
        }
        let tone = Tone::ALL[self.next % Tone::ALL.len()];
        self.next += 1;
        self.toasts
            .push(tone, format!("Aviso #{} ({tone:?})", self.next), 40);
        true
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let mut lines = vec![Line::from("pulsa t; cada aviso dura 40 frames (~2 s)")];
        lines.extend(render_toast(&self.toasts, area.width, theme));
        f.render_widget(Paragraph::new(lines), area);
    }
}
```

- [ ] **Step 8: Registrar en `demos/mod.rs`**

Añadir los `mod` en orden alfabético con los existentes (`chip`, `key_help`, `panel`, `progress`, `rule`, `status`, `toast`), y al final del `vec![...]` de `catalog()`, después de `Box::new(file_changes::FileChangesDemo::new()),`:

```rust
        Box::new(chip::ChipDemo::new()),
        Box::new(rule::RuleDemo::new()),
        Box::new(key_help::KeyHelpDemo::new()),
        Box::new(status::StatusDemo::new()),
        Box::new(progress::ProgressDemo::new()),
        Box::new(panel::PanelDemo::new()),
        Box::new(toast::ToastDemo::new()),
```

- [ ] **Step 9: Ejecutar los checks**

Run: `cargo test -p batten --examples && cargo test -p batten && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS, incluidos `catalog_titles_are_unique` y `every_demo_renders_without_panicking`. Si clippy pide `Default` (`new_without_default`), añadir `#[derive(Default)]` al struct del demo afectado; no cambiar nada más.

- [ ] **Step 10: Commit**

```bash
git add crates/batten/examples/showcase/demos
git commit -m "feat(batten): demos de las primitivas en el showcase"
```

---

### Task 9: Verificación final

**Files:** ninguno.

- [ ] **Step 1: Checks del workspace**

Run: `cargo build && cargo test && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 2: Verificación manual (la hace el usuario)**

Run: `cargo run -p batten --example showcase`
Expected: en el índice aparecen `chip`, `rule`, `key_help`, `status`, `progress`, `panel`, `toast`; la barra avanza sola; en `toast`, `t` lanza avisos que caducan.

---

## Criterios de aceptación

- [ ] Las siete primitivas existen con la API del spec y sus tests pasan.
- [ ] Ninguna pieza en línea supera el ancho recibido (tests `never_exceeds_width`).
- [ ] `border` es un token del tema sobrescribible desde `theme.toml`.
- [ ] El showcase muestra una demo por primitiva.
- [ ] Todos los checks pasan.
