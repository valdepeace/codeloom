# Transcript por entradas, caché de layout y render virtual — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Coste por frame proporcional a lo visible más lo que cambió; sin límite de 65 535 filas; la lectura no salta al cambiar el ancho; loom deja de reconstruir el transcript en cada frame.

**Architecture:** `Transcript` pasa a ser una lista de entradas con revisión (re-deriva solo la entrada que cambia). `widgets::transcript::Layout` cachea alturas por entrada y ancho. `render` pinta solo las líneas visibles y mantiene un ancla en `Viewport`. loom y la demo `chat` actualizan el transcript por eventos (`push_entry`/`append`).

**Tech Stack:** Rust, `ratatui` 0.30 (`unstable-rendered-line-info`), `pulldown-cmark`.

**Spec:** `docs/specs/2026-10-16-transcript-virtual-design.md`

## Global Constraints

- `batten::state` sin `ratatui`/`crossterm`.
- Comentarios en español. Mensajes de commit: los indicados, tal cual.
- Cada commit deja el workspace compilando y pasa: `cargo test -p batten`, `cargo test -p batten --examples`, `cargo test -p codeloom-tui`, `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`.
- **Entorno (Git Bash en Windows):** antes de cualquier `cargo`, en el mismo comando:
  `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe'`.
  No ejecutar `cargo build`/`cargo test` sin `-p` (el usuario puede tener `loom.exe` abierto). No matar procesos. No crear directorios de target temporales.
- No hacer `git push`. No tocar ficheros fuera de los listados. No editar Rust con scripts.

## Review Focus

- `locate` con entradas vacías (markdown vacío al empezar un streaming). Test en Task 1.
- La caché no se invalida cada frame al alternar entre ancho completo y ancho menos la scrollbar. Test en Task 2 (`only_changed_entries_are_measured`).
- Más de 65 535 filas alcanzables. Test en Task 2.
- Cambio de ancho leyendo arriba: la línea del ancla sigue arriba. Test en Task 2.
- loom: deltas consecutivos = una entrada; tras un mensaje de usuario, una nueva. Test en Task 4.

---

### Task 1: `Transcript` por entradas y ancla en `Viewport`

**Files:**
- Modify: `crates/batten/src/state/transcript.rs`, `crates/batten/src/state/viewport.rs`, `crates/batten/src/state/mod.rs`

**Interfaces:**
- Produces: `batten::state::{Format { Plain, Markdown }, EntryId}`; `Transcript::{push_entry(kind, format, text) -> EntryId, append(id, &str), entry_count(), entry_rev(i) -> u64, entry_lines(i) -> &[Line], entry_start(i) -> usize, locate(line) -> Option<(usize, usize)>}`; se mantienen `new, push(Line), len, is_empty, line, iter`; **se elimina** `last_mut`. `Viewport::{restore_top(top), set_anchor(line, row), anchor() -> Option<(usize, usize)>}`.

- [ ] **Step 1: Sustituir `Transcript` (`crates/batten/src/state/transcript.rs`)**

Sustituir el bloque desde `#[derive(Debug, Default, Clone)]` + `pub struct Transcript {` hasta el final del `impl Transcript { ... }` (todo lo que define `Transcript`) por:

```rust
/// Cómo se derivan las líneas de una entrada a partir de su texto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Una línea por línea de texto.
    Plain,
    /// `markdown::parse`.
    Markdown,
}

/// Identificador estable de una entrada (las entradas no se borran).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryId(usize);

#[derive(Debug, Clone)]
struct Entry {
    kind: LineKind,
    format: Format,
    source: String,
    lines: Vec<Line>,
    rev: u64,
}

impl Entry {
    fn derive(&mut self) {
        self.lines = match self.format {
            Format::Plain => self
                .source
                .lines()
                .map(|l| Line::new(self.kind, l))
                .collect(),
            Format::Markdown => super::markdown::parse(&self.source, self.kind),
        };
    }
}

/// Transcript como lista de entradas (mensajes) con revisión. Cada entrada
/// guarda su texto y las líneas derivadas; `append` re-deriva solo la suya.
/// También se ve como una lista de líneas globales (`len`, `line`, `iter`).
#[derive(Debug, Default, Clone)]
pub struct Transcript {
    entries: Vec<Entry>,
    /// Línea global donde empieza cada entrada.
    starts: Vec<usize>,
    total_lines: usize,
    next_rev: u64,
}

impl Transcript {
    pub fn new() -> Self {
        Self::default()
    }

    fn bump(&mut self) -> u64 {
        self.next_rev += 1;
        self.next_rev
    }

    fn push_built(&mut self, mut entry: Entry) -> EntryId {
        entry.rev = self.bump();
        self.starts.push(self.total_lines);
        self.total_lines += entry.lines.len();
        self.entries.push(entry);
        EntryId(self.entries.len() - 1)
    }

    /// Nueva entrada a partir de su texto.
    pub fn push_entry(&mut self, kind: LineKind, format: Format, text: impl Into<String>) -> EntryId {
        let mut entry = Entry {
            kind,
            format,
            source: text.into(),
            lines: Vec::new(),
            rev: 0,
        };
        entry.derive();
        self.push_built(entry)
    }

    /// Una línea ya construida como entrada propia (no admite `append`).
    pub fn push(&mut self, line: Line) {
        let entry = Entry {
            kind: line.kind,
            format: Format::Plain,
            source: line.text.clone(),
            lines: vec![line],
            rev: 0,
        };
        self.push_built(entry);
    }

    /// Añade texto a una entrada y re-deriva solo sus líneas.
    pub fn append(&mut self, id: EntryId, delta: &str) {
        let rev = self.bump();
        let Some(entry) = self.entries.get_mut(id.0) else {
            return;
        };
        entry.source.push_str(delta);
        entry.derive();
        entry.rev = rev;
        self.reindex_from(id.0);
    }

    fn reindex_from(&mut self, from: usize) {
        let mut acc = self.starts.get(from).copied().unwrap_or(0);
        for i in from..self.entries.len() {
            self.starts[i] = acc;
            acc += self.entries[i].lines.len();
        }
        self.total_lines = acc;
    }

    /// Número de líneas globales.
    pub fn len(&self) -> usize {
        self.total_lines
    }

    pub fn is_empty(&self) -> bool {
        self.total_lines == 0
    }

    /// Línea global `i`.
    pub fn line(&self, i: usize) -> Option<&Line> {
        let (entry, offset) = self.locate(i)?;
        self.entries[entry].lines.get(offset)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Line> {
        self.entries.iter().flat_map(|e| e.lines.iter())
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Revisión de la entrada `i` (sube cada vez que cambia).
    pub fn entry_rev(&self, i: usize) -> u64 {
        self.entries.get(i).map(|e| e.rev).unwrap_or(0)
    }

    pub fn entry_lines(&self, i: usize) -> &[Line] {
        self.entries
            .get(i)
            .map(|e| e.lines.as_slice())
            .unwrap_or(&[])
    }

    /// Línea global donde empieza la entrada `i`.
    pub fn entry_start(&self, i: usize) -> usize {
        self.starts.get(i).copied().unwrap_or(self.total_lines)
    }

    /// `(entrada, línea dentro de la entrada)` de una línea global.
    pub fn locate(&self, line: usize) -> Option<(usize, usize)> {
        if line >= self.total_lines {
            return None;
        }
        let entry = self.starts.partition_point(|&s| s <= line) - 1;
        Some((entry, line - self.starts[entry]))
    }
}
```

- [ ] **Step 2: Tests de `Transcript`**

En el `mod tests` del mismo fichero, **borrar** el test `last_mut_edits_in_place` y añadir:

```rust
    #[test]
    fn entries_index_global_lines() {
        let mut t = Transcript::new();
        t.push_entry(LineKind::User, Format::Plain, "uno\ndos");
        t.push_entry(LineKind::Assistant, Format::Plain, "tres");
        assert_eq!(t.len(), 3);
        assert_eq!(t.entry_count(), 2);
        assert_eq!(t.entry_start(1), 2);
        assert_eq!(t.line(2).unwrap().plain(), "tres");
        assert_eq!(t.locate(1), Some((0, 1)));
        assert_eq!(t.locate(2), Some((1, 0)));
        assert_eq!(t.locate(3), None);
    }

    #[test]
    fn append_rederives_only_its_entry() {
        let mut t = Transcript::new();
        t.push_entry(LineKind::User, Format::Plain, "hola");
        let id = t.push_entry(LineKind::Assistant, Format::Markdown, "# Tí");
        let (r0, r1) = (t.entry_rev(0), t.entry_rev(1));
        t.append(id, "tulo\n\n- a\n- b");
        assert_eq!(t.entry_rev(0), r0);
        assert!(t.entry_rev(1) > r1);
        let texts: Vec<&str> = t.iter().map(|l| l.plain()).collect();
        assert_eq!(texts, vec!["hola", "Título", "", "• a", "• b"]);
        assert_eq!(t.len(), 5);
    }

    #[test]
    fn append_in_the_middle_reindexes_the_rest() {
        let mut t = Transcript::new();
        let a = t.push_entry(LineKind::Assistant, Format::Plain, "a");
        t.push_entry(LineKind::User, Format::Plain, "z");
        t.append(a, "\nb");
        assert_eq!(t.entry_start(1), 2);
        assert_eq!(t.line(2).unwrap().plain(), "z");
    }

    #[test]
    fn empty_entries_do_not_break_locate() {
        let mut t = Transcript::new();
        t.push_entry(LineKind::Assistant, Format::Markdown, "");
        t.push_entry(LineKind::User, Format::Plain, "x");
        assert_eq!(t.len(), 1);
        assert_eq!(t.locate(0), Some((1, 0)));
        assert_eq!(t.line(0).unwrap().plain(), "x");
    }
```

- [ ] **Step 3: Re-exports (`crates/batten/src/state/mod.rs`)**

Cambiar `pub use transcript::{Line, LineKind, SpanSpec, StyleToken, Transcript};` por
`pub use transcript::{EntryId, Format, Line, LineKind, SpanSpec, StyleToken, Transcript};`.

- [ ] **Step 4: Ancla en `Viewport` (`crates/batten/src/state/viewport.rs`)**

1. En `pub struct Viewport`, después de `rows: usize,`, añadir:
   ```rust
       /// `(línea, fila dentro de la línea)` que estaba arriba en el último pintado.
       anchor: Option<(usize, usize)>,
   ```
   y en `impl Default for Viewport`, después de `rows: 0,`, añadir `anchor: None,`.
2. En `set_geometry`, sustituir el comentario `// limitación: en un resize solo se acota ...` (dos líneas) por:
   ```rust
        // Tras un cambio de ancho, el widget restaura antes `top` desde el ancla.
   ```
3. Dentro de `impl Viewport`, después de `ensure_visible`, añadir:
   ```rust
    /// Fija `top` sin tocar el seguimiento (`set_geometry` lo acota después).
    pub fn restore_top(&mut self, top: usize) {
        self.top = top;
    }

    /// Lo llama el widget al pintar: qué línea quedó arriba y cuántas filas
    /// de ella se saltaron.
    pub fn set_anchor(&mut self, line: usize, row: usize) {
        self.anchor = Some((line, row));
    }

    pub fn anchor(&self) -> Option<(usize, usize)> {
        self.anchor
    }
   ```
4. En su `mod tests`:
   ```rust
    #[test]
    fn restore_top_is_clamped_by_geometry() {
        let mut v = vp(100, 10);
        v.scroll_up(5);
        v.restore_top(500);
        v.set_geometry(100, 10);
        assert_eq!(v.window(), 90..100);
        v.set_anchor(3, 1);
        assert_eq!(v.anchor(), Some((3, 1)));
    }
   ```

- [ ] **Step 5: Checks**

Run: `cargo test -p batten && cargo test -p batten --examples && cargo test -p codeloom-tui && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (ningún consumidor usa `last_mut`; si alguno lo usa, reportarlo).

- [ ] **Step 6: Commit**

```bash
git add crates/batten/src/state
git commit -m "feat(batten): transcript por entradas con revisión y ancla en el viewport"
```

---

### Task 2: `Layout` y render virtual (con todos los llamantes)

**Files:**
- Modify: `crates/batten/src/widgets/transcript.rs`, `crates/batten/examples/showcase/demos/transcript.rs`, `crates/batten/examples/showcase/demos/chat.rs`, `crates/codeloom-tui/src/app.rs`, `crates/codeloom-tui/src/render.rs`

**Interfaces:**
- Produces: `batten::widgets::transcript::Layout::{new, sync(&mut self, &Transcript, u16, &Theme), total_rows, measured, width, row_of(line), height_of(line), line_at_row(row) -> Option<(usize, usize)>}`; `batten::widgets::transcript::render(frame, area, &Transcript, &mut Viewport, &mut Layout, Option<&Selection>, &Theme)`. **Se eliminan** la antigua `render` de 5 argumentos y `render_with_selection`.

- [ ] **Step 1: Imports del widget**

En `crates/batten/src/widgets/transcript.rs`, cambiar `use crate::state::{Line, LineKind, StyleToken, Transcript};` por
`use crate::state::{Line, LineKind, Selection, StyleToken, Transcript, Viewport};`.

- [ ] **Step 2: `Layout` y `render`**

Sustituir las funciones `pub fn render(...)` y `pub fn render_with_selection(...)` (ambas enteras) por:

```rust
/// Altura en filas de una línea a `width` columnas (mínimo 1).
fn measure(line: &Line, width: u16, theme: &Theme) -> usize {
    Paragraph::new(render_line(line, theme))
        .wrap(Wrap { trim: false })
        .line_count(width)
        .max(1)
}

#[derive(Debug, Clone)]
struct EntryLayout {
    rev: u64,
    heights: Vec<usize>,
}

/// Caché de alturas (filas tras el wrap) por entrada y ancho. La guarda quien
/// usa el widget; `sync` solo re-mide las entradas cuya revisión cambió.
#[derive(Debug, Default, Clone)]
pub struct Layout {
    width: u16,
    entries: Vec<EntryLayout>,
    /// Por línea global: fila donde empieza y altura.
    row_starts: Vec<usize>,
    heights: Vec<usize>,
    total_rows: usize,
    measured: usize,
}

impl Layout {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pone la caché al día para `width`.
    pub fn sync(&mut self, transcript: &Transcript, width: u16, theme: &Theme) {
        if width != self.width {
            self.entries.clear();
            self.width = width;
        }
        self.entries.truncate(transcript.entry_count());
        let mut first_changed = None;
        for i in 0..transcript.entry_count() {
            let rev = transcript.entry_rev(i);
            if self.entries.get(i).is_some_and(|e| e.rev == rev) {
                continue;
            }
            let heights = transcript
                .entry_lines(i)
                .iter()
                .map(|l| measure(l, width, theme))
                .collect();
            self.measured += 1;
            let layout = EntryLayout { rev, heights };
            if i < self.entries.len() {
                self.entries[i] = layout;
            } else {
                self.entries.push(layout);
            }
            first_changed.get_or_insert(i);
        }
        if let Some(entry) = first_changed {
            self.reindex_from(entry, transcript.entry_start(entry));
        }
    }

    fn reindex_from(&mut self, entry: usize, line: usize) {
        self.row_starts.truncate(line);
        self.heights.truncate(line);
        let mut acc = match line {
            0 => 0,
            _ => self.row_starts[line - 1] + self.heights[line - 1],
        };
        for e in &self.entries[entry..] {
            for &h in &e.heights {
                self.row_starts.push(acc);
                self.heights.push(h);
                acc += h;
            }
        }
        self.total_rows = acc;
    }

    pub fn total_rows(&self) -> usize {
        self.total_rows
    }

    /// Entradas medidas desde la creación (para tests de rendimiento).
    pub fn measured(&self) -> usize {
        self.measured
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    /// Fila global donde empieza la línea `line`.
    pub fn row_of(&self, line: usize) -> usize {
        self.row_starts.get(line).copied().unwrap_or(self.total_rows)
    }

    /// Filas que ocupa la línea `line`.
    pub fn height_of(&self, line: usize) -> usize {
        self.heights.get(line).copied().unwrap_or(0)
    }

    /// `(línea, fila dentro de la línea)` que ocupa la fila `row`.
    pub fn line_at_row(&self, row: usize) -> Option<(usize, usize)> {
        if self.heights.is_empty() {
            return None;
        }
        let row = row.min(self.total_rows.saturating_sub(1));
        let line = self.row_starts.partition_point(|&s| s <= row) - 1;
        Some((line, row - self.row_starts[line]))
    }
}

/// Pinta el transcript: solo las líneas visibles. `layout` cachea alturas;
/// `viewport` recibe la geometría y el ancla; `selection` se resalta y su
/// extremo se mantiene visible.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    transcript: &Transcript,
    viewport: &mut Viewport,
    layout: &mut Layout,
    selection: Option<&Selection>,
    theme: &Theme,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let rows = area.height as usize;
    let prev_width = layout.width();
    // Ancho de texto: si ya se medía sin la columna de la scrollbar y sigue
    // desbordando, se mantiene (así no se invalida la caché cada frame).
    let narrow = area.width.saturating_sub(1).max(1);
    let text_w = if area.width >= 2 && prev_width == narrow {
        layout.sync(transcript, narrow, theme);
        if layout.total_rows() > rows {
            narrow
        } else {
            layout.sync(transcript, area.width, theme);
            area.width
        }
    } else {
        layout.sync(transcript, area.width, theme);
        if area.width >= 2 && layout.total_rows() > rows {
            layout.sync(transcript, narrow, theme);
            narrow
        } else {
            area.width
        }
    };
    let overflow = text_w < area.width;

    if prev_width != 0 && prev_width != text_w && !viewport.is_following() {
        if let Some((line, row)) = viewport.anchor() {
            let h = layout.height_of(line).max(1);
            viewport.restore_top(layout.row_of(line) + row.min(h - 1));
        }
    }
    viewport.set_geometry(layout.total_rows(), rows);
    if let (Some(s), false) = (selection, transcript.is_empty()) {
        let head = s.head().min(transcript.len() - 1);
        let start = layout.row_of(head);
        viewport.ensure_visible(start, start + layout.height_of(head));
    }

    let text_area = Rect {
        width: text_w,
        ..area
    };
    if let Some((first, skip)) = layout.line_at_row(viewport.top()) {
        viewport.set_anchor(first, skip);
        let sel_style = theme.styles.selection;
        let mut lines = Vec::new();
        let mut filled = 0;
        let mut i = first;
        while i < transcript.len() && filled < rows + skip {
            if let Some(line) = transcript.line(i) {
                let mut out = render_line(line, theme);
                if selection.is_some_and(|s| s.contains(i)) {
                    for tl in &mut out {
                        tl.style = tl.style.patch(sel_style);
                    }
                }
                lines.extend(out);
            }
            filled += layout.height_of(i);
            i += 1;
        }
        let skip = u16::try_from(skip).unwrap_or(u16::MAX);
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .scroll((skip, 0)),
            text_area,
        );
    }
    if overflow {
        let bar = Rect {
            x: area.x + area.width - 1,
            width: 1,
            ..area
        };
        crate::widgets::scrollbar::render(frame, bar, viewport.scroll(), theme);
    }
}
```

- [ ] **Step 3: Tests del widget**

En `mod tests` de `crates/batten/src/widgets/transcript.rs`:

1. Sustituir las funciones auxiliares `draw` y `buffer` por:
   ```rust
       fn draw(
           transcript: &Transcript,
           vp: &mut Viewport,
           layout: &mut Layout,
           w: u16,
           h: u16,
       ) -> String {
           let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
           term.draw(|f| render(f, f.area(), transcript, vp, layout, None, &Theme::test()))
               .unwrap();
           term.backend()
               .buffer()
               .content
               .iter()
               .map(|c| c.symbol())
               .collect()
       }

       fn buffer(transcript: &Transcript, w: u16, h: u16) -> String {
           draw(transcript, &mut Viewport::new(), &mut Layout::new(), w, h)
       }
   ```
2. En cada test existente que llame a `draw(&t, &mut vp, W, H)`, crear `let mut layout = Layout::new();` junto a `vp` y pasar `&mut layout` como tercer argumento (el mismo `layout` en todas las llamadas del test).
3. En `selected_lines_are_highlighted` y `the_selection_head_is_scrolled_into_view`, sustituir la llamada `render_with_selection(f, f.area(), &t, &mut vp, Some(&sel), &Theme::test())` por `render(f, f.area(), &t, &mut vp, &mut layout, Some(&sel), &Theme::test())`, declarando `let mut layout = Layout::new();` antes del `term.draw`. Si importan `crate::state::Selection` dentro del test, mantenerlo.
4. Añadir:
   ```rust
       fn long_transcript(n: usize) -> (Transcript, crate::state::EntryId) {
           let mut t = Transcript::new();
           for i in 0..n {
               t.push_entry(LineKind::Assistant, crate::state::Format::Plain, format!("l{i:05}"));
           }
           let last = t.push_entry(LineKind::Assistant, crate::state::Format::Plain, "fin");
           (t, last)
       }

       #[test]
       fn only_changed_entries_are_measured() {
           let (mut t, last) = long_transcript(1000);
           let (mut vp, mut layout) = (Viewport::new(), Layout::new());
           draw(&t, &mut vp, &mut layout, 40, 10);
           draw(&t, &mut vp, &mut layout, 40, 10);
           let before = layout.measured();
           t.append(last, " y más");
           let out = draw(&t, &mut vp, &mut layout, 40, 10);
           assert_eq!(layout.measured(), before + 1);
           assert!(out.contains("fin y más"));
       }

       #[test]
       fn width_change_remeasures_everything() {
           let (t, _) = long_transcript(100);
           let (mut vp, mut layout) = (Viewport::new(), Layout::new());
           draw(&t, &mut vp, &mut layout, 40, 10);
           let before = layout.measured();
           draw(&t, &mut vp, &mut layout, 60, 10);
           assert!(layout.measured() >= before + 101);
       }

       #[test]
       fn line_at_row_handles_multi_row_lines() {
           let mut t = Transcript::new();
           t.push(Line::new(LineKind::Assistant, "corta"));
           t.push(Line::new(LineKind::Assistant, "palabra ".repeat(10)));
           t.push(Line::new(LineKind::Assistant, "otra"));
           let mut layout = Layout::new();
           layout.sync(&t, 20, &Theme::test());
           let h1 = layout.height_of(1);
           assert!(h1 > 1);
           assert_eq!(layout.line_at_row(0), Some((0, 0)));
           assert_eq!(layout.line_at_row(1), Some((1, 0)));
           assert_eq!(layout.line_at_row(h1), Some((1, h1 - 1)));
           assert_eq!(layout.line_at_row(h1 + 1), Some((2, 0)));
           assert_eq!(layout.line_at_row(10_000), Some((2, 0)));
       }

       #[test]
       fn rows_beyond_u16_are_reachable() {
           let mut t = Transcript::new();
           for i in 0..70_000 {
               t.push(Line::new(LineKind::Banner, format!("l{i:05}")));
           }
           let (mut vp, mut layout) = (Viewport::new(), Layout::new());
           let out = draw(&t, &mut vp, &mut layout, 20, 5);
           assert!(out.contains("l69999"));
           vp.to_top();
           let out = draw(&t, &mut vp, &mut layout, 20, 5);
           assert!(out.contains("l00000"));
       }

       #[test]
       fn width_change_keeps_the_anchor_line_on_top() {
           let mut t = Transcript::new();
           for i in 0..200 {
               t.push(Line::new(
                   LineKind::Assistant,
                   format!("a{i:03} {}", "xx ".repeat(15)),
               ));
           }
           let (mut vp, mut layout) = (Viewport::new(), Layout::new());
           draw(&t, &mut vp, &mut layout, 40, 10);
           vp.scroll_up(100);
           draw(&t, &mut vp, &mut layout, 40, 10);
           let (line, _) = vp.anchor().unwrap();
           draw(&t, &mut vp, &mut layout, 25, 10);
           assert_eq!(vp.anchor().unwrap().0, line);
       }
   ```

- [ ] **Step 4: Demo `transcript` (`crates/batten/examples/showcase/demos/transcript.rs`)**

1. Cambiar el import `use batten::widgets::{transcript, Theme};` por `use batten::widgets::{transcript, transcript::Layout, Theme};` (si el fichero importa `transcript` de otra forma, añadir `use batten::widgets::transcript::Layout;`).
2. En `pub struct TranscriptDemo`, añadir `layout: Layout,`; en `new()`, añadir `layout: Layout::new(),`.
3. Sustituir la llamada `transcript::render_with_selection(f, body, &self.transcript, &mut self.viewport, self.selection.as_ref(), theme);` por:
   ```rust
        transcript::render(
            f,
            body,
            &self.transcript,
            &mut self.viewport,
            &mut self.layout,
            self.selection.as_ref(),
            theme,
        );
   ```

- [ ] **Step 5: Demo `chat` (`crates/batten/examples/showcase/demos/chat.rs`)**

1. Añadir `use batten::widgets::transcript::Layout;`.
2. En `pub struct ChatDemo`, añadir `layout: Layout,`; en `ChatDemo::new`, añadir `layout: Layout::new(),`.
3. Sustituir `transcript::render(f, rows[1], &view, &mut self.viewport, theme);` por
   `transcript::render(f, rows[1], &view, &mut self.viewport, &mut self.layout, None, theme);`.

- [ ] **Step 6: loom (`crates/codeloom-tui`)**

1. `src/app.rs`: en `pub struct App`, después de `pub viewport: batten::state::Viewport,`, añadir
   `pub layout: batten::widgets::transcript::Layout,`; en `App::new`, después de `viewport: batten::state::Viewport::new(),`, añadir `layout: batten::widgets::transcript::Layout::new(),`.
2. `src/render.rs`, en `draw_transcript`: sustituir la llamada a `batten::widgets::transcript::render_with_selection(...)` por:
   ```rust
    batten::widgets::transcript::render(
        frame,
        inner,
        &t,
        &mut app.viewport,
        &mut app.layout,
        app.selection.as_ref(),
        &app.theme,
    );
   ```

- [ ] **Step 7: Checks**

Run: `cargo test -p batten && cargo test -p batten --examples && cargo test -p codeloom-tui && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS. Si algún test de los existentes falla, reportar el assert exacto (sin cambiar valores esperados). `rows_beyond_u16_are_reachable` puede tardar unos segundos en debug: es normal.

- [ ] **Step 8: Commit**

```bash
git add crates/batten crates/codeloom-tui
git commit -m "feat(batten): caché de layout y render virtual del transcript"
```

---

### Task 3: demo `chat` — streaming con `append`

**Files:**
- Modify: `crates/batten/examples/showcase/demos/chat.rs`

- [ ] **Step 1: Estado**

1. En el `use batten::state::{...}`, quitar `Stream` y añadir `EntryId` y `Format`.
2. En `pub struct ChatDemo`, sustituir el campo `stream: Stream,` por `answer: Option<EntryId>,`; en `new()`, sustituir `stream: Stream::new(),` por `answer: None,`.

- [ ] **Step 2: Pasos del guion**

En `step`, sustituir los brazos `Step::Delta(s) => ...` y `Step::Flush => { ... }` por:

```rust
            Step::Delta(s) => match self.answer {
                Some(id) => self.transcript.append(id, &s),
                None => {
                    let id = self
                        .transcript
                        .push_entry(LineKind::Assistant, Format::Markdown, s);
                    self.answer = Some(id);
                }
            },
            Step::Flush => self.answer = None,
```

- [ ] **Step 3: Pintado sin copia**

1. Borrar el método `fn view(&self) -> Transcript { ... }` entero.
2. En `render`, borrar la línea `let view = self.view();` y cambiar `&view` por `&self.transcript` en la llamada a `transcript::render`.
3. Si `markdown` ya no se usa en el fichero salvo en tests, mover su `use` al `mod tests` (o dejarlo si el compilador no avisa).

- [ ] **Step 4: Tests**

En `enter_starts_a_turn_that_streams_and_flushes`, sustituir las líneas
`assert!(!d.stream.is_empty());`, `assert_eq!(d.transcript.len(), 1);` y `assert!(d.view().len() >= 2);` por:

```rust
        assert!(d.answer.is_some());
        assert!(d.transcript.len() >= 2);
```

y la línea `assert!(d.stream.is_empty());` por `assert!(d.answer.is_none());`. El resto del test no cambia.

- [ ] **Step 5: Checks**

Run: `cargo test -p batten --examples && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/batten/examples/showcase/demos/chat.rs
git commit -m "feat(batten): la demo chat hace streaming con append sobre una entrada"
```

---

### Task 4: loom — transcript persistente por eventos

**Files:**
- Modify: `crates/codeloom-tui/src/app.rs`, `crates/codeloom-tui/src/render.rs`, `crates/codeloom-tui/src/run.rs`

**Interfaces:**
- Produces: `App.transcript: batten::state::Transcript`, `App::push_user(&mut self, text: &str)`, `App::push_assistant(&mut self, delta: &str)`. **Se eliminan** `TranscriptItem`, `App.items` y `App::transcript()`.

- [ ] **Step 1: `app.rs`**

1. Borrar `pub struct TranscriptItem { ... }` (con su `#[derive(...)]`).
2. En `pub struct App`, sustituir `pub items: Vec<TranscriptItem>,` por:
   ```rust
       /// Transcript persistente: se actualiza con los eventos, no por frame.
       pub transcript: batten::state::Transcript,
       /// Entrada de la respuesta en curso (los deltas se añaden a ella).
       assistant: Option<batten::state::EntryId>,
   ```
   y en `App::new` sustituir `items: Vec::new(),` por
   `transcript: batten::state::Transcript::new(),` y `assistant: None,`.
3. Añadir en `impl App`:
   ```rust
       /// Mensaje del usuario: entrada nueva; la siguiente respuesta empieza otra.
       pub fn push_user(&mut self, text: &str) {
           use batten::state::{Format, LineKind};
           self.transcript.push_entry(LineKind::User, Format::Plain, text);
           self.assistant = None;
       }

       /// Delta de la respuesta: se añade a la entrada en curso (markdown).
       pub fn push_assistant(&mut self, delta: &str) {
           use batten::state::{Format, LineKind};
           match self.assistant {
               Some(id) => self.transcript.append(id, delta),
               None => {
                   let id = self
                       .transcript
                       .push_entry(LineKind::Assistant, Format::Markdown, delta);
                   self.assistant = Some(id);
               }
           }
       }
   ```
4. En `apply`:
   - sustituir el brazo `LiveEvent::TurnEnded => self.status = "ready".into(),` por
     ```rust
                LiveEvent::TurnEnded => {
                    self.status = "ready".into();
                    self.assistant = None;
                }
     ```
   - sustituir el cuerpo entero del brazo `LiveEvent::Delta { text } => { ... }` por `LiveEvent::Delta { text } => self.push_assistant(&text),`;
   - en el brazo `LiveEvent::MessageAdded(m)`, sustituir el `self.items.push(TranscriptItem { role: "you", text: m.text() });` por `self.push_user(&m.text());`.
5. Borrar el método `pub fn transcript(&self) -> batten::state::Transcript { ... }` entero.
6. Tests de `app.rs`:
   - en `deltas_accumulate_into_one_item`, sustituir los dos asserts por
     ```rust
        assert_eq!(app.transcript.entry_count(), 1);
        assert_eq!(app.transcript.line(0).unwrap().plain(), "ab");
     ```
   - en `user_line_comes_from_the_event_not_from_submit`, sustituir
     `assert!(app.items.iter().all(|i| i.role != "you"));` por `assert!(app.transcript.is_empty());` y
     `assert_eq!(app.items.iter().filter(|i| i.role == "you").count(), 1);` por
     `assert_eq!(app.transcript.line(0).unwrap().kind, batten::state::LineKind::User);`;
   - añadir:
     ```rust
        #[test]
        fn a_user_message_starts_a_new_answer_entry() {
            let mut app = App::new();
            app.push_assistant("uno");
            app.push_user("pregunta");
            app.push_assistant("dos");
            assert_eq!(app.transcript.entry_count(), 3);
            assert_eq!(app.transcript.line(2).unwrap().plain(), "dos");
        }
     ```

- [ ] **Step 2: `render.rs`**

1. En `draw_transcript`, borrar `let t = app.transcript();` y cambiar `&t` por `&app.transcript` en la llamada a `render`.
2. En `mod tests`: cambiar `use crate::app::{App, TranscriptItem};` por `use crate::app::App;`, y sustituir cada
   `app.items.push(TranscriptItem { role: "you", text: X })` por `app.push_user(&X)` (con `X` el texto: `"hello"`, `format!("msg-{i:02}")`), y el de `role: "CodeLoom"` en `assistant_markdown_is_rendered` por `app.push_assistant("## Hecho\n\n- **uno**\n- `dos`");`.

- [ ] **Step 3: `run.rs`**

1. En `transcript_key`, sustituir `let len = app.transcript().len();` por `let len = app.transcript.len();` y `s.text(&app.transcript())` por `s.text(&app.transcript)`.
2. En el test `select_extend_copy_and_escape`, sustituir el bucle que hace `app.items.push(crate::app::TranscriptItem { role: "you", text: s.into() })` por `app.push_user(s);`.

- [ ] **Step 4: Checks**

Run: `cargo test -p codeloom-tui && cargo test -p batten && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/codeloom-tui/src
git commit -m "feat(tui): transcript persistente actualizado por eventos"
```

---

### Task 5: limpiar la deuda resuelta

**Files:**
- Modify: `docs/specs/2026-10-16-deuda-tecnica.md`

- [ ] **Step 1:** Borrar las filas 1, 2 y 3 de la tabla (render de todo el historial, resize, `u16`) y añadir debajo de la tabla la línea:
  `Resueltas el 2026-10-16: 1, 2 y 3 (ver 2026-10-16-transcript-virtual-design.md).`
- [ ] **Step 2:** Comprobar que ya no quedan esos comentarios: `grep -rn "limitación" crates/batten/src/widgets/transcript.rs crates/batten/src/state/viewport.rs` no debe devolver nada.
- [ ] **Step 3: Commit**

```bash
git add docs/specs/2026-10-16-deuda-tecnica.md
git commit -m "docs: deuda técnica del transcript resuelta"
```
