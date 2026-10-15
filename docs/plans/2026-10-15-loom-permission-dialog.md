# Diálogo de permisos — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Diálogo de permisos en batten (con diff unified y botones) y su integración en loom, para que cada tool pida confirmación salvo con `--yolo`.

**Architecture:** Fase 1 (batten): `state::Diff` (con `similar`), `widgets::diff`, `widgets::button`, `state::PermissionDialog` + `widgets::permission`, demo. Fase 2 (loom): `Decision::AllowSession` en core; la TUI recibe peticiones por un `mpsc`, abre el diálogo y responde por `oneshot`; el CLI conecta `UiPermissionResponder` y respeta `--yolo`.

**Tech Stack:** Rust (edición 2021, rustc 1.92), `ratatui` 0.30, `similar` 3, `tokio`, `serde_json`.

**Spec:** `docs/specs/2026-10-15-loom-permission-dialog-design.md`

## Global Constraints

- `batten::state` no depende de `ratatui` ni `crossterm` (`similar` sí está permitido: es puro). batten no depende de crossterm.
- `codeloom-core` no depende de `ratatui`/`crossterm`.
- Comentarios en español. Mensajes de commit: los indicados, tal cual.
- Cada commit pasa: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test` del crate tocado (`-p batten`, `-p codeloom-core`, `-p codeloom-tui`, `-p codeloom-cli`), y en la Task 5 también `cargo test -p batten --examples`.
- **Entorno (Git Bash en Windows):** antes de cualquier `cargo`, en el mismo comando:
  `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe'`.
  Si el enlazado falla con `LNK1104`, hay un ejecutable abierto: reportarlo, no matar procesos.
- No hacer `git push`. No tocar ficheros fuera de los listados.

## Review Focus

- Diff de textos iguales o sin salto final; números de línea 1-based. Tests en Task 1.
- Scroll del diff más allá del final y áreas mínimas: sin pánico. Tests en Tasks 2 y 4.
- Con el diálogo abierto, las teclas no llegan al composer y `Esc` deniega (no cierra loom); `Ctrl+C` sigue saliendo. Test en Task 7.
- Varias peticiones a la vez: se encolan y cada una recibe su respuesta. Test en Task 7.
- `AllowSession` no vuelve a preguntar por el mismo `tool:action`. Test en Task 6.

---

## Fase 1 — batten

### Task 1: `state::Diff`

**Files:**
- Modify: `crates/batten/Cargo.toml`, `crates/batten/src/state/mod.rs`
- Create: `crates/batten/src/state/diff.rs`

**Interfaces:**
- Produces: `batten::state::{Diff, DiffKind, DiffLine}`; `Diff::new(&str, &str, usize) -> Diff`, `lines() -> &[DiffLine]`, `added()`, `removed()`, `is_empty()`.

- [ ] **Step 1: Dependencia**

En `crates/batten/Cargo.toml`, en `[dependencies]` después de `thiserror = "2"`, añadir:

```toml
similar = "3"
```

- [ ] **Step 2: Crear `crates/batten/src/state/diff.rs`**

```rust
//! Diff unificado por líneas entre dos textos que suministra la app
//! (batten no lee ficheros ni ejecuta Git).

use similar::{ChangeTag, TextDiff};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    /// Cabecera `@@ -a,b +c,d @@`.
    Hunk,
    Context,
    Insert,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffKind,
    /// Número de línea en el texto viejo (1-based); `None` en Insert y Hunk.
    pub old_no: Option<usize>,
    /// Número de línea en el texto nuevo (1-based); `None` en Delete y Hunk.
    pub new_no: Option<usize>,
    /// Texto sin salto de línea final.
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Diff {
    lines: Vec<DiffLine>,
}

impl Diff {
    /// Diff por líneas agrupado en hunks con `context` líneas de contexto.
    pub fn new(before: &str, after: &str, context: usize) -> Self {
        if before == after {
            return Self::default();
        }
        let td = TextDiff::from_lines(before, after);
        let mut lines = Vec::new();
        for group in td.grouped_ops(context) {
            let (Some(first), Some(last)) = (group.first(), group.last()) else {
                continue;
            };
            let (os, oe) = (first.old_range().start, last.old_range().end);
            let (ns, ne) = (first.new_range().start, last.new_range().end);
            lines.push(DiffLine {
                kind: DiffKind::Hunk,
                old_no: None,
                new_no: None,
                text: format!("@@ -{},{} +{},{} @@", os + 1, oe - os, ns + 1, ne - ns),
            });
            for op in &group {
                for ch in td.iter_changes(op) {
                    let kind = match ch.tag() {
                        ChangeTag::Equal => DiffKind::Context,
                        ChangeTag::Delete => DiffKind::Delete,
                        ChangeTag::Insert => DiffKind::Insert,
                    };
                    lines.push(DiffLine {
                        kind,
                        old_no: ch.old_index().map(|i| i + 1),
                        new_no: ch.new_index().map(|i| i + 1),
                        text: ch.value().trim_end_matches(['\n', '\r']).to_string(),
                    });
                }
            }
        }
        Self { lines }
    }

    pub fn lines(&self) -> &[DiffLine] {
        &self.lines
    }

    pub fn added(&self) -> usize {
        self.count(DiffKind::Insert)
    }

    pub fn removed(&self) -> usize {
        self.count(DiffKind::Delete)
    }

    /// `true` si no hay cambios.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    fn count(&self, kind: DiffKind) -> usize {
        self.lines.iter().filter(|l| l.kind == kind).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use DiffKind::*;

    fn kinds(d: &Diff) -> Vec<DiffKind> {
        d.lines().iter().map(|l| l.kind).collect()
    }

    #[test]
    fn equal_texts_are_empty() {
        assert!(Diff::new("a\n", "a\n", 3).is_empty());
    }

    #[test]
    fn one_changed_line() {
        let d = Diff::new("a\nb\nc\n", "a\nB\nc\n", 1);
        assert_eq!(kinds(&d), vec![Hunk, Context, Delete, Insert, Context]);
        assert_eq!(d.lines()[0].text, "@@ -1,3 +1,3 @@");
        assert_eq!((d.lines()[1].old_no, d.lines()[1].new_no), (Some(1), Some(1)));
        assert_eq!((d.lines()[2].old_no, d.lines()[2].new_no), (Some(2), None));
        assert_eq!((d.lines()[3].old_no, d.lines()[3].new_no), (None, Some(2)));
        assert_eq!(d.lines()[3].text, "B");
        assert_eq!((d.added(), d.removed()), (1, 1));
    }

    #[test]
    fn context_is_trimmed() {
        let before: String = (1..=10).map(|i| format!("{i}\n")).collect();
        let after = before.replace("5\n", "cinco\n");
        let d = Diff::new(&before, &after, 1);
        assert_eq!(kinds(&d), vec![Hunk, Context, Delete, Insert, Context]);
        assert_eq!(d.lines()[0].text, "@@ -4,3 +4,3 @@");
        assert_eq!(d.lines()[1].old_no, Some(4));
    }

    #[test]
    fn without_trailing_newline() {
        let d = Diff::new("x", "y", 3);
        assert_eq!(kinds(&d), vec![Hunk, Delete, Insert]);
        assert_eq!(d.lines()[1].text, "x");
        assert_eq!(d.lines()[2].text, "y");
    }
}
```

- [ ] **Step 3: Registrar en `crates/batten/src/state/mod.rs`**

Añadir `pub mod diff;` después de `pub mod composer;`, y `pub use diff::{Diff, DiffKind, DiffLine};` después de `pub use composer::Composer;`.

- [ ] **Step 4: Checks**

Run: `cargo test -p batten diff && cargo test -p batten && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (si `fmt --check` falla, `cargo fmt` y repetir).

- [ ] **Step 5: Commit**

```bash
git add crates/batten Cargo.lock
git commit -m "feat(batten): diff unificado por líneas entre dos textos"
```

---

### Task 2: tokens de diff y `widgets::diff`

**Files:**
- Modify: `crates/batten/src/state/transcript.rs`, `crates/batten/src/widgets/theme.rs`, `crates/batten/src/widgets/profile.rs`, `crates/batten/src/widgets/mod.rs`
- Create: `crates/batten/src/widgets/diff.rs`

**Interfaces:**
- Consumes: `Diff`, `DiffKind`, `DiffLine` (Task 1); `text::truncate`.
- Produces: `Styles.{diff_insert, diff_delete, diff_hunk}`, `StyleToken::{DiffInsert, DiffDelete, DiffHunk}`; `batten::widgets::diff::render(frame: &mut Frame, area: Rect, diff: &Diff, scroll: usize, theme: &Theme)`.

- [ ] **Step 1: Tokens del tema**

1. `crates/batten/src/state/transcript.rs`, en `enum StyleToken`, después de `Border,`: añadir `DiffInsert,`, `DiffDelete,`, `DiffHunk,` (una por línea).
2. `crates/batten/src/widgets/theme.rs`:
   - en `struct Styles`, después de `pub border: Style,`: `pub diff_insert: Style,` `pub diff_delete: Style,` `pub diff_hunk: Style,`;
   - en `impl Default for Styles`, después de `border: Style::default().fg(Color::Indexed(240)),`:
     ```rust
     diff_insert: Style::default().fg(Color::Green),
     diff_delete: Style::default().fg(Color::Red),
     diff_hunk: Style::default().fg(Color::Cyan),
     ```
   - en `set_named_style`, después de `StyleToken::Border => s.border,`: `StyleToken::DiffInsert => s.diff_insert,` `StyleToken::DiffDelete => s.diff_delete,` `StyleToken::DiffHunk => s.diff_hunk,`;
   - en `apply_override`, después de `"border" => s.border = style,`: `"diff_insert" => s.diff_insert = style,` `"diff_delete" => s.diff_delete = style,` `"diff_hunk" => s.diff_hunk = style,`;
   - en `token_to_key`, después de `StyleToken::Border => "border",`: `StyleToken::DiffInsert => "diff_insert",` `StyleToken::DiffDelete => "diff_delete",` `StyleToken::DiffHunk => "diff_hunk",`;
   - en `mod tests`, añadir:
     ```rust
     #[test]
     fn diff_tokens_resolve_and_override() {
         let th = Theme::test();
         assert_eq!(th.style(StyleToken::DiffInsert).fg, Some(Color::Green));
         assert_eq!(th.style(StyleToken::DiffHunk).fg, Some(Color::Cyan));
         let th = Theme::from_toml("[styles]\ndiff_delete = \"magenta\"\n").unwrap();
         assert_eq!(th.style(StyleToken::DiffDelete).fg, Some(Color::Magenta));
     }
     ```
3. `crates/batten/src/widgets/profile.rs`, en el literal `Styles { ... }` de `for_profile`, después de `border: a(s.border),`: añadir `diff_insert: a(s.diff_insert),` `diff_delete: a(s.diff_delete),` `diff_hunk: a(s.diff_hunk),`.

- [ ] **Step 2: Crear `crates/batten/src/widgets/diff.rs`**

```rust
//! Vista unified de un `Diff`: números viejo/nuevo, símbolo y texto coloreado.

use super::text::truncate;
use super::theme::Theme;
use crate::state::{Diff, DiffKind, DiffLine};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

/// Pinta las líneas del diff desde `scroll` (acotado a la última línea).
pub fn render(frame: &mut Frame, area: Rect, diff: &Diff, scroll: usize, theme: &Theme) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let lines = diff.lines();
    let max_no = lines
        .iter()
        .flat_map(|l| [l.old_no, l.new_no])
        .flatten()
        .max()
        .unwrap_or(0);
    let nw = max_no.to_string().len();
    let start = scroll.min(lines.len().saturating_sub(1));
    let w = area.width as usize;
    let rows: Vec<Line<'static>> = lines
        .iter()
        .skip(start)
        .take(area.height as usize)
        .map(|l| row(l, nw, w, theme))
        .collect();
    frame.render_widget(Paragraph::new(rows), area);
}

fn row(l: &DiffLine, nw: usize, w: usize, theme: &Theme) -> Line<'static> {
    let s = &theme.styles;
    let text = l.text.replace('\t', "    ");
    if l.kind == DiffKind::Hunk {
        return Line::from(Span::styled(truncate(&text, w), s.diff_hunk));
    }
    let num = |n: Option<usize>| match n {
        Some(n) => format!("{n:>nw$}"),
        None => " ".repeat(nw),
    };
    let gutter = format!("{} {} ", num(l.old_no), num(l.new_no));
    let (sym, style) = match l.kind {
        DiffKind::Insert => ("+", s.diff_insert),
        DiffKind::Delete => ("-", s.diff_delete),
        _ => (" ", s.default),
    };
    let gw = gutter.width() + 2;
    if gw >= w {
        return Line::from(Span::styled(truncate(&gutter, w), s.dim));
    }
    Line::from(vec![
        Span::styled(gutter, s.dim),
        Span::styled(format!("{sym} "), style),
        Span::styled(truncate(&text, w - gw), style),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;
    use ratatui::Terminal;

    fn draw(diff: &Diff, w: u16, h: u16, scroll: usize) -> Terminal<TestBackend> {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| render(f, f.area(), diff, scroll, &Theme::test()))
            .unwrap();
        term
    }

    fn row_text(term: &Terminal<TestBackend>, y: u16) -> String {
        let buf = term.backend().buffer();
        let s: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
        s.trim_end().to_string()
    }

    fn sample() -> Diff {
        Diff::new("a\nb\nc\n", "a\nB\nc\n", 1)
    }

    #[test]
    fn gutter_symbols_and_colors() {
        let term = draw(&sample(), 20, 5, 0);
        assert_eq!(row_text(&term, 0), "@@ -1,3 +1,3 @@");
        assert_eq!(row_text(&term, 1), "1 1   a");
        assert_eq!(row_text(&term, 2), "2   - b");
        assert_eq!(row_text(&term, 3), "  2 + B");
        assert_eq!(row_text(&term, 4), "3 3   c");
        let buf = term.backend().buffer();
        assert_eq!(buf[(4, 2)].fg, Color::Red);
        assert_eq!(buf[(4, 3)].fg, Color::Green);
    }

    #[test]
    fn scroll_is_clamped_to_last_line() {
        let term = draw(&sample(), 20, 3, 99);
        assert_eq!(row_text(&term, 0), "3 3   c");
    }

    #[test]
    fn long_lines_are_truncated() {
        let d = Diff::new("abcdefghij\n", "x\n", 3);
        let term = draw(&d, 8, 3, 0);
        assert_eq!(row_text(&term, 1), "1   - a…");
    }

    #[test]
    fn tiny_and_empty_do_not_panic() {
        draw(&sample(), 0, 0, 0);
        draw(&sample(), 3, 1, 0);
        draw(&Diff::default(), 10, 3, 5);
    }
}
```

- [ ] **Step 3: Registrar en `crates/batten/src/widgets/mod.rs`**

Añadir `pub mod diff;` después de `pub mod composer;`.

- [ ] **Step 4: Checks**

Run: `cargo test -p batten && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (si `fmt --check` falla, `cargo fmt` y repetir). Si falla con "missing field" en `Styles`, falta un campo en `profile.rs` (Step 1.3).

- [ ] **Step 5: Commit**

```bash
git add crates/batten/src
git commit -m "feat(batten): vista unified del diff con tokens de tema propios"
```

---

### Task 3: `widgets::button`

**Files:**
- Create: `crates/batten/src/widgets/button.rs`
- Modify: `crates/batten/src/widgets/mod.rs`

**Interfaces:**
- Consumes: `Tone`, `tone::tone_style`, `gradient::color_of`.
- Produces: `batten::widgets::{button(label: &str, focused: bool, tone: Tone, theme: &Theme) -> Span<'static>, button_row(items: &[(&str, Tone)], selected: usize, theme: &Theme) -> Line<'static>}`.

- [ ] **Step 1: Crear `crates/batten/src/widgets/button.rs`**

```rust
//! Botones de texto: el enfocado lleva fondo del color del tono.

use super::gradient::color_of;
use super::theme::Theme;
use super::tone::tone_style;
use crate::state::Tone;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// `" label "`; enfocado → fondo del tono, texto negro en negrita; si no, `muted`.
pub fn button(label: &str, focused: bool, tone: Tone, theme: &Theme) -> Span<'static> {
    let text = format!(" {label} ");
    if focused {
        Span::styled(
            text,
            Style::default()
                .bg(color_of(tone_style(tone, theme)))
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(text, theme.styles.muted)
    }
}

/// Fila de botones separados por dos espacios; `selected` es el enfocado.
pub fn button_row(items: &[(&str, Tone)], selected: usize, theme: &Theme) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, (label, tone)) in items.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(button(label, i == selected, *tone, theme));
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
    fn focused_uses_tone_background() {
        let th = Theme::test();
        let b = button("Denegar", true, Tone::Error, &th);
        assert_eq!(b.content, " Denegar ");
        assert_eq!(b.style.bg, Some(Color::Red));
        let b = button("Denegar", false, Tone::Error, &th);
        assert_eq!(b.style, th.styles.muted);
    }

    #[test]
    fn row_marks_only_the_selected() {
        let th = Theme::test();
        let items = [("Sí", Tone::Success), ("No", Tone::Error)];
        let l = button_row(&items, 1, &th);
        assert_eq!(text(&l), " Sí    No ");
        assert_eq!(l.spans[0].style, th.styles.muted);
        assert_eq!(l.spans[2].style.bg, Some(Color::Red));
    }
}
```

- [ ] **Step 2: Registrar en `mod.rs`**

Añadir `pub mod button;` antes de `pub mod chip;`, y `pub use button::{button, button_row};` antes de `pub use chip::chip;`.

- [ ] **Step 3: Checks**

Run: `cargo test -p batten button && cargo test -p batten && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/batten/src/widgets
git commit -m "feat(batten): botones de texto con foco por tono"
```

---

### Task 4: `PermissionDialog` y `widgets::permission`

**Files:**
- Create: `crates/batten/src/state/permission.rs`, `crates/batten/src/widgets/permission.rs`
- Modify: `crates/batten/src/state/mod.rs`, `crates/batten/src/widgets/mod.rs`

**Interfaces:**
- Consumes: `Diff` (Task 1), `widgets::diff::render` (Task 2), `button_row` (Task 3), `render_panel`/`Panel`, `key_help`, `Tone`.
- Produces: `batten::state::{PermissionChoice, PermissionBody, PermissionDialog}`; `PermissionDialog::new(tool: impl Into<String>, subtitle: impl Into<String>, body: PermissionBody)`, `choice()`, `next()`, `prev()`, `scroll_by(isize)`, `scroll()`; `PermissionChoice::index(self) -> usize`; `batten::widgets::permission::{area(screen: Rect) -> Rect, render(frame: &mut Frame, screen: Rect, dialog: &PermissionDialog, theme: &Theme)}`.

- [ ] **Step 1: Crear `crates/batten/src/state/permission.rs`**

```rust
//! Estado del diálogo de permisos (sin teclas: la app traduce su teclado).

use super::diff::Diff;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PermissionChoice {
    #[default]
    Allow,
    AllowSession,
    Deny,
}

impl PermissionChoice {
    const ALL: [PermissionChoice; 3] = [
        PermissionChoice::Allow,
        PermissionChoice::AllowSession,
        PermissionChoice::Deny,
    ];

    /// Posición en la fila de botones.
    pub fn index(self) -> usize {
        match self {
            PermissionChoice::Allow => 0,
            PermissionChoice::AllowSession => 1,
            PermissionChoice::Deny => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PermissionBody {
    Text(String),
    Diff(Diff),
}

#[derive(Debug, Clone)]
pub struct PermissionDialog {
    pub tool: String,
    /// Línea secundaria (p. ej. la ruta); vacía si no hay.
    pub subtitle: String,
    pub body: PermissionBody,
    choice: PermissionChoice,
    scroll: usize,
}

impl PermissionDialog {
    pub fn new(tool: impl Into<String>, subtitle: impl Into<String>, body: PermissionBody) -> Self {
        Self {
            tool: tool.into(),
            subtitle: subtitle.into(),
            body,
            choice: PermissionChoice::default(),
            scroll: 0,
        }
    }

    pub fn choice(&self) -> PermissionChoice {
        self.choice
    }

    /// Siguiente opción (cíclica).
    pub fn next(&mut self) {
        self.choice = PermissionChoice::ALL[(self.choice.index() + 1) % 3];
    }

    /// Opción anterior (cíclica).
    pub fn prev(&mut self) {
        self.choice = PermissionChoice::ALL[(self.choice.index() + 2) % 3];
    }

    /// Desplaza el cuerpo; nunca baja de 0.
    pub fn scroll_by(&mut self, delta: isize) {
        self.scroll = self.scroll.saturating_add_signed(delta);
    }

    pub fn scroll(&self) -> usize {
        self.scroll
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dialog() -> PermissionDialog {
        PermissionDialog::new("bash", "", PermissionBody::Text("ls".into()))
    }

    #[test]
    fn choices_cycle_both_ways() {
        let mut d = dialog();
        assert_eq!(d.choice(), PermissionChoice::Allow);
        d.next();
        assert_eq!(d.choice(), PermissionChoice::AllowSession);
        d.next();
        d.next();
        assert_eq!(d.choice(), PermissionChoice::Allow);
        d.prev();
        assert_eq!(d.choice(), PermissionChoice::Deny);
    }

    #[test]
    fn scroll_never_goes_negative() {
        let mut d = dialog();
        d.scroll_by(-3);
        assert_eq!(d.scroll(), 0);
        d.scroll_by(5);
        d.scroll_by(-2);
        assert_eq!(d.scroll(), 3);
    }
}
```

- [ ] **Step 2: Registrar en `state/mod.rs`**

Añadir `pub mod permission;` después de `pub mod history;`, y `pub use permission::{PermissionBody, PermissionChoice, PermissionDialog};` después de `pub use history::History;`.

- [ ] **Step 3: Crear `crates/batten/src/widgets/permission.rs`**

```rust
//! Diálogo de permisos: panel centrado con cuerpo (texto o diff), botones y ayuda.

use super::button::button_row;
use super::diff;
use super::key_help::key_help;
use super::panel::{render as render_panel, Panel};
use super::theme::Theme;
use crate::state::{PermissionBody, PermissionDialog, Tone};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Span;
use ratatui::widgets::{Clear, Paragraph, Wrap};
use ratatui::Frame;

const BUTTONS: [(&str, Tone); 3] = [
    ("Permitir", Tone::Success),
    ("Permitir en la sesión", Tone::Info),
    ("Denegar", Tone::Error),
];

const HELP: [(&str, &str); 5] = [
    ("←→", "elegir"),
    ("enter", "confirmar"),
    ("a/s/d", "atajo"),
    ("↑↓", "scroll"),
    ("esc", "denegar"),
];

/// Rectángulo del diálogo: 80 % del ancho (máx. 100) y 80 % del alto
/// (mín. 8 filas), centrado y nunca mayor que la pantalla.
pub fn area(screen: Rect) -> Rect {
    let w = ((screen.width as u32 * 8 / 10) as u16).min(100);
    let h = ((screen.height as u32 * 8 / 10) as u16)
        .max(8)
        .min(screen.height);
    Rect {
        x: screen.x + (screen.width - w) / 2,
        y: screen.y + (screen.height - h) / 2,
        width: w,
        height: h,
    }
}

/// Pinta el diálogo sobre `screen` (limpia su área antes).
pub fn render(frame: &mut Frame, screen: Rect, dialog: &PermissionDialog, theme: &Theme) {
    let outer = area(screen);
    if outer.width < 2 || outer.height < 2 {
        return;
    }
    frame.render_widget(Clear, outer);
    let panel = Panel {
        title: "Permiso",
        info: &dialog.tool,
        tone: Tone::Warning,
    };
    let inner = render_panel(frame, outer, &panel, theme);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let sub_h = u16::from(!dialog.subtitle.is_empty());
    let [sub, body, _gap, buttons, help] = Layout::vertical([
        Constraint::Length(sub_h),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);
    if sub_h == 1 {
        let line = Span::styled(dialog.subtitle.clone(), theme.styles.muted);
        frame.render_widget(Paragraph::new(line), sub);
    }
    match &dialog.body {
        PermissionBody::Text(t) => {
            let y = u16::try_from(dialog.scroll()).unwrap_or(u16::MAX);
            let p = Paragraph::new(t.clone())
                .wrap(Wrap { trim: false })
                .scroll((y, 0));
            frame.render_widget(p, body);
        }
        PermissionBody::Diff(d) => diff::render(frame, body, d, dialog.scroll(), theme),
    }
    let row = button_row(&BUTTONS, dialog.choice().index(), theme);
    frame.render_widget(Paragraph::new(row), buttons);
    frame.render_widget(Paragraph::new(key_help(&HELP, help.width, theme)), help);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Diff;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn area_is_centered_and_bounded() {
        assert_eq!(area(Rect::new(0, 0, 120, 40)), Rect::new(12, 4, 96, 32));
        assert_eq!(area(Rect::new(0, 0, 200, 50)), Rect::new(50, 5, 100, 40));
        assert_eq!(area(Rect::new(0, 0, 20, 5)), Rect::new(2, 0, 16, 5));
        assert_eq!(area(Rect::new(0, 0, 0, 0)), Rect::new(0, 0, 0, 0));
    }

    fn screen_text(dialog: &PermissionDialog, w: u16, h: u16) -> String {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| render(f, f.area(), dialog, &Theme::test()))
            .unwrap();
        term.backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn shows_title_body_and_buttons() {
        let d = PermissionDialog::new(
            "edit",
            "src/main.rs",
            PermissionBody::Diff(Diff::new("a\n", "b\n", 3)),
        );
        let s = screen_text(&d, 120, 40);
        for needle in ["Permiso", "edit", "src/main.rs", "@@ -1,1 +1,1 @@", "Permitir", "Denegar"] {
            assert!(s.contains(needle), "falta {needle:?}");
        }
    }

    #[test]
    fn small_screens_do_not_panic() {
        let d = PermissionDialog::new("bash", "", PermissionBody::Text("ls -la".into()));
        for (w, h) in [(0, 0), (1, 1), (20, 5), (120, 40)] {
            screen_text(&d, w, h);
        }
    }
}
```

- [ ] **Step 4: Registrar en `widgets/mod.rs`**

Añadir `pub mod permission;` después de `pub mod panel;`.

- [ ] **Step 5: Checks**

Run: `cargo test -p batten permission && cargo test -p batten && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/batten/src
git commit -m "feat(batten): diálogo de permisos con diff, botones y ayuda"
```

---

### Task 5: demo `permission`

**Files:**
- Create: `crates/batten/examples/showcase/demos/permission.rs`
- Modify: `crates/batten/examples/showcase/demos/mod.rs`

- [ ] **Step 1: Crear `demos/permission.rs`**

```rust
//! Demo del diálogo de permisos: un edit con diff y un bash con su comando.

use crate::demo::Demo;
use batten::state::{Diff, PermissionBody, PermissionChoice, PermissionDialog};
use batten::widgets::fx::{dialog_open, Effect, FxDuration};
use batten::widgets::permission;
use batten::widgets::Theme;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const BEFORE: &str = "fn main() {\n    let x = 1;\n    println!(\"{}\", x);\n}\n";
const AFTER: &str = "fn main() {\n    let x = 2;\n    let y = x * 2;\n    println!(\"{}\", y);\n}\n";

pub struct PermissionDemo {
    dialogs: [PermissionDialog; 2],
    current: usize,
    pending_fx: bool,
    effect: Option<Effect>,
    last: Option<PermissionChoice>,
}

impl PermissionDemo {
    pub fn new() -> Self {
        Self {
            dialogs: [
                PermissionDialog::new(
                    "edit",
                    "src/main.rs",
                    PermissionBody::Diff(Diff::new(BEFORE, AFTER, 3)),
                ),
                PermissionDialog::new(
                    "bash",
                    "",
                    PermissionBody::Text("cargo test --workspace && git status".into()),
                ),
            ],
            current: 0,
            pending_fx: true,
            effect: None,
            last: None,
        }
    }
}

impl Demo for PermissionDemo {
    fn title(&self) -> &str {
        "permission"
    }

    fn hint(&self) -> &str {
        "n: otro caso · ←→ elegir · ↑↓ scroll · enter · o: efecto"
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        let d = &mut self.dialogs[self.current];
        match code {
            KeyCode::Char('n') => {
                self.current = (self.current + 1) % 2;
                self.pending_fx = true;
            }
            KeyCode::Left => d.prev(),
            KeyCode::Right => d.next(),
            KeyCode::Up => d.scroll_by(-1),
            KeyCode::Down => d.scroll_by(1),
            KeyCode::Enter => self.last = Some(d.choice()),
            KeyCode::Char('o') => self.pending_fx = true,
            _ => return false,
        }
        true
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let last = match self.last {
            Some(c) => format!("última decisión: {c:?}"),
            None => "sin decisión todavía".into(),
        };
        let bg = vec![
            Line::from("Simulación: el agente quiere ejecutar una tool."),
            Line::from(last),
        ];
        f.render_widget(Paragraph::new(bg), area);
        permission::render(f, area, &self.dialogs[self.current], theme);
        if self.pending_fx {
            self.effect = Some(dialog_open(theme));
            self.pending_fx = false;
        }
        if let Some(e) = self.effect.as_mut() {
            e.process(FxDuration::from_millis(50), f.buffer_mut(), permission::area(area));
        }
    }
}
```

- [ ] **Step 2: Registrar en `demos/mod.rs`**

Añadir `mod permission;` en orden alfabético con los demás `mod`, y al final del `vec![...]` de `catalog()` (después de `Box::new(profile::ProfileDemo::new()),`):

```rust
        Box::new(permission::PermissionDemo::new()),
```

- [ ] **Step 3: Checks**

Run: `cargo test -p batten --examples && cargo test -p batten && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS. Si clippy pide `Default` (`new_without_default`), añadir `impl Default for PermissionDemo { fn default() -> Self { Self::new() } }`; nada más.

- [ ] **Step 4: Commit**

```bash
git add crates/batten/examples/showcase/demos
git commit -m "feat(batten): demo del diálogo de permisos en el showcase"
```

---

## Fase 2 — loom

### Task 6: core — `Decision::AllowSession`

**Files:**
- Modify: `crates/codeloom-core/src/permission/mod.rs`, `crates/codeloom-core/src/agent/mod.rs`

**Interfaces:**
- Produces: `Decision::AllowSession`. `PermissionService::check` nunca lo devuelve (lo convierte en `Allow` y lo persiste).

- [ ] **Step 1: Test (falla)**

En `crates/codeloom-core/src/permission/mod.rs`, dentro de `mod tests`, añadir:

```rust
    struct SessionOnce(std::sync::Arc<std::sync::atomic::AtomicUsize>);

    #[async_trait]
    impl PermissionResponder for SessionOnce {
        async fn decide(&self, _req: &PermissionRequest) -> Decision {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Decision::AllowSession
        }
    }

    #[tokio::test]
    async fn allow_session_persists_for_the_same_tool_action() {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let svc = PermissionService::with_responder(Box::new(SessionOnce(calls.clone())));
        assert_eq!(svc.check(req("bash", "run")).await, Decision::Allow);
        assert_eq!(svc.check(req("bash", "run")).await, Decision::Allow);
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
```

Run: `cargo test -p codeloom-core allow_session`
Expected: error de compilación (`no variant ... AllowSession`).

- [ ] **Step 2: Implementar**

1. En `enum Decision`, después de `Allow,` añadir:
   ```rust
       /// Permitir y no volver a preguntar por este `tool:action` en la sesión.
       AllowSession,
   ```
2. En `PermissionService::check`, sustituir la última línea `self.responder.decide(&req).await` por:
   ```rust
        let decision = self.responder.decide(&req).await;
        if decision == Decision::AllowSession {
            self.grant_persistent(&req.tool, &req.action).await;
            return Decision::Allow;
        }
        decision
   ```
3. En `crates/codeloom-core/src/agent/mod.rs`, cambiar la línea
   `crate::permission::Decision::Allow => allowed.push((id, name, input)),` por
   ```rust
                    crate::permission::Decision::Allow
                    | crate::permission::Decision::AllowSession => allowed.push((id, name, input)),
   ```

- [ ] **Step 3: Checks**

Run: `cargo test -p codeloom-core && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/codeloom-core/src
git commit -m "feat(core): permitir una tool para toda la sesión"
```

---

### Task 7: TUI — diálogo de permisos

**Files:**
- Modify: `crates/codeloom-tui/Cargo.toml`, `crates/codeloom-tui/src/lib.rs`, `crates/codeloom-tui/src/app.rs`, `crates/codeloom-tui/src/run.rs`, `crates/codeloom-tui/src/render.rs`
- Create: `crates/codeloom-tui/src/permission_flow.rs`

**Interfaces:**
- Consumes: `batten::state::{Diff, PermissionBody, PermissionChoice, PermissionDialog}`, `batten::widgets::permission`, `batten::widgets::fx::{dialog_open, Effect, FxDuration}`; `codeloom_core::permission::{Decision, PermissionRequest}`.
- Produces: `permission_flow::{dialog_for(&PermissionRequest) -> PermissionDialog, permission_key(&mut PermissionDialog, KeyCode) -> Option<Decision>, PermissionReply}`; `App.permission: Option<PendingPermission>`, `App.fx_pending: bool`, `App::enqueue_permission(req, reply)`, `App::resolve_permission(Decision)`; `run::run(app, prompt_tx, bus_rx, perm_rx)` con un parámetro nuevo `perm_rx: tokio::sync::mpsc::Receiver<(PermissionRequest, PermissionReply)>`.

- [ ] **Step 1: Dependencia**

En `crates/codeloom-tui/Cargo.toml`, en `[dependencies]`, añadir `serde_json.workspace = true`.

- [ ] **Step 2: Crear `crates/codeloom-tui/src/permission_flow.rs`**

```rust
//! Traducción entre peticiones de permiso del core y el diálogo de batten.

use batten::state::{Diff, PermissionBody, PermissionChoice, PermissionDialog};
use codeloom_core::permission::{Decision, PermissionRequest};
use crossterm::event::KeyCode;
use tokio::sync::oneshot;

/// Canal por el que se devuelve la decisión al core.
pub type PermissionReply = oneshot::Sender<Decision>;

/// Diálogo para una petición. `detail` es el JSON del input de la tool.
// ponytail: en `edit` los números de línea son relativos al fragmento
// `old`/`new`, no al fichero; leer el fichero si hace falta precisión.
pub fn dialog_for(req: &PermissionRequest) -> PermissionDialog {
    let parsed: Option<serde_json::Value> = serde_json::from_str(&req.detail).ok();
    let (subtitle, body) = match (req.tool.as_str(), parsed) {
        ("edit", Some(v)) => {
            let field = |k: &str| v[k].as_str().unwrap_or_default().to_string();
            let diff = Diff::new(&field("old"), &field("new"), 3);
            (field("path"), PermissionBody::Diff(diff))
        }
        ("bash", Some(v)) => {
            let cmd = v["command"].as_str().unwrap_or_default().to_string();
            (String::new(), PermissionBody::Text(cmd))
        }
        (_, Some(v)) => {
            let pretty = serde_json::to_string_pretty(&v).unwrap_or_else(|_| req.detail.clone());
            (String::new(), PermissionBody::Text(pretty))
        }
        (_, None) => (String::new(), PermissionBody::Text(req.detail.clone())),
    };
    PermissionDialog::new(req.tool.clone(), subtitle, body)
}

/// Aplica una tecla al diálogo. Devuelve la decisión si la tecla la cierra.
pub fn permission_key(d: &mut PermissionDialog, code: KeyCode) -> Option<Decision> {
    match code {
        KeyCode::Left | KeyCode::BackTab => {
            d.prev();
            None
        }
        KeyCode::Right | KeyCode::Tab => {
            d.next();
            None
        }
        KeyCode::Up => {
            d.scroll_by(-1);
            None
        }
        KeyCode::Down => {
            d.scroll_by(1);
            None
        }
        KeyCode::Enter => Some(match d.choice() {
            PermissionChoice::Allow => Decision::Allow,
            PermissionChoice::AllowSession => Decision::AllowSession,
            PermissionChoice::Deny => Decision::Deny,
        }),
        KeyCode::Char('a') => Some(Decision::Allow),
        KeyCode::Char('s') => Some(Decision::AllowSession),
        KeyCode::Char('d') | KeyCode::Esc => Some(Decision::Deny),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(tool: &str, detail: &str) -> PermissionRequest {
        PermissionRequest {
            tool: tool.into(),
            action: "run".into(),
            detail: detail.into(),
        }
    }

    #[test]
    fn edit_becomes_a_diff_with_the_path() {
        let d = dialog_for(&req("edit", r#"{"path":"a.rs","old":"x\n","new":"y\n"}"#));
        assert_eq!(d.subtitle, "a.rs");
        match d.body {
            PermissionBody::Diff(diff) => assert_eq!((diff.added(), diff.removed()), (1, 1)),
            other => panic!("esperaba diff, llegó {other:?}"),
        }
    }

    #[test]
    fn bash_shows_the_command() {
        let d = dialog_for(&req("bash", r#"{"command":"ls -la"}"#));
        assert_eq!(d.body, PermissionBody::Text("ls -la".into()));
    }

    #[test]
    fn other_tools_show_pretty_json_and_invalid_json_raw() {
        let d = dialog_for(&req("view", r#"{"path":"a.rs"}"#));
        assert!(matches!(&d.body, PermissionBody::Text(t) if t.contains("\"path\": \"a.rs\"")));
        let d = dialog_for(&req("view", "no es json"));
        assert_eq!(d.body, PermissionBody::Text("no es json".into()));
    }

    #[test]
    fn keys_map_to_decisions() {
        let mut d = dialog_for(&req("bash", r#"{"command":"ls"}"#));
        assert_eq!(permission_key(&mut d, KeyCode::Right), None);
        assert_eq!(permission_key(&mut d, KeyCode::Enter), Some(Decision::AllowSession));
        assert_eq!(permission_key(&mut d, KeyCode::Char('a')), Some(Decision::Allow));
        assert_eq!(permission_key(&mut d, KeyCode::Esc), Some(Decision::Deny));
        assert_eq!(permission_key(&mut d, KeyCode::Char('x')), None);
    }
}
```

- [ ] **Step 3: Registrar en `crates/codeloom-tui/src/lib.rs`**

Añadir `pub mod permission_flow;` después de `pub mod app;`.

- [ ] **Step 4: Estado en `App` (`crates/codeloom-tui/src/app.rs`)**

1. Al principio del fichero, junto a los demás `use`, añadir:
   ```rust
   use crate::permission_flow::{dialog_for, PermissionReply};
   use codeloom_core::permission::{Decision, PermissionRequest};
   use std::collections::VecDeque;
   ```
2. Justo antes de `pub struct App {`, añadir:
   ```rust
   /// Petición de permiso en pantalla, con su canal de respuesta.
   pub struct PendingPermission {
       pub dialog: batten::state::PermissionDialog,
       reply: PermissionReply,
   }
   ```
3. En `pub struct App`, después de `pub changes: batten::state::FileChanges,`, añadir:
   ```rust
       /// Diálogo de permiso abierto, si hay.
       pub permission: Option<PendingPermission>,
       /// Peticiones esperando a que se cierre la actual.
       permission_queue: VecDeque<(PermissionRequest, PermissionReply)>,
       /// `true` cuando se acaba de abrir un diálogo y falta lanzar su efecto.
       pub fx_pending: bool,
   ```
4. En `App::new`, después de `changes: batten::state::FileChanges::new(),`, añadir:
   ```rust
               permission: None,
               permission_queue: VecDeque::new(),
               fx_pending: false,
   ```
5. Dentro de `impl App`, añadir los métodos:
   ```rust
       /// Encola una petición; si no hay diálogo abierto, la muestra ya.
       pub fn enqueue_permission(&mut self, req: PermissionRequest, reply: PermissionReply) {
           self.permission_queue.push_back((req, reply));
           self.open_next_permission();
       }

       /// Responde al diálogo abierto y pasa al siguiente de la cola.
       pub fn resolve_permission(&mut self, decision: Decision) {
           if let Some(p) = self.permission.take() {
               let _ = p.reply.send(decision);
           }
           self.open_next_permission();
       }

       fn open_next_permission(&mut self) {
           if self.permission.is_some() {
               return;
           }
           if let Some((req, reply)) = self.permission_queue.pop_front() {
               self.permission = Some(PendingPermission {
                   dialog: dialog_for(&req),
                   reply,
               });
               self.fx_pending = true;
           }
       }
   ```
6. Si `App` tiene otro constructor que construye el struct con un literal (p. ej. `with_context` sin usar `Self::new()`), añadir ahí los mismos tres campos.

- [ ] **Step 5: Teclado y bucle (`crates/codeloom-tui/src/run.rs`)**

1. Al principio de `handle_key`, antes del `match code {`, añadir:
   ```rust
       if code == KeyCode::Char('c') && mods.contains(KeyModifiers::CONTROL) {
           app.should_quit = true;
           return None;
       }
       if let Some(p) = app.permission.as_mut() {
           if let Some(d) = crate::permission_flow::permission_key(&mut p.dialog, code) {
               app.resolve_permission(d);
           }
           return None;
       }
   ```
2. Cambiar la firma de `run` añadiendo un cuarto parámetro:
   ```rust
       mut perm_rx: tokio::sync::mpsc::Receiver<(
           codeloom_core::permission::PermissionRequest,
           crate::permission_flow::PermissionReply,
       )>,
   ```
3. En `run`, justo antes de `while !app.should_quit {`, añadir `let mut fx: Option<batten::widgets::fx::Effect> = None;`.
4. Dentro del bucle, después del `while let Ok(ev) = bus_rx.try_recv() { ... }`, añadir:
   ```rust
           while let Ok((req, reply)) = perm_rx.try_recv() {
               app.enqueue_permission(req, reply);
           }
           if std::mem::take(&mut app.fx_pending) {
               fx = Some(batten::widgets::fx::dialog_open(&app.theme));
           }
   ```
5. Sustituir `terminal.draw(|f| render(f, &app))?;` por:
   ```rust
           terminal.draw(|f| {
               render(f, &app);
               if let (Some(e), Some(_)) = (fx.as_mut(), app.permission.as_ref()) {
                   let area = batten::widgets::permission::area(f.area());
                   e.process(
                       batten::widgets::fx::FxDuration::from_millis(50),
                       f.buffer_mut(),
                       area,
                   );
               }
           })?;
   ```
6. En `mod tests` de `run.rs`, añadir:
   ```rust
       fn open_dialog(app: &mut App) -> tokio::sync::oneshot::Receiver<codeloom_core::permission::Decision> {
           let (tx, rx) = tokio::sync::oneshot::channel();
           app.enqueue_permission(
               codeloom_core::permission::PermissionRequest {
                   tool: "bash".into(),
                   action: "run".into(),
                   detail: r#"{"command":"ls"}"#.into(),
               },
               tx,
           );
           rx
       }

       #[test]
       fn dialog_captures_keys_and_answers() {
           use codeloom_core::permission::Decision;
           let mut app = App::new();
           let mut rx = open_dialog(&mut app);
           assert!(app.fx_pending);
           handle_key(&mut app, KeyCode::Char('a'), KeyModifiers::NONE);
           assert_eq!(app.composer.text(), "");
           assert!(app.permission.is_none());
           assert_eq!(rx.try_recv().unwrap(), Decision::Allow);
       }

       #[test]
       fn esc_denies_without_quitting_and_ctrl_c_still_quits() {
           use codeloom_core::permission::Decision;
           let mut app = App::new();
           let mut rx = open_dialog(&mut app);
           handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
           assert!(!app.should_quit);
           assert_eq!(rx.try_recv().unwrap(), Decision::Deny);
           let _rx2 = open_dialog(&mut app);
           handle_key(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
           assert!(app.should_quit);
       }

       #[test]
       fn queued_requests_are_answered_in_order() {
           use codeloom_core::permission::Decision;
           let mut app = App::new();
           let mut first = open_dialog(&mut app);
           let mut second = open_dialog(&mut app);
           handle_key(&mut app, KeyCode::Char('d'), KeyModifiers::NONE);
           assert!(app.permission.is_some());
           handle_key(&mut app, KeyCode::Char('s'), KeyModifiers::NONE);
           assert_eq!(first.try_recv().unwrap(), Decision::Deny);
           assert_eq!(second.try_recv().unwrap(), Decision::AllowSession);
       }
   ```

- [ ] **Step 6: Pintar el diálogo (`crates/codeloom-tui/src/render.rs`)**

1. Al final de `pub fn render`, después de `draw_composer(frame, rows[2], app);`, añadir:
   ```rust
       if let Some(p) = &app.permission {
           batten::widgets::permission::render(frame, area, &p.dialog, &app.theme);
       }
   ```
2. En `mod tests`, añadir:
   ```rust
       #[test]
       fn shows_permission_dialog_on_top() {
           let mut app = App::new();
           let (tx, _rx) = tokio::sync::oneshot::channel();
           app.enqueue_permission(
               codeloom_core::permission::PermissionRequest {
                   tool: "bash".into(),
                   action: "run".into(),
                   detail: r#"{"command":"cargo test"}"#.into(),
               },
               tx,
           );
           let content = buffer_of(&app, 120, 40);
           assert!(content.contains("Permiso"));
           assert!(content.contains("cargo test"));
           assert!(content.contains("Denegar"));
       }
   ```

- [ ] **Step 7: Checks**

Run: `cargo test -p codeloom-tui && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: `codeloom-tui` PASA. `codeloom-cli` **no compila todavía** por la firma nueva de `run` (lo arregla la Task 8): si `clippy --workspace` falla **solo** por `codeloom-cli` con "this function takes 4 arguments but 3 arguments were supplied", es lo esperado; ejecutar en su lugar `cargo clippy -p codeloom-tui --all-targets -- -D warnings` y continuar.

- [ ] **Step 8: Commit**

```bash
git add crates/codeloom-tui Cargo.lock
git commit -m "feat(tui): diálogo de permisos con cola y efecto de apertura"
```

---

### Task 8: CLI — conectar el responder y `--yolo`

**Files:**
- Modify: `crates/codeloom-cli/src/main.rs`

- [ ] **Step 1: Imports**

Cambiar `use codeloom_core::permission::PermissionService;` por:

```rust
use codeloom_core::permission::{DenyAll, PermissionService};
use codeloom_tui::widgets::permission::UiPermissionResponder;
```

- [ ] **Step 2: Servicio de permisos**

Sustituir la línea `let permission = Arc::new(PermissionService::allow_all());` por:

```rust
    let (perm_tx, perm_rx) = tokio::sync::mpsc::channel(8);
    let permission = Arc::new(if cli.print.is_some() {
        // ponytail: `-p` no es interactivo; sin `--yolo` se deniega todo.
        if cli.yolo {
            PermissionService::allow_all()
        } else {
            PermissionService::with_responder(Box::new(DenyAll))
        }
    } else {
        PermissionService::with_responder(Box::new(UiPermissionResponder::new(perm_tx, cli.yolo)))
    });
```

- [ ] **Step 3: Quitar el descarte y pasar el receptor**

1. Borrar la línea `let _ = cli.yolo;`.
2. En la llamada `codeloom_tui::run::run(...)`, añadir `perm_rx,` como cuarto argumento después de `bus_rx,`.

- [ ] **Step 4: Checks**

Run: `cargo build && cargo test && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS en todo el workspace.

- [ ] **Step 5: Commit**

```bash
git add crates/codeloom-cli/src/main.rs
git commit -m "feat(cli): pedir permiso por cada tool y respetar --yolo"
```

---

### Task 9: Verificación final (la hace el controlador, no el subagente)

- [ ] `cargo build && cargo test && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
- [ ] Manual (usuario): `cargo run -p batten --example showcase` → demo `permission`; `cargo run -p codeloom-cli --bin loom` → pedir algo que use `bash` y comprobar que aparece el diálogo; `--yolo` no pregunta.
