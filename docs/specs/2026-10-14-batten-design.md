# `batten` — Widget kit para chats de agentes en el terminal

> Estado: **diseño aprobado** (secciones 1-5 revisadas en sesión).
>
> Fecha: 2026-10-14
>
> Relacionado: `docs/reference/analisis-herdr-crush-dsh.md` (referencia de
> stack/UX). Este crate es **independiente** de CodeLoom.

## 0. Propósito

Una **librería general y publicable** (no específica de CodeLoom) con todo lo
necesario para construir la UI de chat de un agente en el terminal: transcript
con streaming, estados de tools, spinner animado, panel de to-do, composer,
sidebar y markdown. La meta instrumental es poder **desarrollar y ver el front
con independencia del back** (motor, proveedores, permisos), mediante ejemplos
que inyectan eventos falsos.

Nombre: **`batten`** (la pieza del telar que coloca y fija cada hilo nuevo en
el tejido). Libre en crates.io. Sin marca "CodeLoom" dentro.

## 1. Decisiones cerradas

| # | Decisión |
|---|---|
| 1 | **General y publicable**, sin conocimiento de CodeLoom ni del modelo de agente. |
| 2 | **Dos capas**: `state` (sin ratatui) usable sola + `widgets` (ratatui) encima. |
| 3 | Enfoque **híbrido**: estado y widgets propios; deps puntuales probadas. `phi-tui` como **referencia**, no dependencia. |
| 4 | **Cero código de Crush** (su licencia es `FSL-1.1-MIT`, con cláusula *Competing Use*). Solo referencia de UX. |
| 5 | `state` **no** depende de ratatui ni crossterm; `widgets` solo de ratatui. **`batten` no depende de crossterm.** |
| 6 | Tema **first-class**: tokens semánticos en un **struct tipado** + **mapa de extensiones**. |
| 7 | Tests de widgets: **asserts sobre `TestBackend`** en v0; **`insta` (snapshots)** cuando estabilicen (v1). |
| 8 | Ubicación: **miembro del workspace de loom** ahora; **repo propio** cuando estabilice. |
| 9 | Licencia del crate: `MIT OR Apache-2.0`. |

## 2. Arquitectura

```
batten/                 (crate general, publicable)
  src/
    state/              SIN ratatui/crossterm (puro, tests rápidos)
      transcript.rs     items + Line/LineKind (texto plano + spans por offsets)
      stream.rs         deltas -> líneas (streaming)
      viewport.rs       scroll + follow_bottom + ventana visible (matemática)
      tool.rs           máquina de estados de tool
      todo.rs           modelo y orden de la lista de tareas
      selection.rs      anclas/rangos (sin estilo)
      composer.rs       buffer multilínea + cursor
    widgets/            CON ratatui
      theme/            tokens + theme.toml (serde) + Theme::default/test
      transcript.rs     vista del transcript
      tool_item.rs      cabecera + cuerpo por estado + colapsable
      spinner.rs        anim scramble+gradiente, reloj compartido
      todos.rs          caja/panel de tareas
      sidebar.rs        logo (paramétrico) + info
      composer.rs       vista sobre state::composer
      markdown.rs       render con pulldown-cmark
      logo.rs           wordmark reutilizable
    lib.rs
  examples/             binarios de ejemplo (no se publican)
    demo.rs             escenarios por argumento
docs/specs/…            este documento
```

**Frontera dura**: `state` no conoce ratatui (los tests de estado no arrancan
terminal ni backend); `widgets` no conoce crossterm; nada de agente,
proveedores ni permisos dentro del crate.

## 3. Capa `state`

- **`Transcript`**: `Vec<Line>` ya cerradas. `Line { text: String, kind:
  LineKind, spans: Vec<SpanSpec>, detail: Option<LineDetail> }` donde
  `SpanSpec` es `(start, len, token)` **por offsets sobre el texto plano** →
  selección/copia son independientes del estilo.
  `LineKind = User | System | Assistant | Thinking | ToolCall | ToolResult |
  Todo | Plan | Banner | Error`. `LineDetail = Folded{…} | Diff{…}`.
- **`Stream`**: acumula deltas de texto/razonamiento en la línea en curso y la
  cierra en eventos estructurales (fin de turno, tool call, cambio de bloque).
- **`Viewport`**: `scroll_offset`, `follow_bottom`, `window_range(len, rows)`.
- **`ToolItem`**: `ToolStatus = Pending | Running | Success | Error | Canceled
  | AwaitingPermission`; `Collapsed | Expanded`. Estado `Success/Error`
  **derivado** del resultado (no almacenado por separado, evita desincronía).
- **`TodoItem`**: `TodoStatus = Pending | InProgress | Completed`; orden por
  estado (in-progress y completed arriba).
- **`Selection`**: `anchor`/`head` (rangos de líneas), sin estilo.
- **`Composer`**: buffer multilínea + cursor; Enter envía, Shift+Enter salto.
- **Todo el estado es determinista e inyectable**: la animación avanza por
  frames contados, no por reloj real (testeable).

## 4. Capa `widgets`

- **`theme`**: ver §5.
- **`transcript`**: pinta la ventana visible; prefijos por rol resueltos por el
  tema.
- **`tool_item`**: cabecera `icono + nombre + anim` y cuerpo por estado
  (*"Requesting permission…"*, *"Waiting for tool response…"*, *"Canceled."*);
  colapsable. El spinner solo aparece si el tool no ha terminado y no está
  cancelado.
- **`spinner`**: animación *scramble* + gradiente por columna, **reloj único
  compartido**: se arma un tick solo si hay algún item `spinning`
  (`Animatable { spinning() -> bool; advance() -> bool }`), como hace Crush.
- **`todos`**: caja/panel con glifos `✓` (completed) `•` (pending) `→` (in
  progress), ordenados por estado.
- **`sidebar`**: logo (wordmark paramétrico) + info (estado, cwd, modelo).
- **`composer`**: vista sobre `state::Composer` (sin depender de terceros en v0).
- **`markdown`**: `render_markdown(&str) -> Vec<Line>` con `pulldown-cmark`.
- **`logo`**: wordmark reutilizable (nombre + campos `╱` + gradiente).

## 5. Theming

- La capa `state` **no** lleva color: los items llevan *kind* semántico; el
  widget mapea *kind → estilo* vía el tema.
- **Tokens semánticos**: texto (`text`, `text_muted`, `text_dim`, `heading`);
  roles (`user`, `assistant`, `system`, `error`, `warning`, `success`); tool
  (`tool_name`, `tool_pending/running/success/error/canceled`); todo
  (`todo_pending/in_progress/completed`); chrome (`border`, `title`,
  `composer_border`, `statusbar`, `selection_bg`, `scrollbar`); acento
  (`accent_from`, `accent_to`, `logo_field`); **glifos** (iconos de tool/todo,
  frames del spinner, prefijos, separadores).
- **Representación**: un **struct tipado** con un campo por token (seguridad en
  compilación, autocompletado) + un **mapa `extensions`** para tokens
  adicionales/tema externo.
- **Carga**: `Theme::default()` (oscuro) + `theme.toml` **parcial** (serde con
  defaults → solo se sobreescribe lo indicado). Colores en TOML como nombre
  (`"cyan"`), índice (`"indexed:244"`) o hex (`"#00d4ff"`).
- **Precedencia**: defaults ← `theme.toml` ← overrides de la app.
- `Theme::test()` para tests deterministas.
- v1 opcional: respetar `NO_COLOR` degradando a tema sin color.

## 6. Roadmap

- **v0 — chat core**: `theme` + `transcript` (estado y vista) + `spinner` +
  `tool_item` (estados) + `composer`.
- **v1**: `todos`, `sidebar`/`logo`, `markdown`, `selection`/copiar, resultado
  de tool colapsable + diff, snapshots `insta`.
- **v2**: overlays/diálogos, notificaciones, imágenes, resaltado de sintaxis.

## 7. Demo y tests

- **`examples/demo.rs`** con escenarios por argumento: `basic` (streaming),
  `tools` (pending→running→success / error, colapsable), `todos`
  (vacía→in_progress→completed), `states` (permiso, cancelado, error, spinner).
  Usa **la misma API** que consumirá CodeLoom → es el *harness* de integración.
- **Tests**: `state` unitarios (sin backend); `widgets` con `TestBackend` y
  `Theme::test()`; `theme` parseo de TOML parcial → estilos resueltos. Headless.

## 8. Licencias y límites

- **Crush**: `FSL-1.1-MIT` → **no se copia código** (riesgo *Competing Use*).
  Solo referencia de comportamiento/UX.
- **phi-tui**: MIT; no se depende de él. Si se copiara algún fragmento, se
  incluye su aviso MIT.
- **batten**: `MIT OR Apache-2.0`.
- Sin marca CodeLoom dentro; logo paramétrico; tema por defecto neutro.

## 9. Dependencias

- `ratatui` 0.30 (features mínimas), `pulldown-cmark` (markdown),
  `serde` + `toml` (tema), `unicode-width`.
- **Sin crossterm** en el crate. El backend/eventos los pone la app (o un
  `feature = "runtime"` opcional); si se necesita crossterm, se obtiene vía
  `ratatui::crossterm` / `ratatui::backend::CrosstermBackend` para no divergir
  de versión. `ratatui::init`/`restore` cubren raw mode + pantalla alternativa.
- Los `examples` pueden usar crossterm como dev-dependency.

## 10. No-objetivos (YAGNI)

- Nada de LSP/MCP/skills/hooks en la UI (eso es m4 de CodeLoom).
- Nada de orquestación ni de subagentes/jobs (los pinta la app que use el kit).
- No reimplementar lo que ya dan crates sólidas si encajan (markdown).

## 11. Riesgos

- **Scope creep**: "todos los componentes de Crush" es enorme (styles, diffview,
  list, completions, dialog, notification, image, highlight). El roadmap lo
  trocea; v0 es deliberadamente mínimo.
- **Animar bien**: el reloj único y el scramble+gradiente tienen truco; testear
  por frames inyectados evita flakiness.
- **Publicar el doble** (workspace → repo propio) tiene coste; por eso se hace
  solo cuando estabilice.

## 12. Preguntas abiertas

- ¿El composer pasa a `tui-textarea` en v1 o mantenemos el propio?
- ¿`NO_COLOR` en v0 o v1?
- ¿Nombre final del binario de ejemplo (`demo`)?
