# batten showcase — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Un ejemplo multi-archivo `showcase` de batten: un storybook navegable en la TUI que muestra y permite probar los 7 widgets más una pantalla `chat`, sin back.

**Architecture:** Un trait `Demo` (una demo por widget) y un estado `Showcase` (catálogo, selección, foco) con un `route` puro y testeable. `main.rs` pone el bucle de eventos (crossterm vía `ratatui::crossterm`) y el layout. Todo vive en `examples/showcase/`; el crate publicable no cambia y sigue sin crossterm.

**Tech Stack:** Rust (edición 2021), `ratatui` 0.30 (con `ratatui::crossterm` en el ejemplo), `batten` (el propio crate).

**Spec:** `docs/specs/2026-10-15-batten-showcase-design.md`

## Global Constraints

- El ejemplo vive en `crates/batten/examples/showcase/` (multi-archivo: `main.rs`).
- `batten` (lib) NO gana dependencias ni código; `state` sigue sin `ratatui` y el crate sin `crossterm`. Solo el ejemplo usa `ratatui::crossterm`.
- Nada de back: cada demo es autónomo (datos falsos).
- Comentarios en español.
- Los tests del ejemplo corren con `cargo test -p batten --examples`.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` y `cargo test` (lib) siguen pasando.
- El filtro `KeyEventKind::Press` se aplica en el enrutado.

## Review Focus

- **Tamaños 0×0, 1×1**: el layout y cada demo deben no entrar en pánico. Test en Task 1 (smoke sobre todo el catálogo).
- **Demo no seleccionado**: no debe animar; `tick` delega solo en `current()`. Verificado estructuralmente (una línea) y por el smoke, que tickea cada demo por separado.
- **Foco**: con foco en el índice, `q` sale y `↑/↓` mueven; con foco en el demo, las teclas van al demo (una `q` tecleada NO sale). Test en Task 1.
- **Índice colapsado** en terminal estrecha (<70): sigue navegable y no entra en pánico. Test en Task 1 (`draw` a 60×20).
- **Estados de tool fuera de orden/duplicados**: `n` en el demo reconstruye el item en el siguiente estado, siempre consistente. Test en Task 3.

---

## File Structure

```
crates/batten/examples/
  showcase/
    main.rs        # bucle de eventos, layout, navegación, salida
    demo.rs        # trait Demo
    app.rs         # Focus, Action, Showcase, route
    demos/
      mod.rs       # catalog()
      spinner.rs
      transcript.rs
      tool_item.rs
      composer.rs
      logo.rs
      sidebar.rs
      theme.rs
      chat.rs
  demo.rs          # SE ELIMINA (Task 9)
```

---

### Task 1: Framework del showcase + demo `spinner`

**Files:**
- Create: `crates/batten/examples/showcase/main.rs`
- Create: `crates/batten/examples/showcase/demo.rs`
- Create: `crates/batten/examples/showcase/app.rs`
- Create: `crates/batten/examples/showcase/demos/mod.rs`
- Create: `crates/batten/examples/showcase/demos/spinner.rs`

**Interfaces:**
- Consumes: `batten::{state, widgets}`, `ratatui`, `ratatui::crossterm`.
- Produces: `trait Demo`; `enum Focus`; `enum Action`; `struct Showcase` (`new`, `len`, `title_at`, `selected`, `focus`, `should_quit`, `next`, `prev`, `toggle_focus`, `current`, `tick`); `fn route(&mut Showcase, KeyEventKind, KeyCode, KeyModifiers) -> Action`; `demos::catalog() -> Vec<Box<dyn Demo>>`.

- [ ] **Step 1: Escribir los tests**

Crea `crates/batten/examples/showcase/app.rs` con:

```rust
//! Estado del showcase: catálogo, selección y foco.

use crate::demo::Demo;
use crate::demos;
use batten::widgets::Anim;
use ratatui::crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Index,
    Demo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    None,
}

pub struct Showcase {
    demos: Vec<Box<dyn Demo>>,
    selected: usize,
    focus: Focus,
    quit: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Demo vacía para tests de navegación, independientes del catálogo real.
    struct Dummy(&'static str);

    const NAMES: [&str; 8] = ["a", "b", "c", "d", "e", "f", "g", "h"];

    impl Demo for Dummy {
        fn title(&self) -> &str {
            self.0
        }
        fn hint(&self) -> &str {
            ""
        }
        fn render(
            &mut self,
            _f: &mut ratatui::Frame,
            _a: ratatui::layout::Rect,
            _t: &batten::widgets::Theme,
        ) {
        }
    }

    fn dummy_show(n: usize) -> Showcase {
        let demos: Vec<Box<dyn Demo>> = (0..n)
            .map(|i| Box::new(Dummy(NAMES[i])) as Box<dyn Demo>)
            .collect();
        Showcase::with_demos(demos)
    }

    #[test]
    fn navigation_clamps_and_resets_focus() {
        let mut show = dummy_show(5);
        assert_eq!(show.selected(), 0);
        show.prev();
        assert_eq!(show.selected(), 0); // clamp arriba
        show.next();
        show.next();
        assert_eq!(show.selected(), 2);
        show.focus_demo_for_test();
        assert_eq!(show.focus(), Focus::Demo);
        show.next();
        assert_eq!(show.focus(), Focus::Index); // navegar resetea el foco
        for _ in 0..100 {
            show.next();
        }
        assert_eq!(show.selected(), show.len() - 1); // clamp abajo
    }

    #[test]
    fn tab_toggles_focus() {
        let mut show = Showcase::new();
        assert_eq!(show.focus(), Focus::Index);
        route(&mut show, KeyEventKind::Press, KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(show.focus(), Focus::Demo);
        route(&mut show, KeyEventKind::Press, KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(show.focus(), Focus::Index);
    }

    #[test]
    fn release_is_ignored() {
        let mut show = Showcase::new();
        let a = route(&mut show, KeyEventKind::Release, KeyCode::Char('q'), KeyModifiers::NONE);
        assert_eq!(a, Action::None);
        assert!(!show.should_quit());
    }

    #[test]
    fn ctrl_c_quits_always() {
        let mut show = Showcase::new();
        route(&mut show, KeyEventKind::Press, KeyCode::Tab, KeyModifiers::NONE); // foco demo
        let a = route(&mut show, KeyEventKind::Press, KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(a, Action::Quit);
        assert!(show.should_quit());
    }

    #[test]
    fn q_quits_from_index_only() {
        let mut show = Showcase::new();
        let a = route(&mut show, KeyEventKind::Press, KeyCode::Char('q'), KeyModifiers::NONE);
        assert_eq!(a, Action::Quit);
        assert!(show.should_quit());

        // Con foco en el demo, 'q' se reenvía al demo (no sale).
        let mut show = Showcase::new();
        route(&mut show, KeyEventKind::Press, KeyCode::Tab, KeyModifiers::NONE);
        let a = route(&mut show, KeyEventKind::Press, KeyCode::Char('q'), KeyModifiers::NONE);
        assert_eq!(a, Action::None);
        assert!(!show.should_quit());
    }

    #[test]
    fn arrows_move_selection_in_index_focus() {
        let mut show = dummy_show(3);
        route(&mut show, KeyEventKind::Press, KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(show.selected(), 1);
        route(&mut show, KeyEventKind::Press, KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(show.selected(), 0);
    }
}
```

Crea `crates/batten/examples/showcase/demos/mod.rs` con:

```rust
//! Catálogo de demos del showcase.

mod spinner;

use crate::demo::Demo;

/// Todos los demos, en el orden del índice.
pub fn catalog() -> Vec<Box<dyn Demo>> {
    vec![Box::new(spinner::SpinnerDemo::new())]
}

#[cfg(test)]
mod tests {
    use super::*;
    use batten::widgets::{Anim, Theme};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn catalog_titles_are_unique() {
        let cat = catalog();
        assert!(!cat.is_empty());
        let mut titles: Vec<&str> = cat.iter().map(|d| d.title()).collect();
        let n = titles.len();
        titles.sort_unstable();
        titles.dedup();
        assert_eq!(titles.len(), n, "títulos repetidos");
    }

    #[test]
    fn every_demo_renders_without_panicking() {
        let anim = Anim::new();
        for mut demo in catalog() {
            for (w, h) in [(0u16, 0u16), (1, 1), (40, 10), (120, 40)] {
                demo.tick(&anim);
                let backend = TestBackend::new(w, h);
                let mut term = Terminal::new(backend).unwrap();
                term.draw(|f| {
                    let area = f.area();
                    demo.render(f, area, &Theme::test());
                })
                .unwrap();
            }
        }
    }
}
```

- [ ] **Step 2: Ejecutar para ver fallar**

Run: `cargo test -p batten --examples`
Expected: FAIL (no compila: `Demo`, `Showcase`, `route`, `catalog` no existen).

- [ ] **Step 3: Implementar**

Crea `crates/batten/examples/showcase/demo.rs`:

```rust
//! Una demo del showcase: un widget (o composición) autónomo.

use batten::widgets::{Anim, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::Frame;

pub trait Demo {
    /// Título en el índice.
    fn title(&self) -> &str;
    /// Línea de ayuda con las teclas propias del demo.
    fn hint(&self) -> &str;
    /// Pinta la demo dentro de `area`.
    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme);
    /// Avanza la animación (una vez por frame). Por defecto no hace nada.
    fn tick(&mut self, _anim: &Anim) {}
    /// Procesa una tecla. En v0 el valor de retorno se ignora.
    fn handle_key(&mut self, _code: KeyCode) -> bool {
        false
    }
}
```

Completa `crates/batten/examples/showcase/app.rs` añadiendo, encima de `#[cfg(test)]`:

```rust
impl Showcase {
    pub fn new() -> Self {
        Self {
            demos: demos::catalog(),
            selected: 0,
            focus: Focus::Index,
            quit: false,
        }
    }

    pub fn len(&self) -> usize {
        self.demos.len()
    }

    pub fn title_at(&self, i: usize) -> Option<&str> {
        self.demos.get(i).map(|d| d.title())
    }

    /// Pista de teclas del demo activo.
    pub fn current_hint(&self) -> &str {
        self.demos
            .get(self.selected)
            .map(|d| d.hint())
            .unwrap_or("")
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn focus(&self) -> Focus {
        self.focus
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    /// Mueve la selección una posición abajo (con tope) y vuelve el foco al índice.
    pub fn next(&mut self) {
        if self.selected + 1 < self.demos.len() {
            self.selected += 1;
        }
        self.focus = Focus::Index;
    }

    /// Mueve la selección una posición arriba (con tope) y vuelve el foco al índice.
    pub fn prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
        self.focus = Focus::Index;
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Index => Focus::Demo,
            Focus::Demo => Focus::Index,
        };
    }

    pub fn current(&mut self) -> Option<&mut dyn Demo> {
        self.demos.get_mut(self.selected).map(|d| d.as_mut())
    }

    /// Avanza la animación del demo activo.
    pub fn tick(&mut self, anim: &Anim) {
        if let Some(d) = self.current() {
            d.tick(anim);
        }
    }

    #[cfg(test)]
    pub(crate) fn focus_demo_for_test(&mut self) {
        self.focus = Focus::Demo;
    }
}

/// Resuelve una tecla. Las globales primero; el resto, al demo si tiene el foco.
pub fn route(
    show: &mut Showcase,
    kind: KeyEventKind,
    code: KeyCode,
    mods: KeyModifiers,
) -> Action {
    if kind != KeyEventKind::Press {
        return Action::None;
    }
    if mods.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
        show.quit = true;
        return Action::Quit;
    }
    match show.focus {
        Focus::Index => match code {
            KeyCode::Up | KeyCode::Char('k') => {
                show.prev();
                Action::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                show.next();
                Action::None
            }
            KeyCode::Tab => {
                show.toggle_focus();
                Action::None
            }
            KeyCode::Char('q') => {
                show.quit = true;
                Action::Quit
            }
            _ => Action::None,
        },
        Focus::Demo => match code {
            KeyCode::Esc => {
                show.focus = Focus::Index;
                Action::None
            }
            KeyCode::Tab => {
                show.toggle_focus();
                Action::None
            }
            other => {
                if let Some(d) = show.current() {
                    d.handle_key(other);
                }
                Action::None
            }
        },
    }
}
```

Crea `crates/batten/examples/showcase/demos/spinner.rs`:

```rust
//! Demo del spinner (scramble + degradado).

use crate::demo::Demo;
use batten::widgets::{Anim, Theme};
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct SpinnerDemo {
    label: String,
    anim: Anim,
}

impl SpinnerDemo {
    pub fn new() -> Self {
        Self {
            label: "loading widgets".into(),
            anim: Anim::new(),
        }
    }
}

impl Demo for SpinnerDemo {
    fn title(&self) -> &str {
        "spinner"
    }

    fn hint(&self) -> &str {
        "animación automática"
    }

    fn tick(&mut self, anim: &Anim) {
        self.anim = *anim;
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let mut lines = vec![Line::from("scramble + degradado, avanza solo:")];
        lines.push(self.anim.render(&self.label, theme));
        f.render_widget(Paragraph::new(lines), area);
    }
}
```

Crea `crates/batten/examples/showcase/main.rs`:

```rust
//! Showcase navegable de los widgets de batten.
//! Uso: `cargo run -p batten --example showcase`

mod app;
mod demo;
mod demos;

use app::{route, Action, Focus, Showcase};
use batten::widgets::{Anim, Theme, FRAME_INTERVAL};
use ratatui::crossterm::event::{self, Event};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use std::time::{Duration, Instant};

const INDEX_WIDTH: u16 = 26;
const WIDE_MIN: u16 = 70;

fn main() -> std::io::Result<()> {
    let theme = Theme::default_dark();
    let mut show = Showcase::new();
    let mut anim = Anim::new();
    let mut terminal = ratatui::init();
    let mut last_tick = Instant::now();

    while !show.should_quit() {
        if last_tick.elapsed() >= FRAME_INTERVAL {
            anim.advance();
            show.tick(&anim);
            last_tick = Instant::now();
        }
        terminal.draw(|f| draw(f, &mut show, &anim, &theme))?;
        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(k) = event::read()? {
                if route(&mut show, k.kind, k.code, k.modifiers) == Action::Quit {
                    break;
                }
            }
        }
    }

    ratatui::restore();
    Ok(())
}

fn draw(f: &mut Frame, show: &mut Showcase, _anim: &Anim, theme: &Theme) {
    let area = f.area();
    if area.width == 0 || area.height == 0 {
        return;
    }
    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);
    let (body, footer) = (rows[0], rows[1]);

    let wide = area.width >= WIDE_MIN;
    let (index, panel) = if wide {
        let cols =
            Layout::horizontal([Constraint::Length(INDEX_WIDTH), Constraint::Min(1)]).split(body);
        (Some(cols[0]), cols[1])
    } else {
        (None, body)
    };

    let header_h = if wide { 2 } else { 3 };
    let parts = Layout::vertical([Constraint::Length(header_h), Constraint::Min(1)]).split(panel);
    let (header, content) = (parts[0], parts[1]);

    if let Some(ia) = index {
        draw_index(f, ia, show, theme);
        draw_header(f, header, show, theme);
    } else {
        draw_collapsed(f, header, show, theme);
    }

    if let Some(demo) = show.current() {
        demo.render(f, content, theme);
    }
    draw_footer(f, footer);
}

fn draw_index(f: &mut Frame, area: Rect, show: &Showcase, theme: &Theme) {
    let mut lines: Vec<Line> = Vec::new();
    for i in 0..show.len() {
        let title = show.title_at(i).unwrap_or("");
        let selected = i == show.selected();
        let focused = selected && show.focus() == Focus::Index;
        let marker = if focused { "▸ " } else if selected { "• " } else { "  " };
        let mut style = theme.style(batten::state::StyleToken::Default);
        if focused {
            style = style.add_modifier(Modifier::BOLD);
        }
        lines.push(Line::styled(format!("{marker}{title}"), style));
    }
    f.render_widget(Paragraph::new(lines), area);
}

fn draw_header(f: &mut Frame, area: Rect, show: &Showcase, theme: &Theme) {
    let title = show.title_at(show.selected()).unwrap_or("");
    let hint = match show.focus() {
        Focus::Index => "Tab: foco · ↓/↑: widget · q: salir".to_string(),
        Focus::Demo => format!("{} · Tab/Esc: índice", show.current_hint()),
    };
    let lines = vec![
        Line::styled(
            title.to_string(),
            theme
                .style(batten::state::StyleToken::Heading)
                .add_modifier(Modifier::BOLD),
        ),
        Line::styled(hint.to_string(), theme.style(batten::state::StyleToken::Muted)),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

fn draw_collapsed(f: &mut Frame, area: Rect, show: &Showcase, theme: &Theme) {
    let title = show.title_at(show.selected()).unwrap_or("");
    let lines = vec![
        Line::styled(
            format!("‹ {title} ›  (Tab cambia de widget)"),
            theme.style(batten::state::StyleToken::Muted),
        ),
        Line::styled(
            title.to_string(),
            theme.style(batten::state::StyleToken::Heading),
        ),
        Line::styled(
            "↓/↑: widget · Tab: foco · q: salir",
            theme.style(batten::state::StyleToken::Muted),
        ),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

fn draw_footer(f: &mut Frame, area: Rect) {
    f.render_widget(
        Paragraph::new("batten showcase").style(Style::default().add_modifier(Modifier::DIM)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn draw_does_not_panic_at_any_size() {
        let theme = Theme::default_dark();
        let anim = Anim::new();
        let mut show = Showcase::new();
        for (w, h) in [(0u16, 0u16), (1, 1), (60, 20), (120, 40)] {
            let backend = TestBackend::new(w, h);
            let mut term = Terminal::new(backend).unwrap();
            term.draw(|f| draw(f, &mut show, &anim, &theme)).unwrap();
        }
    }
}
```

- [ ] **Step 4: Ejecutar para ver pasar**

Run: `cargo test -p batten --examples`
Expected: PASS (tests de `app` y de `demos`).

Run: `cargo build -p batten --example showcase`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/batten/examples
git commit -m "feat(batten): navigable widget showcase with a spinner demo"
```

---

### Task 2: Demo `transcript`

**Files:**
- Create: `crates/batten/examples/showcase/demos/transcript.rs`
- Modify: `crates/batten/examples/showcase/demos/mod.rs`

**Interfaces:**
- Consumes: `batten::state::{Transcript, Line, LineKind, Viewport}`, `batten::widgets::transcript::render`, `Demo`.
- Produces: `struct TranscriptDemo` (`new`).

- [ ] **Step 1: Escribir el test**

En `demos/transcript.rs`, al final:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_changes_the_window() {
        let mut d = TranscriptDemo::new();
        d.handle_key(KeyCode::Char('G')); // al final
        let bottom = d.viewport.window(d.transcript.len(), 5);
        d.handle_key(KeyCode::Char('g')); // al principio
        let top = d.viewport.window(d.transcript.len(), 5);
        assert_ne!(bottom, top);
        assert_eq!(top.start, 0);
    }
}
```

- [ ] **Step 2: Ejecutar para ver fallar**

Run: `cargo test -p batten --examples`
Expected: FAIL (`TranscriptDemo` no existe).

- [ ] **Step 3: Implementar**

Crea `crates/batten/examples/showcase/demos/transcript.rs`:

```rust
//! Demo del transcript con una línea de cada tipo.

use crate::demo::Demo;
use batten::state::{Line, LineKind, Transcript, Viewport};
use batten::widgets::{transcript, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::Frame;

pub struct TranscriptDemo {
    transcript: Transcript,
    viewport: Viewport,
}

impl TranscriptDemo {
    pub fn new() -> Self {
        let mut transcript = Transcript::new();
        for (kind, text) in [
            (LineKind::User, "lee hello.txt y dime qué contiene"),
            (LineKind::System, "modelo mistral-nemo"),
            (LineKind::Assistant, "hola mundo"),
            (LineKind::Thinking, "el archivo parece un saludo"),
            (LineKind::ToolCall, "view hello.txt"),
            (LineKind::ToolResult, "6 líneas, 12 bytes"),
            (LineKind::Todo, "[x] leer hello.txt"),
            (LineKind::Plan, "1. leer 2. responder"),
            (LineKind::Banner, "sesión iniciada"),
            (LineKind::Error, "no se pudo escribir el archivo"),
        ] {
            transcript.push(Line::new(kind, text));
        }
        Self {
            transcript,
            viewport: Viewport::new(),
        }
    }
}

impl Demo for TranscriptDemo {
    fn title(&self) -> &str {
        "transcript"
    }

    fn hint(&self) -> &str {
        "↓/↑: scroll · g/G: inicio/fin"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        transcript::render(f, area, &self.transcript, &self.viewport, theme);
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Down => self.viewport.scroll_down(1),
            KeyCode::Up => self.viewport.scroll_up(1),
            KeyCode::Char('g') => self.viewport.scroll_up(usize::MAX),
            KeyCode::Char('G') => self.viewport.follow_bottom(),
            _ => return false,
        }
        true
    }
}
```

En `demos/mod.rs`, añade `mod transcript;` (ordenado) y `Box::new(transcript::TranscriptDemo::new())` al final del `vec![]`.

- [ ] **Step 4: Ejecutar para ver pasar**

Run: `cargo test -p batten --examples`
Expected: PASS (incluye el smoke del nuevo demo).

- [ ] **Step 5: Commit**

```bash
git add crates/batten/examples
git commit -m "feat(batten): transcript demo in the showcase"
```

---

### Task 3: Demo `tool_item`

**Files:**
- Create: `crates/batten/examples/showcase/demos/tool_item.rs`
- Modify: `crates/batten/examples/showcase/demos/mod.rs`

**Interfaces:**
- Consumes: `batten::state::{ToolItem, ToolStatus}`, `batten::widgets::{tool_item::render, Anim, Theme}`, `Demo`.
- Produces: `struct ToolItemDemo` (`new`).

- [ ] **Step 1: Escribir el test**

En `demos/tool_item.rs`, al final:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn n_cycles_the_selected_state() {
        let mut d = ToolItemDemo::new();
        let before = d.items[0].status();
        d.handle_key(KeyCode::Char('n'));
        assert_ne!(d.items[0].status(), before);
    }

    #[test]
    fn arrows_move_the_selection_with_clamp() {
        let mut d = ToolItemDemo::new();
        d.handle_key(KeyCode::Up);
        assert_eq!(d.selected, 0);
        d.handle_key(KeyCode::Down);
        assert_eq!(d.selected, 1);
    }
}
```

- [ ] **Step 2: Ejecutar para ver fallar**

Run: `cargo test -p batten --examples`
Expected: FAIL (`ToolItemDemo` no existe).

- [ ] **Step 3: Implementar**

Crea `crates/batten/examples/showcase/demos/tool_item.rs`:

```rust
//! Demo de tool items en todos sus estados.

use crate::demo::Demo;
use batten::state::ToolItem;
use batten::widgets::{tool_item, Anim, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::Frame;

/// Construye un `ToolItem` en la fase indicada (0..=5).
fn make(name: &str, phase: usize) -> ToolItem {
    let mut t = ToolItem::new(name, name);
    match phase % 6 {
        0 => {}                        // Pending
        1 => t.start(),                // Running
        2 => t.set_result(false),      // Success
        3 => t.set_result(true),       // Error
        4 => {
            t.start();
            t.cancel();
        } // Canceled
        _ => {
            t.start();
            t.await_permission();
        } // AwaitingPermission
    }
    t
}

pub struct ToolItemDemo {
    names: Vec<&'static str>,
    phases: Vec<usize>,
    items: Vec<ToolItem>,
    selected: usize,
    anim: Anim,
}

impl ToolItemDemo {
    pub fn new() -> Self {
        let names = vec!["view", "bash", "edit", "grep", "glob", "write"];
        let phases = vec![0, 1, 2, 3, 4, 5];
        let items = names
            .iter()
            .zip(&phases)
            .map(|(n, p)| make(n, *p))
            .collect();
        Self {
            names,
            phases,
            items,
            selected: 0,
            anim: Anim::new(),
        }
    }
}

impl Demo for ToolItemDemo {
    fn title(&self) -> &str {
        "tool_item"
    }

    fn hint(&self) -> &str {
        "↓/↑: elegir · n: cambiar estado · Enter: expandir"
    }

    fn tick(&mut self, anim: &Anim) {
        self.anim = *anim;
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let n = self.items.len().max(1);
        let rows = Layout::vertical(vec![Constraint::Length(2); n]).split(area);
        for (i, item) in self.items.iter_mut().enumerate() {
            if i >= rows.len() {
                break;
            }
            tool_item::render(f, rows[i], item, &self.anim, theme);
        }
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Down => self.selected = (self.selected + 1).min(self.items.len() - 1),
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Char('n') => {
                self.phases[self.selected] += 1;
                let name = self.names[self.selected];
                self.items[self.selected] = make(name, self.phases[self.selected]);
            }
            KeyCode::Enter | KeyCode::Char('x') => self.items[self.selected].toggle_expanded(),
            _ => return false,
        }
        true
    }
}
```

En `demos/mod.rs`, añade `mod tool_item;` y `Box::new(tool_item::ToolItemDemo::new())`.

- [ ] **Step 4: Ejecutar para ver pasar**

Run: `cargo test -p batten --examples`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/batten/examples
git commit -m "feat(batten): tool item demo in the showcase"
```

---

### Task 4: Demo `composer`

**Files:**
- Create: `crates/batten/examples/showcase/demos/composer.rs`
- Modify: `crates/batten/examples/showcase/demos/mod.rs`

**Interfaces:**
- Consumes: `batten::state::Composer`, `batten::widgets::{composer::render, Theme}`, `Demo`.
- Produces: `struct ComposerDemo` (`new`).

- [ ] **Step 1: Escribir el test**

En `demos/composer.rs`, al final:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_then_enter_appends_a_sent_line() {
        let mut d = ComposerDemo::new();
        d.handle_key(KeyCode::Char('h'));
        d.handle_key(KeyCode::Char('i'));
        d.handle_key(KeyCode::Enter);
        assert_eq!(d.sent, vec!["hi".to_string()]);
        assert!(d.composer.is_empty());
    }

    #[test]
    fn backspace_removes_a_char() {
        let mut d = ComposerDemo::new();
        d.handle_key(KeyCode::Char('a'));
        d.handle_key(KeyCode::Backspace);
        assert!(d.composer.is_empty());
    }
}
```

- [ ] **Step 2: Ejecutar para ver fallar**

Run: `cargo test -p batten --examples`
Expected: FAIL (`ComposerDemo` no existe).

- [ ] **Step 3: Implementar**

Crea `crates/batten/examples/showcase/demos/composer.rs`:

```rust
//! Demo del composer: escribe y envía.

use crate::demo::Demo;
use batten::state::Composer;
use batten::widgets::{composer, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct ComposerDemo {
    composer: Composer,
    sent: Vec<String>,
}

impl ComposerDemo {
    pub fn new() -> Self {
        Self {
            composer: Composer::new(),
            sent: Vec::new(),
        }
    }
}

impl Demo for ComposerDemo {
    fn title(&self) -> &str {
        "composer"
    }

    fn hint(&self) -> &str {
        "escribe · Enter: enviar · Backspace: borrar"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(3)]).split(area);
        let body: Vec<Line> = self
            .sent
            .iter()
            .map(|s| Line::from(format!("you: {s}")))
            .collect();
        f.render_widget(Paragraph::new(body), rows[0]);
        composer::render(f, rows[1], &self.composer, theme);
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Enter => {
                if let Some(text) = self.composer.submit() {
                    self.sent.push(text);
                }
            }
            KeyCode::Backspace => self.composer.backspace(),
            KeyCode::Char(c) => self.composer.push_char(c),
            _ => return false,
        }
        true
    }
}
```

En `demos/mod.rs`, añade `mod composer;` y `Box::new(composer::ComposerDemo::new())`.

- [ ] **Step 4: Ejecutar para ver pasar**

Run: `cargo test -p batten --examples`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/batten/examples
git commit -m "feat(batten): composer demo in the showcase"
```

---

### Task 5: Demo `logo`

**Files:**
- Create: `crates/batten/examples/showcase/demos/logo.rs`
- Modify: `crates/batten/examples/showcase/demos/mod.rs`

**Interfaces:**
- Consumes: `batten::widgets::logo::{self, Wordmark}`, `Anim`, `Theme`, `Demo`.
- Produces: `struct LogoDemo` (`new`).

- [ ] **Step 1: Escribir el test**

En `demos/logo.rs`, al final:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_appends_to_the_name() {
        let mut d = LogoDemo::new();
        for c in "hi".chars() {
            d.handle_key(KeyCode::Char(c));
        }
        assert!(d.name.starts_with("batten"));
        assert!(d.name.ends_with("hi"));
    }

    #[test]
    fn backspace_removes_a_char() {
        let mut d = LogoDemo::new();
        d.handle_key(KeyCode::Char('x'));
        d.handle_key(KeyCode::Backspace);
        assert!(!d.name.ends_with('x'));
    }
}
```

- [ ] **Step 2: Ejecutar para ver fallar**

Run: `cargo test -p batten --examples`
Expected: FAIL (`LogoDemo` no existe).

- [ ] **Step 3: Implementar**

Crea `crates/batten/examples/showcase/demos/logo.rs`:

```rust
//! Demo del logo paramétrico (fuente de bloques).

use crate::demo::Demo;
use batten::widgets::logo::{self, Wordmark};
use batten::widgets::Theme;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct LogoDemo {
    name: String,
}

impl LogoDemo {
    pub fn new() -> Self {
        Self {
            name: "batten".into(),
        }
    }
}

impl Demo for LogoDemo {
    fn title(&self) -> &str {
        "logo"
    }

    fn hint(&self) -> &str {
        "escribe para cambiar el nombre · Backspace: borrar"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let wm = Wordmark::new(&self.name);
        let rows = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(logo::FONT_ROWS as u16 + 1),
            Constraint::Min(1),
        ])
        .split(area);
        f.render_widget(
            Paragraph::new(format!("nombre: {}  ({} cols)", self.name, wm.width())),
            rows[0],
        );
        if wm.width() <= area.width as usize {
            f.render_widget(Paragraph::new(logo::render(&wm, theme)), rows[1]);
        } else {
            f.render_widget(
                Paragraph::new(logo::compact(&self.name, area.width as usize, theme)),
                rows[1],
            );
        }
        f.render_widget(
            Paragraph::new(Line::from("compacto (mismo widget, sin sitio para bloques):")),
            rows[2],
        );
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char(c) => self.name.push(c),
            KeyCode::Backspace => {
                self.name.pop();
            }
            _ => return false,
        }
        true
    }
}
```

En `demos/mod.rs`, añade `mod logo;` y `Box::new(logo::LogoDemo::new())`.

- [ ] **Step 4: Ejecutar para ver pasar**

Run: `cargo test -p batten --examples`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/batten/examples
git commit -m "feat(batten): logo demo in the showcase"
```

---

### Task 6: Demo `sidebar`

**Files:**
- Create: `crates/batten/examples/showcase/demos/sidebar.rs`
- Modify: `crates/batten/examples/showcase/demos/mod.rs`

**Interfaces:**
- Consumes: `batten::state::Sidebar`, `batten::widgets::{sidebar::render, Theme}`, `Demo`.
- Produces: `struct SidebarDemo` (`new`).

- [ ] **Step 1: Escribir el test**

En `demos/sidebar.rs`, al final:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plus_and_minus_add_and_remove_sections() {
        let mut d = SidebarDemo::new();
        let before = d.sidebar.sections.len();
        d.handle_key(KeyCode::Char('+'));
        assert_eq!(d.sidebar.sections.len(), before + 1);
        d.handle_key(KeyCode::Char('-'));
        assert_eq!(d.sidebar.sections.len(), before);
    }
}
```

- [ ] **Step 2: Ejecutar para ver fallar**

Run: `cargo test -p batten --examples`
Expected: FAIL (`SidebarDemo` no existe).

- [ ] **Step 3: Implementar**

Crea `crates/batten/examples/showcase/demos/sidebar.rs`:

```rust
//! Demo del sidebar genérico (logo + secciones).

use crate::demo::Demo;
use batten::state::Sidebar;
use batten::widgets::{sidebar, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::Frame;

pub struct SidebarDemo {
    sidebar: Sidebar,
    extra: usize,
}

impl SidebarDemo {
    pub fn new() -> Self {
        let mut sidebar = Sidebar::with_logo("batten");
        sidebar.push_section("estado", vec!["working".into()]);
        sidebar.push_section("modelo", vec!["mistral-nemo".into()]);
        Self {
            sidebar,
            extra: 0,
        }
    }
}

impl Demo for SidebarDemo {
    fn title(&self) -> &str {
        "sidebar"
    }

    fn hint(&self) -> &str {
        "+: añadir sección · -: quitar la última"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        sidebar::render(f, area, &self.sidebar, theme);
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char('+') => {
                self.extra += 1;
                self.sidebar
                    .push_section(format!("extra {}", self.extra), vec!["—".into()]);
            }
            KeyCode::Char('-') => {
                self.sidebar.sections.pop();
            }
            _ => return false,
        }
        true
    }
}
```

En `demos/mod.rs`, añade `mod sidebar;` y `Box::new(sidebar::SidebarDemo::new())`.

- [ ] **Step 4: Ejecutar para ver pasar**

Run: `cargo test -p batten --examples`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/batten/examples
git commit -m "feat(batten): sidebar demo in the showcase"
```

---

### Task 7: Demo `theme`

**Files:**
- Create: `crates/batten/examples/showcase/demos/theme.rs`
- Modify: `crates/batten/examples/showcase/demos/mod.rs`

**Interfaces:**
- Consumes: `batten::state::StyleToken`, `batten::widgets::Theme`, `Demo`.
- Produces: `struct ThemeDemo` (`new`).

- [ ] **Step 1: Escribir el test**

En `demos/theme.rs`, al final:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_toggles_between_default_and_partial() {
        let mut d = ThemeDemo::new();
        let before = d.alt;
        d.handle_key(KeyCode::Char('t'));
        assert_ne!(d.alt, before);
    }
}
```

- [ ] **Step 2: Ejecutar para ver fallar**

Run: `cargo test -p batten --examples`
Expected: FAIL (`ThemeDemo` no existe).

- [ ] **Step 3: Implementar**

Crea `crates/batten/examples/showcase/demos/theme.rs`:

```rust
//! Demo del tema: tokens resueltos con el default y con un theme.toml parcial.

use crate::demo::Demo;
use batten::state::StyleToken;
use batten::widgets::Theme;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const TOKENS: &[(&str, StyleToken)] = &[
    ("default", StyleToken::Default),
    ("muted", StyleToken::Muted),
    ("dim", StyleToken::Dim),
    ("heading", StyleToken::Heading),
    ("user", StyleToken::User),
    ("assistant", StyleToken::Assistant),
    ("system", StyleToken::System),
    ("error", StyleToken::Error),
    ("warning", StyleToken::Warning),
    ("success", StyleToken::Success),
    ("tool_name", StyleToken::ToolName),
    ("tool_running", StyleToken::ToolRunning),
    ("tool_success", StyleToken::ToolSuccess),
    ("tool_error", StyleToken::ToolError),
    ("accent", StyleToken::Accent),
    ("accent_from", StyleToken::AccentFrom),
    ("accent_to", StyleToken::AccentTo),
    ("logo_field", StyleToken::LogoField),
];

pub struct ThemeDemo {
    default: Theme,
    partial: Theme,
    alt: bool,
}

impl ThemeDemo {
    pub fn new() -> Self {
        let partial = Theme::from_toml(
            "[styles]\naccent_from = \"magenta\"\naccent_to = \"yellow\"\nerror = \"indexed:196\"\n",
        )
        .unwrap_or_else(|_| Theme::default_dark());
        Self {
            default: Theme::default_dark(),
            partial,
            alt: false,
        }
    }
}

impl Demo for ThemeDemo {
    fn title(&self) -> &str {
        "theme"
    }

    fn hint(&self) -> &str {
        "t: alternar default / theme.toml parcial"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, _theme: &Theme) {
        let title = if self.alt {
            "tema: theme.toml parcial"
        } else {
            "tema: default_dark"
        };
        let th = if self.alt {
            &self.partial
        } else {
            &self.default
        };
        let mut lines = vec![Line::from(title.to_string())];
        for (name, token) in TOKENS {
            lines.push(Line::from(vec![
                Span::styled("Aa ", th.style(*token)),
                Span::styled((*name).to_string(), th.style(*token)),
            ]));
        }
        f.render_widget(Paragraph::new(lines), area);
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        if code == KeyCode::Char('t') {
            self.alt = !self.alt;
            true
        } else {
            false
        }
    }
}
```

En `demos/mod.rs`, añade `mod theme;` y `Box::new(theme::ThemeDemo::new())`.

- [ ] **Step 4: Ejecutar para ver pasar**

Run: `cargo test -p batten --examples`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/batten/examples
git commit -m "feat(batten): theme demo in the showcase"
```

---

### Task 8: Demo `chat`

**Files:**
- Create: `crates/batten/examples/showcase/demos/chat.rs`
- Modify: `crates/batten/examples/showcase/demos/mod.rs`

**Interfaces:**
- Consumes: `batten::state::{Composer, Line, LineKind, Sidebar, ToolItem, Transcript, Viewport}`, `batten::widgets::{transcript, tool_item, sidebar, composer, Anim, Theme}`, `Demo`.
- Produces: `struct ChatDemo` (`new`).

- [ ] **Step 1: Escribir el test**

En `demos/chat.rs`, al final:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_appends_a_user_line() {
        let mut d = ChatDemo::new();
        let before = d.transcript.len();
        d.handle_key(KeyCode::Char('h'));
        d.handle_key(KeyCode::Char('i'));
        d.handle_key(KeyCode::Enter);
        assert_eq!(d.transcript.len(), before + 1);
        assert_eq!(d.transcript.last_mut().unwrap().plain(), "hi");
    }
}
```

- [ ] **Step 2: Ejecutar para ver fallar**

Run: `cargo test -p batten --examples`
Expected: FAIL (`ChatDemo` no existe).

- [ ] **Step 3: Implementar**

Crea `crates/batten/examples/showcase/demos/chat.rs`:

```rust
//! Demo de la composición completa: transcript + tool + sidebar + composer.

use crate::demo::Demo;
use batten::state::{Composer, Line, LineKind, Sidebar, ToolItem, Transcript, Viewport};
use batten::widgets::{composer, sidebar, tool_item, transcript, Anim, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::Frame;

pub struct ChatDemo {
    transcript: Transcript,
    viewport: Viewport,
    composer: Composer,
    sidebar: Sidebar,
    tool: ToolItem,
    anim: Anim,
    started: bool,
    elapsed_frames: u32,
}

impl ChatDemo {
    pub fn new() -> Self {
        let mut transcript = Transcript::new();
        transcript.push(Line::new(LineKind::User, "lee hello.txt"));
        let mut sidebar = Sidebar::with_logo("batten");
        sidebar.push_section("estado", vec!["working".into()]);
        sidebar.push_section("modelo", vec!["mistral-nemo".into()]);
        let mut tool = ToolItem::new("c1", "view");
        tool.start();
        Self {
            transcript,
            viewport: Viewport::new(),
            composer: Composer::new(),
            sidebar,
            tool,
            anim: Anim::new(),
            started: true,
            elapsed_frames: 0,
        }
    }
}

impl Demo for ChatDemo {
    fn title(&self) -> &str {
        "chat"
    }

    fn hint(&self) -> &str {
        "escribe · Enter: enviar · ↓/↑: scroll"
    }

    fn tick(&mut self, anim: &Anim) {
        self.anim = *anim;
        self.elapsed_frames = self.elapsed_frames.saturating_add(1);
        // A los ~25 frames (≈ FRAME_INTERVAL) la tool termina y llega la respuesta.
        if self.started && self.elapsed_frames == 25 {
            self.tool.set_result(false);
            self.transcript
                .push(Line::new(LineKind::Assistant, "hola mundo"));
            self.started = false;
        }
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let wide = area.width >= 60;
        let (body, side) = if wide {
            let cols =
                Layout::horizontal([Constraint::Min(1), Constraint::Length(28)]).split(area);
            (cols[0], Some(cols[1]))
        } else {
            (area, None)
        };

        let rows = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(body);
        tool_item::render(f, rows[0], &self.tool, &self.anim, theme);
        transcript::render(f, rows[1], &self.transcript, &self.viewport, theme);
        composer::render(f, rows[2], &self.composer, theme);
        if let Some(s) = side {
            sidebar::render(f, s, &self.sidebar, theme);
        }
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Enter => {
                if let Some(text) = self.composer.submit() {
                    self.transcript.push(Line::new(LineKind::User, text));
                    self.viewport.follow_bottom();
                }
            }
            KeyCode::Backspace => self.composer.backspace(),
            KeyCode::Down => self.viewport.scroll_down(1),
            KeyCode::Up => self.viewport.scroll_up(1),
            KeyCode::Char(c) => self.composer.push_char(c),
            _ => return false,
        }
        true
    }
}
```

En `demos/mod.rs`, añade `mod chat;` y `Box::new(chat::ChatDemo::new())`.

- [ ] **Step 4: Ejecutar para ver pasar**

Run: `cargo test -p batten --examples`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/batten/examples
git commit -m "feat(batten): full chat composition demo in the showcase"
```

---

### Task 9: Retirar `demo.rs` y verificación final

**Files:**
- Delete: `crates/batten/examples/demo.rs`
- Modify: `docs/specs/2026-10-14-batten-design.md` (referencias a `examples/demo.rs`)

**Interfaces:**
- Consumes: nada.
- Produces: nada.

- [ ] **Step 1: Retirar el ejemplo antiguo**

```bash
git rm crates/batten/examples/demo.rs
```

- [ ] **Step 2: Actualizar la spec de batten**

En `docs/specs/2026-10-14-batten-design.md`:

- En el árbol de §2, sustituye la línea `demo.rs             escenarios por argumento` por
  `showcase/            storybook navegable de widgets (main.rs multi-archivo)`.
- En §7, sustituye la viñeta que empieza por `- **examples/demo.rs**` por:

```markdown
- **`examples/showcase/`**: storybook navegable (índice + demo por widget +
  pantalla `chat`), sin back. Ver `docs/specs/2026-10-15-batten-showcase-design.md`.
```

- [ ] **Step 3: Verificar**

Run: `cargo test -p batten --examples`
Expected: PASS.

Run: `cargo build -p batten --example showcase`
Expected: PASS.

Run: `cargo fmt --check`
Expected: PASS.

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.

Run: `cargo test -p batten -p codeloom-core -p codeloom-tui`
Expected: PASS.

- [ ] **Step 4: Verificación manual**

Run: `cargo run -p batten --example showcase`
Expected: índice con transcript, tool_item, composer, spinner, logo, sidebar, theme y chat; `↓/↑` cambia widget, `Tab` da foco al demo, dentro se teclea/scrolla/cicla, `Esc` vuelve, `q` sale.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor(batten): replace the demo with the widget showcase"
```

---

## Criterios de aceptación

- [ ] `cargo run -p batten --example showcase` abre el índice con 8 demos navegables.
- [ ] Los 7 widgets y `chat` se ven y responden al teclado.
- [ ] `cargo test -p batten --examples` pasa (estado, enrutado y smoke de todos los demos).
- [ ] `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` y `cargo test -p batten -p codeloom-core -p codeloom-tui` pasan.
- [ ] `examples/demo.rs` retirado; `batten` sigue sin crossterm.
