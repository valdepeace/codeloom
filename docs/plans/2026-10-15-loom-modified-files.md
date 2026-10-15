# Ficheros modificados — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Mostrar en el sidebar los ficheros tocados en la sesión con `+A -R`, como Crush: modelo y widget en batten, evento y tracker en core, y cableado en codeloom-tui.

**Architecture:** batten añade `state::FileChanges` (puro) y `widgets::file_changes` (render). core añade `LiveEvent::FileDiff` y un `ChangeTracker` que resuelve los números con `git diff --numstat` (respaldo: líneas de `old`/`new` del `edit`); el agent loop lo publica tras cada `edit` con éxito. codeloom-tui agrega los eventos y pinta el widget bajo el sidebar.

**Tech Stack:** Rust (edición 2021), `ratatui` 0.30, `unicode-width`, `tokio`, `serde_json`.

**Spec:** `docs/specs/2026-10-15-loom-modified-files-design.md`

## Global Constraints

- `batten` no gana dependencias ni código crossterm; `state` sigue sin `ratatui`.
- core no toca la red en tests (el tracker no usa red; git es local y opcional).
- Comentarios en español.
- Números: git cuando hay repo; respaldo `(new.lines().count(), old.lines().count())`.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` y los tests pasan en cada commit.

## Review Focus

- **Sin git instalado / dir sin repo**: `ChangeTracker::detect` no entra en pánico y usa el respaldo. Test en Task 3.
- **Fichero nuevo (untracked) o binario**: `git diff --numstat` no da números → respaldo. Test en Task 3.
- **Anchura 0**: `file_changes::render` no entra en pánico. Test en Task 2.
- **`upsert` repetido del mismo path**: no duplica y conserva el orden. Test en Task 1.
- **Sidebar muy bajo**: al partir la columna, el sidebar no queda con 0 filas. Test en Task 5.

---

## File Structure

```
crates/batten/
  src/state/file_changes.rs   # NUEVO
  src/state/mod.rs            # + pub mod file_changes; re-exports
  src/widgets/text.rs         # NUEVO: truncate compartido
  src/widgets/file_changes.rs # NUEVO
  src/widgets/mod.rs          # + mod text; + pub mod file_changes
  src/widgets/sidebar.rs      # usa el truncate compartido
  examples/showcase/demos/file_changes.rs  # NUEVO
  examples/showcase/demos/mod.rs           # + catálogo
crates/codeloom-core/
  src/event/mod.rs            # + LiveEvent::FileDiff
  src/agent/changes.rs        # NUEVO
  src/agent/mod.rs            # + ChangeTracker; publica FileDiff
crates/codeloom-tui/
  src/app.rs                  # + changes; + apply
  src/render.rs               # + file_changes bajo el sidebar
```

---

### Task 1: `state::FileChanges`

**Files:**
- Create: `crates/batten/src/state/file_changes.rs`
- Modify: `crates/batten/src/state/mod.rs`

**Interfaces:**
- Consumes: nada.
- Produces: `struct FileChange { path: String, added: usize, removed: usize }`; `struct FileChanges` con `new`, `upsert`, `pop`, `len`, `is_empty`, `iter`, `totals`.

- [ ] **Step 1: Escribir los tests**

Crea `crates/batten/src/state/file_changes.rs`:

```rust
//! Ficheros tocados en la sesión con sus líneas añadidas/quitadas. Puro.

#[cfg(test)]
mod tests {
    use super::*;

    fn change(path: &str, added: usize, removed: usize) -> FileChange {
        FileChange {
            path: path.to_string(),
            added,
            removed,
        }
    }

    #[test]
    fn upsert_appends_new_paths() {
        let mut c = FileChanges::new();
        assert!(c.is_empty());
        c.upsert(change("a.rs", 3, 1));
        c.upsert(change("b.rs", 1, 0));
        assert_eq!(c.len(), 2);
        assert_eq!(c.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["a.rs", "b.rs"]);
    }

    #[test]
    fn upsert_replaces_by_path_and_keeps_order() {
        let mut c = FileChanges::new();
        c.upsert(change("a.rs", 3, 1));
        c.upsert(change("b.rs", 1, 0));
        c.upsert(change("a.rs", 10, 2));
        assert_eq!(c.len(), 2);
        assert_eq!(c.iter().next().unwrap().path, "a.rs");
        assert_eq!(c.iter().next().unwrap().added, 10);
        assert_eq!(c.iter().next().unwrap().removed, 2);
    }

    #[test]
    fn totals_sum_every_file() {
        let mut c = FileChanges::new();
        c.upsert(change("a.rs", 3, 1));
        c.upsert(change("b.rs", 1, 4));
        assert_eq!(c.totals(), (4, 5));
    }

    #[test]
    fn pop_removes_the_last() {
        let mut c = FileChanges::new();
        c.upsert(change("a.rs", 1, 1));
        c.upsert(change("b.rs", 1, 1));
        assert_eq!(c.pop().unwrap().path, "b.rs");
        assert_eq!(c.len(), 1);
        assert!(c.pop().is_some());
        assert!(c.pop().is_none());
    }
}
```

- [ ] **Step 2: Ejecutar para ver fallar**

Run: `cargo test -p batten state::file_changes`
Expected: FAIL (no compila: tipos no existen).

- [ ] **Step 3: Implementar**

Añade encima de `#[cfg(test)]`:

```rust
/// Un fichero tocado: ruta y líneas añadidas/quitadas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    pub added: usize,
    pub removed: usize,
}

/// Lista de ficheros tocados, sin duplicados por `path` (orden de primera
/// aparición).
#[derive(Debug, Default, Clone)]
pub struct FileChanges {
    files: Vec<FileChange>,
}

impl FileChanges {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserta o reemplaza por `path`.
    pub fn upsert(&mut self, change: FileChange) {
        match self.files.iter_mut().find(|c| c.path == change.path) {
            Some(slot) => *slot = change,
            None => self.files.push(change),
        }
    }

    /// Quita y devuelve el último fichero.
    pub fn pop(&mut self) -> Option<FileChange> {
        self.files.pop()
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &FileChange> {
        self.files.iter()
    }

    /// Suma de (added, removed) de todos los ficheros.
    pub fn totals(&self) -> (usize, usize) {
        self.files
            .iter()
            .fold((0, 0), |(a, r), c| (a + c.added, r + c.removed))
    }
}
```

En `state/mod.rs`, añade `pub mod file_changes;` y
`pub use file_changes::{FileChange, FileChanges};`.

- [ ] **Step 4: Ejecutar para ver pasar**

Run: `cargo test -p batten state::file_changes`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/batten
git commit -m "feat(batten): file-changes state for the sidebar"
```

---

### Task 2: `widgets::file_changes` (+ truncate compartido)

**Files:**
- Create: `crates/batten/src/widgets/text.rs`
- Create: `crates/batten/src/widgets/file_changes.rs`
- Modify: `crates/batten/src/widgets/mod.rs`
- Modify: `crates/batten/src/widgets/sidebar.rs` (usar el truncate compartido)

**Interfaces:**
- Consumes: `state::{FileChanges, StyleToken}` (Task 1), `widgets::Theme`.
- Produces: `pub(crate) fn truncate(&str, usize) -> String`; `pub fn render(frame, area, title: &str, changes: &FileChanges, theme: &Theme)`.

- [ ] **Step 1: Escribir el test**

Crea `crates/batten/src/widgets/file_changes.rs`:

```rust
//! Sección de ficheros modificados: título + `+A`/`-R` por fichero.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{FileChange, FileChanges};
    use crate::widgets::Theme;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn buffer(c: &FileChanges, w: u16, h: u16) -> String {
        let backend = TestBackend::new(w, h);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f, f.area(), "ficheros", c, &Theme::test()))
            .unwrap();
        term.backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    fn sample() -> FileChanges {
        let mut c = FileChanges::new();
        c.upsert(FileChange {
            path: "src/main.rs".into(),
            added: 2,
            removed: 1,
        });
        c
    }

    #[test]
    fn shows_title_paths_and_counts() {
        let out = buffer(&sample(), 40, 6);
        assert!(out.contains("ficheros"));
        assert!(out.contains("src/main.rs"));
        assert!(out.contains("+2"));
        assert!(out.contains("-1"));
    }

    #[test]
    fn truncates_long_paths() {
        let mut c = FileChanges::new();
        c.upsert(FileChange {
            path: "a/very/long/path/that/will/not/fit/at/all/file.rs".into(),
            added: 1,
            removed: 0,
        });
        let out = buffer(&c, 20, 4);
        assert!(out.contains('…'));
    }

    #[test]
    fn zero_size_does_not_panic() {
        let _ = buffer(&sample(), 0, 0);
        let _ = buffer(&FileChanges::new(), 0, 0);
    }
}
```

- [ ] **Step 2: Ejecutar para ver fallar**

Run: `cargo test -p batten widgets::file_changes`
Expected: FAIL (`render` no existe).

- [ ] **Step 3: Implementar**

Crea `crates/batten/src/widgets/text.rs`:

```rust
//! Helpers de texto compartidos por los widgets.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Trunca a `max` columnas de display, con `…` final si recorta.
pub(crate) fn truncate(s: &str, max: usize) -> String {
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
    out.push('…');
    out
}
```

Crea `crates/batten/src/widgets/file_changes.rs` (implementación encima de los tests):

```rust
use crate::state::{FileChanges, StyleToken};
use crate::widgets::text::truncate;
use crate::widgets::Theme;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub fn render(frame: &mut Frame, area: Rect, title: &str, changes: &FileChanges, theme: &Theme) {
    let (add, rem) = changes.totals();
    let mut lines = vec![Line::from(vec![
        Span::styled(title.to_string(), theme.style(StyleToken::Heading)),
        Span::raw(" "),
        Span::styled(format!("+{add}"), theme.style(StyleToken::Success)),
        Span::raw(" "),
        Span::styled(format!("-{rem}"), theme.style(StyleToken::Error)),
    ])];

    let width = area.width as usize;
    let room = area.height.saturating_sub(1) as usize;
    for c in changes.iter().take(room) {
        let counts = format!(" +{} -{}", c.added, c.removed);
        let used = counts.chars().count();
        lines.push(Line::from(vec![
            Span::styled(
                truncate(&c.path, width.saturating_sub(used)),
                theme.style(StyleToken::Muted),
            ),
            Span::styled(format!(" +{}", c.added), theme.style(StyleToken::Success)),
            Span::styled(format!(" -{}", c.removed), theme.style(StyleToken::Error)),
        ]));
    }

    frame.render_widget(Paragraph::new(lines), area);
}
```

En `widgets/mod.rs`, añade `mod text;` y `pub mod file_changes;`.

En `widgets/sidebar.rs`, borra su `fn truncate` local y usa
`use crate::widgets::text::truncate;` (mismo comportamiento).

- [ ] **Step 4: Ejecutar para ver pasar**

Run: `cargo test -p batten`
Expected: PASS (los tests del sidebar siguen verdes; 2 de widget nuevos + 3).

- [ ] **Step 5: Commit**

```bash
git add crates/batten
git commit -m "feat(batten): modified-files widget with add/remove counts"
```

---

### Task 3: core — `LiveEvent::FileDiff` + `ChangeTracker`

**Files:**
- Modify: `crates/codeloom-core/src/event/mod.rs`
- Create: `crates/codeloom-core/src/agent/changes.rs`
- Modify: `crates/codeloom-core/src/agent/mod.rs` (`pub mod changes;`)

**Interfaces:**
- Consumes: nada.
- Produces: `LiveEvent::FileDiff { path, added, removed }`; `ChangeTracker::detect`, `ChangeTracker::numbers`, `ChangeTracker::without_git` (test).

- [ ] **Step 1: Escribir los tests**

Crea `crates/codeloom-core/src/agent/changes.rs`:

```rust
//! Resuelve los números (+/-) de un fichero tocado por un `edit`.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_counts_lines_of_old_and_new() {
        let t = ChangeTracker::without_git(".");
        assert_eq!(t.numbers("x", "a\nb\nc", "a\nb"), (2, 3));
        assert_eq!(t.numbers("x", "", "nuevo"), (1, 0));
        assert_eq!(t.numbers("x", "viejo", ""), (0, 1));
    }

    #[test]
    fn detect_without_repo_does_not_panic_and_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        let t = ChangeTracker::detect(dir.path());
        // tempdir no es repo git -> respaldo.
        assert_eq!(t.numbers("x", "a", "a\nb"), (2, 1));
    }
}
```

- [ ] **Step 2: Ejecutar para ver fallar**

Run: `cargo test -p codeloom-core agent::changes`
Expected: FAIL (no compila).

- [ ] **Step 3: Implementar**

En `crates/codeloom-core/src/event/mod.rs`, añade a `LiveEvent`:

```rust
    FileDiff { path: String, added: usize, removed: usize },
```

Crea `crates/codeloom-core/src/agent/changes.rs` (impl encima de los tests):

```rust
use std::path::PathBuf;
use std::process::Command;

/// Resuelve los números de un `edit`. Con git usa `diff --numstat`; si no,
/// cuenta las líneas de `old`/`new` (exacto, porque `edit` reemplaza `old`).
pub struct ChangeTracker {
    cwd: PathBuf,
    git: bool,
}

impl ChangeTracker {
    pub fn detect(cwd: impl Into<PathBuf>) -> Self {
        let cwd = cwd.into();
        let git = Command::new("git")
            .arg("-C")
            .arg(&cwd)
            .args(["rev-parse", "--is-inside-work-tree"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        Self { cwd, git }
    }

    #[cfg(test)]
    pub fn without_git(cwd: impl Into<PathBuf>) -> Self {
        Self {
            cwd: cwd.into(),
            git: false,
        }
    }

    /// `(added, removed)` para un `edit` de `old` a `new`.
    pub fn numbers(&self, path: &str, old: &str, new: &str) -> (usize, usize) {
        if self.git {
            if let Some(n) = self.git_numbers(path) {
                return n;
            }
        }
        (new.lines().count(), old.lines().count())
    }

    fn git_numbers(&self, path: &str) -> Option<(usize, usize)> {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.cwd)
            .args(["diff", "--numstat", "--"])
            .arg(path)
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let line = text.lines().next()?;
        let mut it = line.split('\t');
        let added = it.next()?.trim().parse::<usize>().ok()?;
        let removed = it.next()?.trim().parse::<usize>().ok()?;
        Some((added, removed))
    }
}
```

En `agent/mod.rs`, añade `pub mod changes;` junto a `pub mod dispatch;`.

- [ ] **Step 4: Ejecutar para ver pasar**

Run: `cargo test -p codeloom-core agent::changes`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/codeloom-core
git commit -m "feat(core): file-diff event and change tracker"
```

---

### Task 4: core — el agent loop publica `FileDiff`

**Files:**
- Modify: `crates/codeloom-core/src/agent/mod.rs`

**Interfaces:**
- Consumes: `ChangeTracker` (Task 3), `LiveEvent::FileDiff` (Task 3).
- Produces: `AgentLoop` con campo `changes: ChangeTracker`.

- [ ] **Step 1: Escribir el test**

En `agent/mod.rs`, dentro de `mod tests`, añade (junto a los existentes):

```rust
    #[tokio::test]
    async fn edit_publishes_a_file_diff() {
        let scripts = vec![
            vec![
                StreamChunk::ToolCallComplete {
                    id: "c1".into(),
                    name: "edit".into(),
                    input: serde_json::json!({"path":"a.txt","old":"world","new":"rust"}),
                },
                StreamChunk::Done(FinishReason::ToolCalls),
            ],
            vec![
                StreamChunk::TextDelta("done".into()),
                StreamChunk::Done(FinishReason::Stop),
            ],
        ];
        let provider = Arc::new(QueueProvider {
            scripts: Mutex::new(scripts.into()),
        });
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "hello world").unwrap();

        let store = Arc::new(
            SqliteStore::open(dir.path().join("l.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let sid = store.create_session("/p").await.unwrap();

        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(crate::tools::edit::EditTool));

        let cfg = CodeLoomConfig {
            provider: crate::config::ProviderConfig {
                kind: crate::config::ProviderKind::OpenAiCompat,
                base_url: "http://x".into(),
                api_key_env: "K".into(),
            },
            model: "m".into(),
            max_steps: 5,
        };
        let bus = EventBus::new(64);
        let mut rx = bus.subscribe();
        let agent = AgentLoop::new(
            provider,
            store.clone(),
            Arc::new(tools),
            bus,
            Arc::new(PermissionService::allow_all()),
            cfg,
            dir.path().to_path_buf(),
        );

        agent
            .run(sid, "hi".into(), CancellationToken::new())
            .await
            .unwrap();

        let mut found = None;
        while let Ok(ev) = rx.try_recv() {
            if let LiveEvent::FileDiff {
                path,
                added,
                removed,
            } = ev
            {
                found = Some((path, added, removed));
            }
        }
        assert_eq!(found, Some(("a.txt".to_string(), 1, 1)));
    }
```

- [ ] **Step 2: Ejecutar para ver fallar**

Run: `cargo test -p codeloom-core agent::tests::edit_publishes_a_file_diff`
Expected: FAIL (no se publica `FileDiff`).

- [ ] **Step 3: Implementar**

En `AgentLoop`: añade el campo `changes: crate::agent::changes::ChangeTracker` y
en `new(...)` inicialízalo con `changes: crate::agent::changes::ChangeTracker::detect(cwd.clone())`.

Sustituye el bloque de ejecución de tools por (usa un mapa de llamadas para
emparejar por id antes de mover `allowed`):

```rust
            let mut by_id: std::collections::HashMap<String, (String, serde_json::Value)> =
                std::collections::HashMap::new();
            for (id, name, input) in &allowed {
                by_id.insert(id.clone(), (name.clone(), input.clone()));
            }

            let results =
                dispatch::execute_tools(&self.tools, ctx.clone(), allowed, cancel.clone()).await;
            for (id, out) in results {
                if !out.is_error {
                    if let Some((name, input)) = by_id.get(&id) {
                        if name == "edit" {
                            let path = input["path"].as_str().unwrap_or_default();
                            let old = input["old"].as_str().unwrap_or_default();
                            let new = input["new"].as_str().unwrap_or_default();
                            let (added, removed) = self.changes.numbers(path, old, new);
                            self.bus.publish(LiveEvent::FileDiff {
                                path: path.to_string(),
                                added,
                                removed,
                            });
                        }
                    }
                }
                let tr = Message::new(
                    Role::Tool,
                    vec![ContentPart::ToolResult {
                        call_id: id,
                        content: out.content,
                        is_error: out.is_error,
                    }],
                );
                self.store.append_message(session, &tr).await?;
                self.bus.publish(LiveEvent::MessageAdded(tr.clone()));
                history.push(tr);
            }
```

- [ ] **Step 4: Ejecutar para ver pasar**

Run: `cargo test -p codeloom-core`
Expected: PASS (incluye los tests previos del agent).

- [ ] **Step 5: Commit**

```bash
git add crates/codeloom-core
git commit -m "feat(core): publish a file-diff event after each edit"
```

---

### Task 5: codeloom-tui — cableado

**Files:**
- Modify: `crates/codeloom-tui/src/app.rs`
- Modify: `crates/codeloom-tui/src/render.rs`

**Interfaces:**
- Consumes: `batten::state::{FileChange, FileChanges}` (Task 1), `batten::widgets::file_changes::render` (Task 2), `LiveEvent::FileDiff` (Task 3).
- Produces: `App.changes`.

- [ ] **Step 1: Escribir los tests**

En `crates/codeloom-tui/src/app.rs`, dentro de `mod tests`:

```rust
    #[test]
    fn applies_file_diffs() {
        let mut app = App::new();
        app.apply(codeloom_core::event::LiveEvent::FileDiff {
            path: "src/main.rs".into(),
            added: 3,
            removed: 1,
        });
        assert_eq!(app.changes.len(), 1);
        assert_eq!(app.changes.totals(), (3, 1));
    }
```

En `crates/codeloom-tui/src/render.rs`, dentro de `mod tests`:

```rust
    #[test]
    fn shows_modified_files_when_wide() {
        let mut app = App::with_context("/proj", "m");
        app.apply(codeloom_core::event::LiveEvent::FileDiff {
            path: "src/main.rs".into(),
            added: 3,
            removed: 1,
        });
        let content = buffer_of(&app, 120, 40);
        assert!(content.contains("ficheros modificados"));
        assert!(content.contains("src/main.rs"));
    }
```

- [ ] **Step 2: Ejecutar para ver fallar**

Run: `cargo test -p codeloom-tui app::tests::applies_file_diffs`
Expected: FAIL (`App` no tiene `changes`).

- [ ] **Step 3: Implementar**

En `app.rs`: añade el campo `pub changes: batten::state::FileChanges`, inicializa
en `new()` con `batten::state::FileChanges::new()`, y en `apply` añade el brazo:

```rust
            codeloom_core::event::LiveEvent::FileDiff {
                path,
                added,
                removed,
            } => self.changes.upsert(batten::state::FileChange {
                path,
                added,
                removed,
            }),
```

En `render.rs`, sustituye la rama ancha de `render` por:

```rust
    if area.width >= SIDEBAR_MIN_WIDTH {
        let cols = Layout::horizontal([Constraint::Min(1), Constraint::Length(SIDEBAR_WIDTH)])
            .split(rows[1]);
        draw_transcript(frame, cols[0], app);
        let right = cols[1];
        if app.changes.is_empty() {
            batten::widgets::sidebar::render(frame, right, &app.sidebar(), &app.theme);
        } else {
            let need = (app.changes.len() as u16).saturating_add(2);
            let h = need.min(right.height.saturating_sub(1));
            let parts = Layout::vertical([Constraint::Min(1), Constraint::Length(h)]).split(right);
            batten::widgets::sidebar::render(frame, parts[0], &app.sidebar(), &app.theme);
            batten::widgets::file_changes::render(
                frame,
                parts[1],
                "ficheros modificados",
                &app.changes,
                &app.theme,
            );
        }
    } else {
        draw_transcript(frame, rows[1], app);
    }
```

- [ ] **Step 4: Ejecutar para ver pasar**

Run: `cargo test -p codeloom-tui`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/codeloom-tui
git commit -m "feat(tui): show modified files under the sidebar"
```

---

### Task 6: showcase — demo `file_changes`

**Files:**
- Create: `crates/batten/examples/showcase/demos/file_changes.rs`
- Modify: `crates/batten/examples/showcase/demos/mod.rs`

**Interfaces:**
- Consumes: `batten::state::{FileChange, FileChanges}`, `batten::widgets::file_changes::render`, `Demo`.
- Produces: `struct FileChangesDemo` (`new`).

- [ ] **Step 1: Escribir el test**

Crea `crates/batten/examples/showcase/demos/file_changes.rs` con solo el test:

```rust
//! Demo de la sección de ficheros modificados.

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyCode;

    #[test]
    fn a_adds_and_d_removes() {
        let mut d = FileChangesDemo::new();
        let before = d.changes.len();
        d.handle_key(KeyCode::Char('a'));
        assert_eq!(d.changes.len(), before + 1);
        d.handle_key(KeyCode::Char('d'));
        assert_eq!(d.changes.len(), before);
    }
}
```

- [ ] **Step 2: Ejecutar para ver fallar**

Run: `cargo test -p batten --examples`
Expected: FAIL (`FileChangesDemo` no existe).

- [ ] **Step 3: Implementar**

Añade encima de los tests:

```rust
use crate::demo::Demo;
use batten::state::{FileChange, FileChanges};
use batten::widgets::{file_changes, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::Frame;

pub struct FileChangesDemo {
    changes: FileChanges,
    counter: usize,
}

impl FileChangesDemo {
    pub fn new() -> Self {
        let mut changes = FileChanges::new();
        for (path, added, removed) in [
            ("src/main.rs", 12, 3),
            ("crates/batten/src/widgets/sidebar.rs", 40, 8),
            ("docs/specs/2026-10-15-loom-modified-files-design.md", 191, 0),
        ] {
            changes.upsert(FileChange {
                path: path.into(),
                added,
                removed,
            });
        }
        Self {
            changes,
            counter: 0,
        }
    }
}

impl Demo for FileChangesDemo {
    fn title(&self) -> &str {
        "file_changes"
    }

    fn hint(&self) -> &str {
        "a: añadir fichero · d: quitar el último"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        file_changes::render(f, area, "ficheros modificados", &self.changes, theme);
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char('a') => {
                self.counter += 1;
                self.changes.upsert(FileChange {
                    path: format!("src/nuevo-{}.rs", self.counter),
                    added: 10,
                    removed: 0,
                });
            }
            KeyCode::Char('d') => {
                self.changes.pop();
            }
            _ => return false,
        }
        true
    }
}
```

En `demos/mod.rs`, añade `mod file_changes;` y
`Box::new(file_changes::FileChangesDemo::new())` al catálogo.

- [ ] **Step 4: Ejecutar para ver pasar**

Run: `cargo test -p batten --examples`
Expected: PASS (incluye el smoke del nuevo demo).

- [ ] **Step 5: Commit**

```bash
git add crates/batten
git commit -m "feat(batten): file-changes demo in the showcase"
```

---

### Task 7: Verificación final

**Files:** ninguno.

- [ ] **Step 1: Checks**

Run: `cargo fmt --check`
Expected: PASS.

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.

Run: `cargo test -p batten --examples && cargo test -p batten -p codeloom-core -p codeloom-tui`
Expected: PASS.

- [ ] **Step 2: Verificación manual**

Run: `cargo run -p batten --example showcase` y navega a `file_changes` (`a`/`d`).
Expected: título `ficheros modificados` con totales y una línea por fichero con `+N`/`-M`.

Run: `cargo run -p codeloom-cli --bin loom` y pide una edición de fichero.
Expected: bajo el sidebar aparece `ficheros modificados` con el fichero tocado.

- [ ] **Step 3: Commit** (si hubo ajustes de formato)

```bash
git add -A
git commit -m "chore: format after modified-files work"
```

---

## Criterios de aceptación

- [ ] Los ficheros editados aparecen en el sidebar con `+A`/`-R`.
- [ ] Números de git con repo; si no, delta del `edit`.
- [ ] `path` largos se truncan; 0×0 no rompe.
- [ ] `cargo test -p batten --examples`, `cargo test -p batten -p codeloom-core -p codeloom-tui`, `fmt` y `clippy` pasan.
