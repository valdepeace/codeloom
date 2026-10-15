# batten: efectos e integraciones — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Añadir a batten cuatro features opcionales (activas por defecto): efectos (`tachyonfx`), salida ANSI (`ansi-to-tui`), enlaces OSC 8 (`hyperrat`) y rebaja de colores por terminal (`termprofile`), con una demo cada una.

**Architecture:** Un módulo por feature en `widgets/` tras `#[cfg(feature = ...)]`; presets que toman colores del tema. batten no detecta el terminal ni lleva reloj: la app pasa el perfil y el `dt`.

**Tech Stack:** Rust (edición 2021, rustc 1.92), `ratatui` 0.30 / `ratatui-core` 0.1.2, `tachyonfx` 0.25, `ansi-to-tui` 8, `hyperrat` 0.1.3, `termprofile` 0.2.

**Spec:** diseño acordado en conversación (2026-10-15), tarea acotada sin spec aparte.

## Global Constraints

- batten **no** depende de `crossterm` (ninguna de estas crates lo trae).
- Comentarios en español. Mensajes de commit: los indicados, tal cual.
- Cada commit pasa: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p batten` (y `--examples` en la Task 5).
- **Entorno (Git Bash en Windows):** antes de cualquier `cargo`, en el mismo comando:
  `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe'`.
  Si el enlazado falla con `LNK1104` es que hay un `showcase.exe` abierto: reportarlo, no matar procesos.
- No hacer `git push`. No tocar ficheros fuera de los listados.

## Review Focus

- Los presets de `fx` terminan (`done()`) tras su duración y no entran en pánico con área vacía. Tests en Task 1 y `every_demo_renders_without_panicking` (Task 5).
- `ansi_lines` con secuencias rotas o sin color no pierde el texto. Test en Task 2.
- `file_url` con espacios y barras de Windows. Test en Task 3.
- `for_profile(NoColor)` quita colores pero conserva modificadores (negrita). Test en Task 4.
- Límite conocido (no se testea): los degradados generan RGB al pintar y no se rebajan con `for_profile`.

---

### Task 1: dependencias, features y `widgets::fx`

**Files:**
- Modify: `crates/batten/Cargo.toml`, `crates/batten/src/widgets/mod.rs`
- Create: `crates/batten/src/widgets/fx.rs`

**Interfaces:**
- Produces: `batten::widgets::fx::{dialog_open, toast_in, fade_out, message_in, error_flash}` (todas `(…, theme: &Theme) -> Effect`; `fade_out(ms: u32, theme)`), y re-exports `Effect`, `EffectManager`, `FxDuration` (= `tachyonfx::Duration`) y el crate `tachyonfx`.

- [ ] **Step 1: `crates/batten/Cargo.toml`**

Añadir al final de `[dependencies]`:

```toml
tachyonfx = { version = "0.25", optional = true }
ansi-to-tui = { version = "8", optional = true }
hyperrat = { version = "0.1.3", optional = true }
termprofile = { version = "0.2", optional = true, features = ["convert", "ratatui"] }
```

Y justo antes de `[dependencies]`, añadir:

```toml
[features]
default = ["fx", "ansi", "links", "termprofile"]
fx = ["dep:tachyonfx"]
ansi = ["dep:ansi-to-tui"]
links = ["dep:hyperrat"]
termprofile = ["dep:termprofile"]

```

- [ ] **Step 2: Crear `crates/batten/src/widgets/fx.rs`**

```rust
//! Efectos visuales (tachyonfx) con presets que toman colores del tema.
//! Se aplican sobre el buffer después de pintar:
//! `effect.process(dt, frame.buffer_mut(), area)` en cada frame hasta `done()`.

use super::gradient::color_of;
use super::theme::Theme;
pub use tachyonfx;
pub use tachyonfx::{Duration as FxDuration, Effect, EffectManager};
use tachyonfx::{fx, Interpolation, Motion};

/// Apertura de diálogo: barrido de arriba abajo desde `dim` (250 ms).
pub fn dialog_open(theme: &Theme) -> Effect {
    fx::sweep_in(
        Motion::UpToDown,
        10,
        0,
        color_of(theme.styles.dim),
        (250, Interpolation::QuadOut),
    )
}

/// Entrada de aviso: barrido de izquierda a derecha desde `dim` (200 ms).
pub fn toast_in(theme: &Theme) -> Effect {
    fx::sweep_in(
        Motion::LeftToRight,
        8,
        0,
        color_of(theme.styles.dim),
        (200, Interpolation::QuadOut),
    )
}

/// Desvanecer el texto hacia `dim` en `ms` milisegundos.
pub fn fade_out(ms: u32, theme: &Theme) -> Effect {
    fx::fade_to_fg(color_of(theme.styles.dim), (ms, Interpolation::QuadIn))
}

/// Mensaje nuevo: el texto aparece desde `dim` (300 ms).
pub fn message_in(theme: &Theme) -> Effect {
    fx::fade_from_fg(color_of(theme.styles.dim), (300, Interpolation::QuadOut))
}

/// Destello de error: el texto parte del color de error y vuelve (400 ms).
pub fn error_flash(theme: &Theme) -> Effect {
    fx::fade_from_fg(color_of(theme.styles.error), (400, Interpolation::QuadOut))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::style::{Color, Style};

    fn run(mut e: Effect, ms: u32) -> (Effect, Buffer) {
        let mut b = Buffer::empty(Rect::new(0, 0, 6, 1));
        b.set_string(0, 0, "hola", Style::default().fg(Color::White));
        let area = b.area;
        e.process(FxDuration::from_millis(ms), &mut b, area);
        (e, b)
    }

    #[test]
    fn presets_finish_after_their_duration() {
        let th = Theme::test();
        let cases = [
            (dialog_open(&th), 250),
            (toast_in(&th), 200),
            (fade_out(150, &th), 150),
            (message_in(&th), 300),
            (error_flash(&th), 400),
        ];
        for (e, ms) in cases {
            let (e, _) = run(e, ms + 50);
            assert!(e.done());
        }
    }

    #[test]
    fn message_in_is_midway_after_100ms() {
        let (e, b) = run(message_in(&Theme::test()), 100);
        assert!(e.running());
        assert_ne!(b[(0, 0)].fg, Color::White);
    }

    #[test]
    fn empty_area_does_not_panic() {
        let mut b = Buffer::empty(Rect::new(0, 0, 0, 0));
        let mut e = dialog_open(&Theme::test());
        let area = b.area;
        e.process(FxDuration::from_millis(50), &mut b, area);
    }
}
```

- [ ] **Step 3: Registrar en `crates/batten/src/widgets/mod.rs`**

Añadir, junto a los demás `pub mod` (después de `pub mod file_changes;`):

```rust
#[cfg(feature = "fx")]
pub mod fx;
```

- [ ] **Step 4: Checks**

Run: `cargo test -p batten fx:: && cargo test -p batten && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (si `fmt --check` falla, `cargo fmt` y repetir). `cargo` descargará las crates nuevas; `Cargo.lock` cambia y se commitea.

- [ ] **Step 5: Commit**

```bash
git add crates/batten Cargo.lock
git commit -m "feat(batten): efectos visuales con tachyonfx y features opcionales"
```

---

### Task 2: `widgets::ansi`

**Files:**
- Create: `crates/batten/src/widgets/ansi.rs`
- Modify: `crates/batten/src/widgets/mod.rs`

**Interfaces:**
- Produces: `batten::widgets::ansi::ansi_lines(bytes: &[u8]) -> Vec<Line<'static>>`.

- [ ] **Step 1: Crear `crates/batten/src/widgets/ansi.rs`**

```rust
//! Salida de terminal con colores ANSI (SGR) → líneas de ratatui.

use ansi_to_tui::IntoText;
use ratatui::text::Line;

/// Convierte bytes con secuencias ANSI en líneas con estilo. Las secuencias
/// desconocidas se ignoran; si el parser falla, devuelve el texto plano.
pub fn ansi_lines(bytes: &[u8]) -> Vec<Line<'static>> {
    match bytes.into_text() {
        Ok(text) => text.lines,
        // ponytail: fallback con pérdida; las secuencias rotas quedan visibles.
        Err(_) => String::from_utf8_lossy(bytes)
            .lines()
            .map(|l| Line::from(l.to_string()))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    fn text(l: &Line) -> String {
        l.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn keeps_text_and_colors() {
        let l = ansi_lines(b"\x1b[31mred\x1b[0m plain");
        assert_eq!(l.len(), 1);
        assert_eq!(text(&l[0]), "red plain");
        assert_eq!(l[0].spans[0].style.fg, Some(Color::Red));
    }

    #[test]
    fn splits_lines() {
        assert_eq!(ansi_lines(b"a\nb").len(), 2);
    }

    #[test]
    fn plain_and_broken_sequences_keep_text() {
        assert_eq!(text(&ansi_lines(b"sin color")[0]), "sin color");
        let l = ansi_lines(b"\x1b[999zok");
        assert!(text(&l[0]).contains("ok"));
    }
}
```

- [ ] **Step 2: Registrar en `mod.rs`**

Añadir (antes de `pub mod chip;`):

```rust
#[cfg(feature = "ansi")]
pub mod ansi;
```

- [ ] **Step 3: Checks**

Run: `cargo test -p batten ansi:: && cargo test -p batten && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/batten/src/widgets
git commit -m "feat(batten): salida ANSI convertida a líneas con estilo"
```

---

### Task 3: `widgets::link`

**Files:**
- Create: `crates/batten/src/widgets/link.rs`
- Modify: `crates/batten/src/widgets/mod.rs`

**Interfaces:**
- Produces: `batten::widgets::link::{link(label: &'a str, url: &'a str, theme: &Theme) -> Link<'a>, file_url(path: &Path) -> String, Link}`.

- [ ] **Step 1: Crear `crates/batten/src/widgets/link.rs`**

```rust
//! Enlaces clicables (OSC 8) con el estilo del tema. En terminales sin
//! soporte se pintan como texto normal.

use super::theme::Theme;
pub use hyperrat::Link;
use ratatui::style::Modifier;
use std::path::Path;

/// Enlace con estilo `accent` subrayado; si se recorta, termina en `…`.
pub fn link<'a>(label: &'a str, url: &'a str, theme: &Theme) -> Link<'a> {
    Link::new(label, url)
        .style(theme.styles.accent.add_modifier(Modifier::UNDERLINED))
        .fallback_suffix("…")
}

/// URL `file://` de una ruta (absoluta, o relativa al directorio actual),
/// con barras `/` y espacios escapados.
pub fn file_url(path: &Path) -> String {
    let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let s = abs.to_string_lossy().replace('\\', "/").replace(' ', "%20");
    if s.starts_with('/') {
        format!("file://{s}")
    } else {
        format!("file:///{s}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::style::Color;
    use ratatui::widgets::Widget;

    #[test]
    fn link_uses_accent_underlined() {
        let th = Theme::test();
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 1));
        let area = buf.area;
        link("main.rs", "file:///x/main.rs", &th)
            .enabled(false)
            .render(area, &mut buf);
        assert_eq!(buf[(0, 0)].symbol(), "m");
        assert_eq!(buf[(0, 0)].fg, Color::Rgb(0, 212, 255));
        assert!(buf[(0, 0)].modifier.contains(Modifier::UNDERLINED));
    }

    #[cfg(windows)]
    #[test]
    fn file_url_windows() {
        assert_eq!(
            file_url(Path::new(r"E:\a b\c.rs")),
            "file:///E:/a%20b/c.rs"
        );
    }

    #[cfg(unix)]
    #[test]
    fn file_url_unix() {
        assert_eq!(file_url(Path::new("/a b/c.rs")), "file:///a%20b/c.rs");
    }
}
```

- [ ] **Step 2: Registrar en `mod.rs`**

Añadir (después de `pub mod key_help;`):

```rust
#[cfg(feature = "links")]
pub mod link;
```

- [ ] **Step 3: Checks**

Run: `cargo test -p batten link:: && cargo test -p batten && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/batten/src/widgets
git commit -m "feat(batten): enlaces clicables OSC 8 con estilo del tema"
```

---

### Task 4: `Theme::for_profile`

**Files:**
- Create: `crates/batten/src/widgets/profile.rs`
- Modify: `crates/batten/src/widgets/mod.rs`

**Interfaces:**
- Consumes: `Theme { styles: Styles, glyphs: Glyphs, extensions: BTreeMap<String, Style> }` (campos públicos), todos los campos de `Styles` (incluido `border`).
- Produces: `Theme::for_profile(&self, profile: TermProfile) -> Theme`; re-export `batten::widgets::profile::TermProfile`.

- [ ] **Step 1: Crear `crates/batten/src/widgets/profile.rs`**

```rust
//! Rebaja de los colores del tema según lo que soporte el terminal.

use super::theme::{Styles, Theme};
use ratatui::style::Style;
pub use termprofile::TermProfile;

impl Theme {
    /// Adapta todos los estilos al perfil del terminal (truecolor → 256 → 16 →
    /// sin color). La detección es cosa de la app: `TermProfile::detect`.
    // ponytail: los degradados (logo, progress, panel, spinner) generan RGB al
    // pintar y no pasan por aquí; adaptarlos en `gradient::lerp_color` si hace falta.
    pub fn for_profile(&self, profile: TermProfile) -> Self {
        let a = |st: Style| -> Style { profile.adapt_style(st) };
        let s = &self.styles;
        let styles = Styles {
            default: a(s.default),
            muted: a(s.muted),
            dim: a(s.dim),
            heading: a(s.heading),
            user: a(s.user),
            assistant: a(s.assistant),
            system: a(s.system),
            error: a(s.error),
            warning: a(s.warning),
            success: a(s.success),
            tool_name: a(s.tool_name),
            tool_pending: a(s.tool_pending),
            tool_running: a(s.tool_running),
            tool_success: a(s.tool_success),
            tool_error: a(s.tool_error),
            tool_canceled: a(s.tool_canceled),
            todo_pending: a(s.todo_pending),
            todo_in_progress: a(s.todo_in_progress),
            todo_completed: a(s.todo_completed),
            accent: a(s.accent),
            accent_from: a(s.accent_from),
            accent_to: a(s.accent_to),
            logo_field: a(s.logo_field),
            scrollbar: a(s.scrollbar),
            border: a(s.border),
        };
        let extensions = self
            .extensions
            .iter()
            .map(|(k, v)| (k.clone(), a(*v)))
            .collect();
        Theme {
            styles,
            glyphs: self.glyphs,
            extensions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::{Color, Modifier};

    #[test]
    fn truecolor_is_identity() {
        let th = Theme::test();
        let t = th.for_profile(TermProfile::TrueColor);
        assert_eq!(t.styles.accent, th.styles.accent);
        assert_eq!(t.styles.muted, th.styles.muted);
    }

    #[test]
    fn ansi256_turns_rgb_into_indexed() {
        let t = Theme::test().for_profile(TermProfile::Ansi256);
        assert!(matches!(t.styles.accent.fg, Some(Color::Indexed(_))));
    }

    #[test]
    fn ansi16_has_no_rgb_or_indexed() {
        let t = Theme::test().for_profile(TermProfile::Ansi16);
        for fg in [t.styles.accent.fg, t.styles.muted.fg, t.styles.border.fg] {
            assert!(fg.is_some());
            assert!(!matches!(fg, Some(Color::Rgb(..)) | Some(Color::Indexed(_))));
        }
    }

    #[test]
    fn no_color_strips_colors_but_keeps_modifiers() {
        let t = Theme::test().for_profile(TermProfile::NoColor);
        assert_eq!(t.styles.error.fg, None);
        assert!(t.styles.heading.add_modifier.contains(Modifier::BOLD));
    }
}
```

- [ ] **Step 2: Registrar en `mod.rs`**

Añadir (después de `pub mod panel;`):

```rust
#[cfg(feature = "termprofile")]
pub mod profile;
```

- [ ] **Step 3: Checks**

Run: `cargo test -p batten profile:: && cargo test -p batten && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS. Si falla con "missing field" en `Styles`, añadir el campo que falte con `a(s.<campo>)` y reportarlo.

- [ ] **Step 4: Commit**

```bash
git add crates/batten/src/widgets
git commit -m "feat(batten): rebaja de colores del tema según el terminal"
```

---

### Task 5: demos `fx`, `ansi`, `links`, `profile`

**Files:**
- Create: `crates/batten/examples/showcase/demos/{fx,ansi,links,profile}.rs`
- Modify: `crates/batten/examples/showcase/demos/mod.rs`

- [ ] **Step 1: Crear `demos/fx.rs`**

```rust
//! Demo de efectos: una tecla por preset, aplicado sobre un panel.

use crate::demo::Demo;
use batten::state::Tone;
use batten::widgets::fx::{
    dialog_open, error_flash, fade_out, message_in, toast_in, Effect, FxDuration,
};
use batten::widgets::{render_panel, Panel, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct FxDemo {
    pending: Option<char>,
    effect: Option<Effect>,
    name: &'static str,
}

impl FxDemo {
    pub fn new() -> Self {
        Self {
            pending: None,
            effect: None,
            name: "",
        }
    }
}

impl Demo for FxDemo {
    fn title(&self) -> &str {
        "fx"
    }

    fn hint(&self) -> &str {
        "o t f m e: lanzar efecto"
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char(c @ ('o' | 't' | 'f' | 'm' | 'e')) => {
                self.pending = Some(c);
                true
            }
            _ => false,
        }
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        if let Some(c) = self.pending.take() {
            let (name, e) = match c {
                'o' => ("dialog_open", dialog_open(theme)),
                't' => ("toast_in", toast_in(theme)),
                'f' => ("fade_out", fade_out(400, theme)),
                'm' => ("message_in", message_in(theme)),
                _ => ("error_flash", error_flash(theme)),
            };
            self.name = name;
            self.effect = Some(e);
        }
        let panel = Panel {
            title: "Efectos",
            info: self.name,
            tone: Tone::Neutral,
        };
        let inner = render_panel(f, area, &panel, theme);
        let lines = vec![
            Line::from("o dialog_open · t toast_in · f fade_out"),
            Line::from("m message_in · e error_flash"),
            Line::from("El efecto se aplica a todo este panel."),
        ];
        f.render_widget(Paragraph::new(lines), inner);
        if let Some(e) = self.effect.as_mut() {
            e.process(FxDuration::from_millis(50), f.buffer_mut(), area);
        }
    }
}
```

- [ ] **Step 2: Crear `demos/ansi.rs`**

```rust
//! Demo de salida ANSI: una compilación de cargo con sus colores.

use crate::demo::Demo;
use batten::widgets::ansi::ansi_lines;
use batten::widgets::Theme;
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const SAMPLE: &[u8] = b"\x1b[1m\x1b[32m   Compiling\x1b[0m batten v0.1.0\n\
\x1b[1m\x1b[33mwarning\x1b[0m: unused variable: `x`\n\
\x1b[1m\x1b[31merror[E0425]\x1b[0m: cannot find value `y` in this scope\n\
\x1b[1m\x1b[32m    Finished\x1b[0m `dev` profile in 2.1s";

pub struct AnsiDemo;

impl AnsiDemo {
    pub fn new() -> Self {
        Self
    }
}

impl Demo for AnsiDemo {
    fn title(&self) -> &str {
        "ansi"
    }

    fn hint(&self) -> &str {
        "salida de cargo con colores"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, _theme: &Theme) {
        f.render_widget(Paragraph::new(ansi_lines(SAMPLE)), area);
    }
}
```

- [ ] **Step 3: Crear `demos/links.rs`**

```rust
//! Demo de enlaces OSC 8 (clic con ctrl/cmd en terminales compatibles).

use crate::demo::Demo;
use batten::widgets::link::{file_url, link};
use batten::widgets::Theme;
use ratatui::layout::Rect;
use ratatui::Frame;
use std::path::Path;

pub struct LinksDemo;

impl LinksDemo {
    pub fn new() -> Self {
        Self
    }
}

impl Demo for LinksDemo {
    fn title(&self) -> &str {
        "links"
    }

    fn hint(&self) -> &str {
        "ctrl+clic abre el enlace"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let rows = [
            ("https://ratatui.rs", "https://ratatui.rs".to_string()),
            ("Cargo.toml", file_url(Path::new("Cargo.toml"))),
            (
                "crates/batten/src/lib.rs",
                file_url(Path::new("crates/batten/src/lib.rs")),
            ),
        ];
        for (i, (label, url)) in rows.iter().enumerate() {
            let y = i as u16;
            if y >= area.height {
                break;
            }
            let row = Rect {
                x: area.x,
                y: area.y + y,
                width: area.width,
                height: 1,
            };
            f.render_widget(link(label, url, theme), row);
        }
    }
}
```

- [ ] **Step 4: Crear `demos/profile.rs`**

```rust
//! Demo de rebaja de colores: el mismo tema en los cuatro perfiles.

use crate::demo::Demo;
use batten::state::Tone;
use batten::widgets::profile::TermProfile;
use batten::widgets::{chip, status, Theme};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct ProfileDemo;

impl ProfileDemo {
    pub fn new() -> Self {
        Self
    }
}

impl Demo for ProfileDemo {
    fn title(&self) -> &str {
        "profile"
    }

    fn hint(&self) -> &str {
        "truecolor · 256 · 16 · sin color"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let profiles = [
            TermProfile::TrueColor,
            TermProfile::Ansi256,
            TermProfile::Ansi16,
            TermProfile::NoColor,
        ];
        let cols = Layout::horizontal([Constraint::Ratio(1, 4); 4]).split(area);
        for (p, col) in profiles.into_iter().zip(cols.iter()) {
            let label = format!("{p:?}");
            let th = theme.for_profile(p);
            let mut lines = vec![Line::from(label)];
            for tone in Tone::ALL {
                lines.push(Line::from(chip("TAG", tone, &th)));
                lines.push(status(tone, "estado", "detalle", col.width, &th));
            }
            f.render_widget(Paragraph::new(lines), *col);
        }
    }
}
```

- [ ] **Step 5: Registrar en `demos/mod.rs`**

Añadir los `mod` en orden alfabético con los existentes (`ansi`, `fx`, `links`, `profile`), y al final del `vec![...]` de `catalog()` (después de `Box::new(spinners::SpinnersDemo::new()),`):

```rust
        Box::new(fx::FxDemo::new()),
        Box::new(ansi::AnsiDemo::new()),
        Box::new(links::LinksDemo::new()),
        Box::new(profile::ProfileDemo::new()),
```

- [ ] **Step 6: Checks**

Run: `cargo test -p batten --examples && cargo test -p batten && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS, incluido `every_demo_renders_without_panicking`. Si clippy pide `Default` (`new_without_default`), añadir `impl Default for <Demo> { fn default() -> Self { Self::new() } }` al demo afectado; nada más.

- [ ] **Step 7: Commit**

```bash
git add crates/batten/examples/showcase/demos
git commit -m "feat(batten): demos de efectos, ANSI, enlaces y perfiles de color"
```
