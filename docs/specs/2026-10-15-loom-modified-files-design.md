# Ficheros modificados en el sidebar (batten + core)

> Estado: **diseño aprobado** (secciones 1-7 revisadas en sesión).
>
> Fecha: 2026-10-15
>
> Relacionado: `docs/specs/2026-10-14-batten-design.md`,
> `docs/specs/2026-10-15-batten-sidebar-logo-design.md`,
> `docs/specs/2026-10-13-loom-design.md`. Estilo de referencia: el bloque
> *"Modified Files"* de Crush.

## 0. Propósito

Mostrar, en la columna del sidebar, los ficheros que se han tocado en la sesión
con sus líneas añadidas/quitadas (`+A -R`), como hace Crush. Sirve para ver de un
vistazo qué está cambiando el agente sin salir de la TUI.

## 1. Decisiones cerradas

| # | Decisión |
|---|---|
| 1 | En batten: **componente independiente** (`state::FileChanges` + `widgets::file_changes`). El `Sidebar` no cambia. |
| 2 | En core: evento nuevo `LiveEvent::FileDiff { path, added, removed }` y un `ChangeTracker`. |
| 3 | Los números son **git con respaldo**: `git diff --numstat` si hay repo; si no, el conteo de líneas de `old`/`new` del `edit`. |
| 4 | Solo se detectan escrituras de la tool **`edit`**; las hechas por `bash` quedan invisibles. |
| 5 | En codeloom-tui el widget va **bajo el sidebar**, en la columna derecha. |
| 6 | Título por defecto: **`ficheros modificados`** (coherente con el resto en español del sidebar). Lo pone la app, no batten. |
| 7 | En el showcase hay una demo `file_changes` con datos falsos. |

## 2. Arquitectura y archivos

```
crates/batten/src/
  state/file_changes.rs     # NUEVO: FileChange, FileChanges (puro)
  state/mod.rs              # + pub mod file_changes; + re-exports
  widgets/file_changes.rs   # NUEVO: render(title, changes, theme)
  widgets/mod.rs            # + pub mod file_changes
crates/codeloom-core/src/
  event/mod.rs              # + LiveEvent::FileDiff
  agent/changes.rs          # NUEVO: ChangeTracker
  agent/mod.rs              # + ChangeTracker; publica FileDiff tras cada edit
crates/codeloom-tui/src/
  app.rs                    # + changes: FileChanges; + apply(FileDiff)
  render.rs                 # + file_changes bajo el sidebar
crates/batten/examples/showcase/demos/
  file_changes.rs           # NUEVO: demo con datos falsos
  mod.rs                    # + al catálogo
```

## 3. batten: `state::FileChanges`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    pub added: usize,
    pub removed: usize,
}

#[derive(Debug, Default, Clone)]
pub struct FileChanges { files: Vec<FileChange> }

impl FileChanges {
    pub fn new() -> Self;
    /// Inserta o reemplaza por `path` (no duplica). Conserva el orden de la
    /// primera aparición de cada fichero.
    pub fn upsert(&mut self, change: FileChange);
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    pub fn iter(&self) -> impl Iterator<Item = &FileChange>;
    /// Suma de (added, removed) de todos los ficheros.
    pub fn totals(&self) -> (usize, usize);
}
```

Puro: sin ratatui, sin crossterm.

## 4. batten: `widgets::file_changes`

```rust
pub fn render(frame: &mut Frame, area: Rect, title: &str, changes: &FileChanges, theme: &Theme);
```

- Línea 0: `title` (estilo `Heading`) + los totales ` +A` (`Success`) y ` -R`
  (`Error`).
- Una línea por fichero (hasta `area.height - 1`): `path` truncado por **anchura
  de display** (`unicode-width`, con `…`) en estilo `Muted`, y ` +N` (`Success`)
  + ` -M` (`Error`).
- Sin tokens de tema nuevos.
- `area` 0×0 no entra en pánico; sin ficheros pinta solo el título.

## 5. core: evento y tracker

En `event::LiveEvent`:

```rust
FileDiff { path: String, added: usize, removed: usize },
```

Números **absolutos por fichero** ya resueltos (el consumidor solo guarda el
último valor por `path`).

`agent::changes::ChangeTracker`:

```rust
pub struct ChangeTracker { cwd: PathBuf, git: bool }

impl ChangeTracker {
    /// `git = true` si `git -C <cwd> rev-parse --is-inside-work-tree` responde ok.
    pub fn detect(cwd: impl Into<PathBuf>) -> Self;
    /// Números para un `edit` de `old` a `new`.
    pub fn numbers(&self, path: &str, old: &str, new: &str) -> (usize, usize);
}
```

`numbers`:
- Si `git`: `git -C <cwd> diff --numstat -- <path>`; parsea la primera línea
  `add\tremoved\tpath` (ambos numéricos) → esos números. Si el comando falla o
  la salida no es numérica (p. ej. fichero nuevo/untracked), cae al respaldo.
- Respaldo: `(new.lines().count(), old.lines().count())`. Es exacto para `edit`
  porque reemplaza exactamente `old` por `new` (sin contexto).

Sin git instalado, `detect` no entra en pánico (marca `git = false`).

## 6. core: emisión en el agent loop

En `AgentLoop::run`, al procesar `results` de `execute_tools`, emparejando por
`id` con las llamadas aprobadas:

- por cada resultado **sin error** cuya tool sea `edit`, lee `path`/`old`/`new`
  del input, pide `numbers` al `ChangeTracker` y publica
  `LiveEvent::FileDiff { path, added, removed }` en el bus.

`AgentLoop` gana un campo `changes: ChangeTracker`, construido en `new(cwd)`.

## 7. codeloom-tui

- `App` gana `pub changes: batten::state::FileChanges`.
- `App::apply`:
  ```rust
  LiveEvent::FileDiff { path, added, removed } =>
      self.changes.upsert(batten::state::FileChange { path, added, removed }),
  ```
- `render.rs` (rama ancha): la columna derecha (`Length(30)`) se parte en
  **sidebar** (`Min(1)`) + **ficheros modificados** (`Length(min(n + 2, alto - 1))`)
  cuando `!app.changes.is_empty()`; si está vacío, solo el sidebar.

## 8. showcase: demo `file_changes`

Demo con 3 ficheros falsos; `a` añade un fichero ficticio, `d` quita el último.
Se pinta con `widgets::file_changes::render(f, area, "ficheros modificados", …)`.
Entra en el catálogo (el smoke headless lo cubre).

## 9. Tests

- **batten `FileChanges`**: `upsert` reemplaza por `path` sin duplicar y conserva
  el orden; `totals` suma; `is_empty`/`len`.
- **batten widget**: `TestBackend` muestra el título, un `path` y `+N`/`-M`;
  0×0 sin pánico; trunca un path largo con `…`.
- **core `ChangeTracker`**: con `git = false`, `numbers` cuenta líneas de
  `old`/`new` (incluye borrado total y creación); `detect` sobre un dir sin git
  no entra en pánico.
- **core agent**: con `EditTool` real sobre un fichero temporal y un provider de
  guion que emite la llamada `edit`, el bus recibe un `FileDiff` con los números
  esperados.
- **codeloom-tui**: `apply(FileDiff)` actualiza `changes`; `render` a 120×40
  muestra el título y el path.
- **showcase**: smoke del catálogo (automático) + test de `a`/`d`.

Checks: `cargo test -p batten --examples`, `cargo test -p codeloom-core -p codeloom-tui`,
`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`.

## 10. Criterios de aceptación

- [x] Los ficheros editados aparecen debajo del sidebar con `+A`/`-R`.
- [x] Los números son de git cuando hay repo; si no, delta del `edit`.
- [x] Los `path` largos se truncan; 0×0 no rompe.
- [x] Los checks pasan.

## 11. Fuera de alcance

- Escrituras de `bash` (no se detectan).
- Vista de diff por hunk, colores por línea, abrir el fichero, ver el contenido.
- Persistir la lista en la sesión (es estado de UI de la TUI).

## 12. Preguntas abiertas

- Con git, los números son *cambio actual vs HEAD* (incluyen lo previo a la
  sesión); sin git son *delta de la sesión*. Si se quiere coherencia total,
  habría que quedarse solo con el delta de sesión.
- ¿El orden debe ser el de primera aparición (actual) o el más reciente primero?
