# Diálogo de permisos (batten + loom)

> Estado: **diseño aprobado**. Siguiente paso: plan de implementación.
>
> Fecha: 2026-10-15
>
> Relacionado: `docs/specs/2026-10-15-batten-primitives-design.md` (panel,
> key_help, chip), `docs/plans/2026-10-15-batten-integrations.md` (`fx`).
> Referencia de UX: Crush (`internal/ui/dialog/permissions.go`,
> `internal/ui/diffview/`); filas de diff: `.references/claude-code-rust/src/ui/diff.rs`
> y `.references/herdr-reviewr/src/diff.rs` (ver
> `2026-10-15-batten-reference-findings-showcase.md` §3 y P3: la app suministra
> los textos, batten no lee ficheros ni ejecuta Git).

## 0. Contexto y problema

Hoy loom **no pide permiso**: `codeloom-cli/src/main.rs` construye
`PermissionService::allow_all()`, el flag `--yolo` se ignora y
`UiPermissionResponder` (en `codeloom-tui`) no está conectado. El agente ejecuta
`bash` y `edit` sin preguntar.

Objetivo: un diálogo de permisos en batten (con diff unified para ediciones) y su
integración en loom, de modo que cada tool pida confirmación salvo con `--yolo`.

## 1. batten

### 1.1 `state::Diff`

```rust
pub enum DiffKind { Hunk, Context, Insert, Delete }
pub struct DiffLine {
    pub kind: DiffKind,
    pub old_no: Option<usize>, // 1-based; None en Insert/Hunk
    pub new_no: Option<usize>, // 1-based; None en Delete/Hunk
    pub text: String,          // sin salto de línea final
}
pub struct Diff { lines: Vec<DiffLine> }
impl Diff {
    pub fn new(before: &str, after: &str, context: usize) -> Self;
    pub fn lines(&self) -> &[DiffLine];
    pub fn added(&self) -> usize;
    pub fn removed(&self) -> usize;
    pub fn is_empty(&self) -> bool; // sin cambios
}
```

- Cálculo por líneas con la crate **`similar`** (Rust puro, sin dependencias
  transitivas), agrupado en hunks con `context` líneas de contexto.
- Cada hunk empieza con una `DiffLine` `Hunk` cuyo texto es
  `@@ -a,b +c,d @@` (convención unified).
- Textos iguales → `Diff` vacío.

### 1.2 `widgets::diff`

`render(frame, area, diff: &Diff, scroll: usize, theme) `

- Una fila por `DiffLine`, empezando en `scroll` (acotado al final).
- Gutter: número viejo y número nuevo, cada uno alineado a la derecha al ancho
  del mayor número, en `dim`, separados por un espacio; luego el símbolo
  (`+`, `-`, ` `) y el texto.
- Estilos: `Insert` → token nuevo `diff_insert` (verde), `Delete` →
  `diff_delete` (rojo), `Hunk` → `diff_hunk` (acento/cian, fila entera sin
  gutter de números), `Context` → `default`.
- Texto más ancho que el área: truncado con `…`. Tabuladores → 4 espacios.
- Área sin altura o anchura → no pinta.

### 1.3 `widgets::button`

- `button(label: &str, focused: bool, tone: Tone, theme) -> Span<'static>`:
  `" label "`; enfocado → fondo del color del tono, texto negro, negrita; sin
  foco → `muted`.
- `button_row(items: &[(&str, Tone)], selected: usize, theme) -> Line<'static>`:
  botones separados por dos espacios.

### 1.4 `state::PermissionDialog` + `widgets::permission`

```rust
pub enum PermissionChoice { Allow, AllowSession, Deny }
pub enum PermissionBody { Text(String), Diff(Diff) }
pub struct PermissionDialog {
    pub tool: String,
    pub subtitle: String,       // p. ej. ruta del fichero o vacío
    pub body: PermissionBody,
    choice: PermissionChoice,   // por defecto Allow
    scroll: usize,
}
impl PermissionDialog {
    pub fn new(tool, subtitle, body) -> Self;
    pub fn choice(&self) -> PermissionChoice;
    pub fn next(&mut self); pub fn prev(&mut self); // cíclicos
    pub fn scroll_by(&mut self, delta: isize);     // acotado a >= 0
    pub fn scroll(&self) -> usize;
}
```

Sin teclas en batten: la app traduce teclas a estos métodos.

`widgets::permission`:

- `area(screen: Rect) -> Rect`: rectángulo centrado del 80 % del ancho (máximo
  100 columnas) y 80 % del alto (mínimo 8 filas, o la pantalla si es menor).
- `render(frame, screen, dialog, theme)`: limpia `area(screen)` (`Clear`), pinta
  `panel` con título `"Permiso"`, info = `tool`, tono `Warning`; dentro:
  1. `subtitle` en `muted` (si no está vacío);
  2. el cuerpo (texto con wrap, o `diff` con el scroll del diálogo) ocupando el
     espacio restante;
  3. una fila vacía, la fila de botones `Permitir` (Success) ·
     `Permitir en la sesión` (Info) · `Denegar` (Error) con el seleccionado
     enfocado;
  4. `key_help`: `←→ elegir · enter confirmar · a/s/d atajo · ↑↓ scroll · esc denegar`.
- La app puede aplicar `fx::dialog_open` sobre `area(screen)`.

### 1.5 Tema

Tokens nuevos `diff_insert` (`Green`), `diff_delete` (`Red`), `diff_hunk`
(`Cyan`), con nombre TOML igual al campo.

### 1.6 Showcase

Demo `permission`: alterna con `Tab` entre un `edit` (diff de ejemplo) y un
`bash` (comando); `←/→` cambia la opción; `↑/↓` hace scroll; `o` relanza el
efecto de apertura.

## 2. core

- `Decision::AllowSession`. `PermissionService::check`: si el responder devuelve
  `AllowSession`, guarda la clave `tool:action` como persistente y devuelve
  `Allow` (los llamantes nunca ven `AllowSession`).
- Sin más cambios: se sigue pidiendo permiso por cada tool.

## 3. loom

- **CLI**: sustituye `allow_all()` por `with_responder(UiPermissionResponder::new(tx, cli.yolo))`
  y pasa el `rx` a la TUI. Con `--yolo` todo se permite sin preguntar.
- **TUI**: al recibir `(PermissionRequest, oneshot::Sender<Decision>)`:
  - construye el cuerpo a partir de `detail` (JSON del input):
    `edit` → `Diff::new(old, new, 3)` y `subtitle = path`;
    `bash` → `Text(command)`; resto → `Text(json formateado)`;
    si el JSON no parsea → `Text(detail)`.
  - abre el diálogo (lanza `fx::dialog_open`), captura el teclado mientras esté
    abierto: `←/→/Tab` elegir, `Enter` confirmar, `a`/`s`/`d` atajos (Allow,
    AllowSession, Deny), `↑/↓` scroll, `Esc` Deny.
  - responde por el `oneshot` y cierra. Si llegan varias peticiones, se encolan
    y se muestran de una en una.

## 4. Tests

- `Diff`: textos iguales → vacío; una línea cambiada → Hunk + Delete + Insert con
  números correctos; contexto recortado; `added/removed`.
- `widgets::diff`: gutter y símbolos en `TestBackend`; truncado; scroll acotado.
- `button`/`button_row`: estilos enfocado/no enfocado, texto.
- `PermissionDialog`: ciclo de `next/prev`, scroll no negativo.
- `widgets::permission`: `area` centrada y acotada; render sin pánico en 0×0,
  20×5 y 120×40; botones visibles.
- core: `AllowSession` persiste y la siguiente petición igual se permite sin
  preguntar al responder.
- TUI: mapeo de `detail` a cuerpo (edit/bash/otro/JSON inválido) y de teclas a
  decisión.

## 5. No-objetivos

Vista split, resaltado de sintaxis, fondos de color en el diff, ratón, permisos
por patrón (`bash:git *`), exención automática de tools de solo lectura.
