# `batten` — showcase navegable de widgets

> Estado: **diseño aprobado** (secciones 1-9 revisadas en sesión).
>
> Fecha: 2026-10-15
>
> Relacionado: `docs/specs/2026-10-14-batten-design.md`,
> `docs/specs/2026-10-15-batten-sidebar-logo-design.md`,
> `docs/plans/2026-10-14-batten-v0.md`. Continúa el *harness* de desarrollo que
> hoy es `examples/demo.rs` y sienta la base de la futura página de componentes
> del crate.

## 0. Propósito

Ver y probar **todos los widgets** de batten de forma autónoma, sin app ni back,
como en un *showcase* de librería de componentes (estilo PrimeNG pero en la
terminal): un índice de widgets a la izquierda y, a la derecha, la demo del
widget seleccionado, con la que se puede interactuar.

Servirá, por orden: (1) herramienta de desarrollo para ver los widgets según se
construyen; (2) base de la documentación viva del crate cuando estabilice.

## 1. Decisiones cerradas

| # | Decisión |
|---|---|
| 1 | Forma: **storybook navegable en la TUI** (índice + panel de demo). |
| 2 | Cobertura: los **7 widgets** (transcript, tool_item, composer, spinner, logo, sidebar, theme) **+ una pantalla `chat`** que los compone. |
| 3 | **Interactivo por widget**: el teclado actúa dentro de cada demo. |
| 4 | v0 **sin panel de código/snippets**; se añade en una iteración siguiente. |
| 5 | Vive como **ejemplo multi-archivo** de batten (`examples/showcase/`); el crate publicable no cambia y sigue sin crossterm (solo el bucle de eventos lo usa, vía `ratatui::crossterm`). |
| 6 | El filtro `KeyEventKind::Press` (ver `demo.rs`) se **reusa** en el bucle del showcase. |
| 7 | `examples/demo.rs` se **retira**: el showcase lo cubre y más. |

## 2. Arquitectura y archivos

```
crates/batten/examples/showcase/
  main.rs        # bucle de eventos, layout, navegación, salida
  app.rs         # estado del showcase: catálogo, selección, foco
  demo.rs        # trait Demo
  demos/
    mod.rs       # re-exporta los demos y `catalog()`
    transcript.rs
    tool_item.rs
    composer.rs
    spinner.rs
    logo.rs
    sidebar.rs
    theme.rs
    chat.rs
```

Cargo trata `examples/<nombre>/main.rs` como un ejemplo llamado `showcase`:
`cargo run -p batten --example showcase`.

Sin dependencias nuevas: los demos usan `batten::state` / `batten::widgets`;
solo `main.rs` usa `ratatui::crossterm` (re-export), igual que el demo actual.

## 3. Modelo

```rust
/// Un widget (o composición) demostrable en el showcase.
pub trait Demo {
    /// Título en el índice.
    fn title(&self) -> &str;
    /// Línea de ayuda con las teclas propias del demo.
    fn hint(&self) -> &str;
    /// Pinta la demo dentro de `area`.
    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme);
    /// Avanza la animación (se llama una vez por frame). Por defecto no hace nada.
    fn tick(&mut self, anim: &Anim) {}
    /// Procesa una tecla. Devuelve `true` si el demo la consume. En v0 el
    /// valor de retorno se ignora; queda reservado para uso futuro.
    fn handle_key(&mut self, code: KeyCode) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus { Index, Demo }

pub struct Showcase {
    demos: Vec<Box<dyn Demo>>,
    selected: usize,
    focus: Focus,
    quit: bool,
}
```

`Showcase` expone:

- `new() -> Self` — construye el catálogo desde `demos::catalog()`.
- `selected(&self) -> usize`, `focus(&self) -> Focus`, `should_quit(&self) -> bool`.
- `len(&self) -> usize`, `title_at(&self, i) -> Option<&str>` (para los tests).
- `next(&mut self)` / `prev(&mut self)` — mueven la selección con *clamp* (no
  cicla) y **resetean el foco a `Index`**.
- `toggle_focus(&mut self)` — alterna `Index`/`Demo`.
- `current(&mut self) -> &mut dyn Demo`.
- `tick(&mut self, anim: &Anim)` — llama `tick` **solo del demo activo**.
- `handle_key(&mut self, code: KeyCode)` — enrutado (ver §5).

**Animación**: un único `Anim` compartido avanza cada `FRAME_INTERVAL`
(reutilizado de `batten::widgets`). `Showcase::tick` se llama en cada frame y
delega en el demo activo para que avance su animación.

## 4. Catálogo y demos

Cada demo es autónomo: mantiene su propio estado (sin back) y muestra sus
estados internamente.

| Índice | Qué muestra | Teclas propias |
|---|---|---|
| `transcript` | Un `Transcript` con una línea de cada `LineKind` (User, System, Assistant, Thinking, ToolCall, ToolResult, Todo, Plan, Banner, Error) y un `Viewport` | `↑/↓` scroll, `g`/`G` inicio/fin |
| `tool_item` | Varios `ToolItem` en estados Pending/Running/Success/Error/Canceled/AwaitingPermission; uno seleccionado | `↑/↓` mueve, `n` cicla estado del seleccionado, `Enter`/`x` expande/colapsa |
| `composer` | Un `Composer` real; lo enviado se apila encima como líneas `you:` | escribir, `Enter` envía, `Backspace` borra |
| `spinner` | Un `Anim` con scramble + degradado sobre un texto | (sin teclado propio; anima por frame) |
| `logo` | Un `Wordmark` del nombre actual, con degradado; cae a compacto si no cabe | escribir cambia el nombre, `Backspace` borra |
| `sidebar` | Un `Sidebar` con logo + secciones | `+` añade sección, `-` quita la última |
| `theme` | Un swatch de todos los `StyleToken`, alternando el tema por defecto y un `theme.toml` parcial | `t` alterna tema |
| `chat` | Composición completa: transcript con *streaming* simulado por tiempo, un `ToolItem`, el `Sidebar` y un `Composer` funcional | escribir, `Enter` envía, `↑/↓` scroll del transcript |

Los estados de la tool y del spinner avanzan por `tick` (tiempo), de modo que
también se ven "vivos" sin tocar el teclado.

## 5. Navegación y teclas

Teclas **globales** (las procesa `main.rs` antes que el demo):

- `Tab` — alterna foco índice↔demo.
- Con foco en el índice: `↑/↓` o `j/k` mueven la selección; `q` sale.
- `Esc` — si el foco está en el demo, vuelve al índice.
- `Ctrl+C` — sale siempre.
- Cualquier otra tecla, con foco en el demo, se pasa a `Demo::handle_key`; el
  valor de retorno se ignora en v0 (el demo decide qué hacer con ella).

Enrutado en una función pura y testeable:

```rust
pub enum Action { Quit, None }
pub fn route(show: &mut Showcase, kind: KeyEventKind, code: KeyCode) -> Action;
```

`route` aplica el filtro `kind == KeyEventKind::Press` (si no, no hace nada) y
resuelve las teclas globales; el resto va al demo si el foco está en el demo.

## 6. Layout

- Columna izquierda: índice de widgets (ancho fijo 26). Si `area.width < 70`,
  el índice se colapsa a **una línea** con `‹ título ›` y `Tab` sigue cambiando
  el widget.
- Columna derecha: cabecera (título + `hint` del demo) y el panel del demo con
  el resto del alto.
- Barra inferior de una línea con las teclas globales (`Tab foco · ↑↓ widget ·
  q salir`).

Tamaños 0/1 no deben entrar en pánico (índice colapsado, panel vacío).

## 7. Tema

`Theme::default_dark()` para el showcase. El demo `theme` mantiene **dos**
temas: el por defecto y uno cargado con `Theme::from_toml(..)` parcial, para
verlos lado a lado al pulsar `t`.

## 8. Tests

El punto de "probar de forma autónoma". Viven en el propio ejemplo y se
ejecutan con `cargo test -p batten --examples`.

- **Estado (`app`)**: `new` arma 8 demos con títulos únicos; `next`/`prev`
  mueven la selección con *clamp*; cambiar de widget resetea el foco a `Index`;
  `toggle_focus` alterna.
- **Enrutado (`route`)**: un `Release` no hace nada; `Ctrl+C` devuelve `Quit`;
  `q` con foco índice devuelve `Quit`; `Tab` cambia el foco; con foco en el
  demo, una tecla llega al `handle_key` del demo (p. ej. el composer acumula
  texto).
- **Smoke headless de cada demo**: para **cada** demo del catálogo, renderizar
  con `TestBackend` a (0×0), (1×1), (40×10) y (120×40) y verificar que no entra
  en pánico; además, que al menos un par de demos muestran contenido esperado
  (p. ej. `tool_item` muestra un nombre de tool; `composer` refleja lo tecleado).
- **Comportamiento por demo**: `composer` (escribir + `Enter` añade línea),
  `tool_item` (`n` cambia el estado del seleccionado).

## 9. Criterios de aceptación

- [ ] `cargo run -p batten --example showcase` abre el índice con los 8 demos y
      permite navegar y probarlos con teclado.
- [ ] Los 7 widgets y la pantalla `chat` se ven y responden.
- [ ] `cargo test -p batten --examples` pasa (estado, enrutado y smoke headless
      de todos los demos).
- [ ] `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`
      y `cargo test` (lib) siguen pasando.
- [ ] `examples/demo.rs` retirado; `batten` sigue sin depender de crossterm.

## 10. Fuera de alcance (v0)

- Panel de código/snippets, enlaces al repo, búsqueda de widgets.
- Temas provistos por el usuario o cambio de tema global del showcase.
- Promover el showcase a *feature* del crate o a página web (se hará cuando
  estabilice y exista la página de componentes).

## 11. Preguntas abiertas

- ¿Un demo por widget basta, o querrás varias *stories* por widget (estilo
  PrimeNG: "tool item / colapsable", "tool item / permiso")? v0 = uno por widget
  con sus estados dentro.
- ¿`chat` debe simular también *streaming* carácter a carácter o basta con
  aparecer por bloques?
