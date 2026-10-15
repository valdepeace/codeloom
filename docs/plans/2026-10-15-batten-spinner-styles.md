# Estilos de spinner y barra suave — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Añadir a batten seis spinners de una celda, una barra de progreso con precisión de octavos y una demo que los muestre.

**Architecture:** Constantes de frames en `widgets::spinner` + `Anim::glyph`; `progress_smooth` junto a `progress` en `widgets::progress`; demo `spinners` en el showcase. Inspirado en el catálogo de `dotmax` (ideas, no código).

**Tech Stack:** Rust (edición 2021), `ratatui` 0.30, `unicode-width`.

**Spec:** diseño acordado en conversación (2026-10-15): tarea acotada, sin spec aparte.

## Global Constraints

- Comentarios en español. Mensajes de commit: los indicados, tal cual.
- Ningún widget se sale del `width` recibido.
- Cada commit pasa: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p batten` (y `--examples` en la Task 3).
- **Entorno (Git Bash en Windows):** antes de cualquier `cargo`, en el mismo comando:
  `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe'`.
- No hacer `git push`. No tocar ficheros fuera de los listados.

## Review Focus

- `glyph` con lista de frames vacía no entra en pánico (división por cero en `%`). Test en Task 1.
- Todos los frames ocupan exactamente 1 columna. Test en Task 1.
- `progress_smooth` al 100 % no añade bloque parcial ni se pasa del ancho. Tests en Task 2.

---

### Task 1: spinners de una celda

**Files:**
- Modify: `crates/batten/src/widgets/spinner.rs`

**Interfaces:**
- Produces: `batten::widgets::spinner::{BRAILLE, DOT_RING, ARC, BOUNCE, PULSE, BARS}: &'static [&'static str]`; `Anim::glyph(&self, frames: &[&'static str], theme: &Theme) -> Span<'static>`.

- [ ] **Step 1: Añadir las constantes**

En `crates/batten/src/widgets/spinner.rs`, justo después de la línea `const RUNES: &[u8] = b"0123456789abcdef#@$%&*+~";`, añadir:

```rust

/// Spinners de una celda (frames en orden). Se pintan con `Anim::glyph`.
pub const BRAILLE: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
pub const DOT_RING: &[&str] = &["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];
pub const ARC: &[&str] = &["◜", "◠", "◝", "◞", "◡", "◟"];
pub const BOUNCE: &[&str] = &["⠁", "⠂", "⠄", "⡀", "⢀", "⠠", "⠐", "⠈"];
pub const PULSE: &[&str] = &["·", "•", "●", "•"];
pub const BARS: &[&str] = &[
    "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█", "▇", "▆", "▅", "▄", "▃", "▂",
];

/// Ticks del reloj (50 ms) por frame de un spinner de una celda.
const TICKS_PER_GLYPH: u64 = 2;
```

- [ ] **Step 2: Añadir `glyph` a `impl Anim`**

Dentro de `impl Anim { ... }`, después del método `render`, añadir:

```rust

    /// Frame actual de un spinner de una celda. El color oscila entre
    /// `accent_from` y `accent_to` con un periodo de ~2 s. Sin frames, vacío.
    pub fn glyph(&self, frames: &[&'static str], theme: &Theme) -> Span<'static> {
        if frames.is_empty() {
            return Span::raw("");
        }
        let step = self.frame / TICKS_PER_GLYPH;
        let shown = frames[(step % frames.len() as u64) as usize];
        let phase = (step % 20) as f32 / 10.0;
        let t = if phase <= 1.0 { phase } else { 2.0 - phase };
        let from = color_of(theme.styles.accent_from);
        let to = color_of(theme.styles.accent_to);
        Span::styled(shown, Style::default().fg(lerp_color(from, to, t)))
    }
```

- [ ] **Step 3: Añadir tests al final del fichero**

```rust

#[cfg(test)]
mod glyph_tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn advances_every_two_ticks_and_wraps() {
        let th = Theme::test();
        assert_eq!(Anim { frame: 0 }.glyph(BRAILLE, &th).content, "⠋");
        assert_eq!(Anim { frame: 1 }.glyph(BRAILLE, &th).content, "⠋");
        assert_eq!(Anim { frame: 2 }.glyph(BRAILLE, &th).content, "⠙");
        assert_eq!(Anim { frame: 20 }.glyph(BRAILLE, &th).content, "⠋");
    }

    #[test]
    fn starts_with_accent_from() {
        let th = Theme::test();
        let g = Anim { frame: 0 }.glyph(PULSE, &th);
        assert_eq!(g.style.fg, Some(color_of(th.styles.accent_from)));
    }

    #[test]
    fn empty_frames_are_empty() {
        assert_eq!(Anim::new().glyph(&[], &Theme::test()).content, "");
    }

    #[test]
    fn every_frame_is_one_column() {
        for frames in [BRAILLE, DOT_RING, ARC, BOUNCE, PULSE, BARS] {
            for f in frames {
                assert_eq!(f.width(), 1, "frame {f:?}");
            }
        }
    }
}
```

- [ ] **Step 4: Checks**

Run: `cargo test -p batten glyph_tests && cargo test -p batten && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (si `fmt --check` falla, `cargo fmt` y repetir).

- [ ] **Step 5: Commit**

```bash
git add crates/batten/src/widgets/spinner.rs
git commit -m "feat(batten): seis spinners de una celda con color oscilante"
```

---

### Task 2: `progress_smooth`

**Files:**
- Modify: `crates/batten/src/widgets/progress.rs`, `crates/batten/src/widgets/mod.rs`

**Interfaces:**
- Produces: `batten::widgets::progress_smooth(ratio: f32, width: u16, theme: &Theme) -> Line<'static>`.

- [ ] **Step 1: Añadir la función**

En `crates/batten/src/widgets/progress.rs`, justo antes de la línea `#[cfg(test)]` (la del módulo de tests existente), añadir:

```rust
/// Bloques parciales por octavos (índice = octavos ocupados en la celda).
const EIGHTHS: [&str; 8] = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];

/// Como `progress`, pero con `█` y la última celda partida en octavos:
/// avanza por fracciones de celda. Hueco en blanco; porcentaje en 5 columnas.
pub fn progress_smooth(ratio: f32, width: u16, theme: &Theme) -> Line<'static> {
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
    let eighths = (r * (bar * 8) as f32).round() as usize;
    let (full, part) = (eighths / 8, eighths % 8);
    let from = color_of(s.accent_from);
    let to = color_of(s.accent_to);
    let color = |i: usize| {
        let t = if bar > 1 { i as f32 / (bar - 1) as f32 } else { 0.0 };
        Style::default().fg(lerp_color(from, to, t))
    };
    let mut spans: Vec<Span<'static>> = (0..full).map(|i| Span::styled("█", color(i))).collect();
    let mut used = full;
    if part > 0 {
        spans.push(Span::styled(EIGHTHS[part], color(full)));
        used += 1;
    }
    spans.push(Span::raw(" ".repeat(bar - used)));
    spans.push(Span::styled(format!(" {pct:>3}%"), s.muted));
    Line::from(spans)
}

```

- [ ] **Step 2: Añadir tests al final del fichero**

```rust

#[cfg(test)]
mod smooth_tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    fn text(l: &Line) -> String {
        l.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn half_is_whole_cells() {
        let l = progress_smooth(0.5, 13, &Theme::test());
        assert_eq!(text(&l), "████      50%");
    }

    #[test]
    fn small_ratio_uses_partial_block() {
        // barra de 8 celdas = 64 octavos; 10 % → 6 octavos → "▊"
        let l = progress_smooth(0.1, 13, &Theme::test());
        assert_eq!(text(&l), format!("▊{}  10%", " ".repeat(7)));
    }

    #[test]
    fn full_and_nan() {
        let th = Theme::test();
        assert_eq!(text(&progress_smooth(1.5, 10, &th)), "█████ 100%");
        assert_eq!(
            text(&progress_smooth(f32::NAN, 10, &th)),
            format!("{}   0%", " ".repeat(5))
        );
    }

    #[test]
    fn narrow_shows_only_percent() {
        assert_eq!(text(&progress_smooth(0.5, 4, &Theme::test())), "50%");
        assert_eq!(text(&progress_smooth(0.5, 0, &Theme::test())), "");
    }

    #[test]
    fn never_exceeds_width() {
        for w in 0..30u16 {
            for r in [0.0, 0.013, 0.37, 0.999, 1.0] {
                let l = progress_smooth(r, w, &Theme::test());
                assert!(text(&l).width() <= w as usize, "w={w} r={r}");
            }
        }
    }
}
```

- [ ] **Step 3: Exportar**

En `crates/batten/src/widgets/mod.rs`, cambiar `pub use progress::progress;` por `pub use progress::{progress, progress_smooth};`.

- [ ] **Step 4: Checks**

Run: `cargo test -p batten smooth_tests && cargo test -p batten && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (si `fmt --check` falla, `cargo fmt` y repetir).

- [ ] **Step 5: Commit**

```bash
git add crates/batten/src/widgets
git commit -m "feat(batten): barra de progreso suave con precisión de octavos"
```

---

### Task 3: demo `spinners`

**Files:**
- Create: `crates/batten/examples/showcase/demos/spinners.rs`
- Modify: `crates/batten/examples/showcase/demos/mod.rs`

- [ ] **Step 1: Crear `demos/spinners.rs`**

```rust
//! Demo de los spinners de una celda y de las dos barras de progreso.

use crate::demo::Demo;
use batten::widgets::spinner::{ARC, BARS, BOUNCE, BRAILLE, DOT_RING, PULSE};
use batten::widgets::{progress, progress_smooth, Anim, Theme};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const STYLES: [(&str, &[&str]); 6] = [
    ("braille", BRAILLE),
    ("dot_ring", DOT_RING),
    ("arc", ARC),
    ("bounce", BOUNCE),
    ("pulse", PULSE),
    ("bars", BARS),
];

pub struct SpinnersDemo {
    anim: Anim,
}

impl SpinnersDemo {
    pub fn new() -> Self {
        Self { anim: Anim::new() }
    }
}

impl Demo for SpinnersDemo {
    fn title(&self) -> &str {
        "spinners"
    }

    fn hint(&self) -> &str {
        "estilos de una celda y barras"
    }

    fn tick(&mut self, anim: &Anim) {
        self.anim = *anim;
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let mut lines: Vec<Line> = STYLES
            .iter()
            .map(|(name, frames)| {
                Line::from(vec![
                    self.anim.glyph(frames, theme),
                    Span::raw(format!(" {name}")),
                ])
            })
            .collect();
        let ratio = ((self.anim.frame / 2) % 101) as f32 / 100.0;
        lines.push(Line::default());
        lines.push(progress(ratio, area.width, theme));
        lines.push(progress_smooth(ratio, area.width, theme));
        f.render_widget(Paragraph::new(lines), area);
    }
}
```

- [ ] **Step 2: Registrar en `demos/mod.rs`**

Añadir `mod spinners;` justo después de `mod spinner;`, y al final del `vec![...]` de `catalog()` (después de `Box::new(toast::ToastDemo::new()),`):

```rust
        Box::new(spinners::SpinnersDemo::new()),
```

- [ ] **Step 3: Checks**

Run: `cargo test -p batten --examples && cargo test -p batten && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS. Si clippy pide `Default` (`new_without_default`), añadir `impl Default for SpinnersDemo { fn default() -> Self { Self::new() } }`; nada más. Si `STYLES` da error de lifetime, cambiar su tipo a `[(&str, &[&'static str]); 6]`.

- [ ] **Step 4: Commit**

```bash
git add crates/batten/examples/showcase/demos
git commit -m "feat(batten): demo de spinners y barras en el showcase"
```
