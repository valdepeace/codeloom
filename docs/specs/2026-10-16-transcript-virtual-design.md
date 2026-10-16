# Transcript por entradas, caché de layout y render virtual

> Estado: **diseño aprobado**. Siguiente paso: plan de implementación.
>
> Fecha: 2026-10-16
>
> Resuelve las entradas 1, 2 y 3 de `2026-10-16-deuda-tecnica.md`.
> Relacionado: P0 de `2026-10-15-batten-reference-findings-showcase.md`.
> Referencia: Crush (`internal/ui/list/list.go`: items con caché de render por
> ancho, `Versioned.Bump` para invalidar).

## 0. Problema

1. **loom reconstruye el transcript en cada frame** (cada 50 ms):
   `App::transcript()` reparsea el markdown de todos los mensajes y crea todas
   las `Line`.
2. **El widget compone y mide todo el historial** en cada frame
   (`Paragraph::line_count` sobre todas las líneas).
3. **`Paragraph::scroll` es `u16`**: más de 65 535 filas no se alcanzan.
4. **Resize**: solo se acota `top`; la lectura salta.

Objetivo: coste por frame proporcional a lo visible más lo que cambió, sin
perder la posición de lectura al cambiar el ancho.

## 1. Modelo por entradas (`batten::state::transcript`)

```rust
pub enum Format { Plain, Markdown }
pub struct EntryId(usize);           // índice estable (las entradas no se borran)

struct Entry {
    kind: LineKind,
    format: Format,
    source: String,                  // texto original
    lines: Vec<Line>,                // derivadas de `source`
    rev: u64,                        // sube en cada cambio de la entrada
}

pub struct Transcript {
    entries: Vec<Entry>,
    starts: Vec<usize>,              // línea global donde empieza cada entrada
    total_lines: usize,
    next_rev: u64,
}
```

API nueva:

- `push_entry(kind, format, text) -> EntryId`: deriva las líneas
  (`Plain`: una `Line` por línea de texto; `Markdown`: `markdown::parse`).
- `append(id, delta)`: añade al `source`, **re-deriva solo esa entrada**, sube su
  `rev` y recalcula `starts` desde esa entrada.
- `entry_count()`, `entry_rev(i)`, `entry_lines(i) -> &[Line]`,
  `entry_start(i) -> usize` (línea global de inicio).
- `locate(line) -> (entry, offset)`: búsqueda binaria en `starts`.

API que se mantiene (vista por líneas globales, para `Selection`, `Stream` y
tests): `new`, `push(Line)` (crea una entrada `Plain` de una línea con esa
`Line` tal cual), `len()` (= `total_lines`), `line(i)`, `is_empty()`, `iter()`.
`last_mut()` se elimina (solo lo usa un test) — si algún consumidor lo necesita,
se sustituye por `append`.

Invariantes: `starts[i]` = suma de líneas de las entradas `< i`; `rev` es
estrictamente creciente en el transcript (un contador global).

## 2. Caché de layout (`batten::widgets::transcript::Layout`)

Estado de vista que crea y guarda quien usa el widget (no es `state` puro
porque mide con ratatui).

```rust
pub struct Layout {
    width: u16,
    entries: Vec<EntryLayout>,       // por entrada: rev medida y alturas por línea
    row_starts: Vec<usize>,          // fila global donde empieza cada línea
    total_rows: usize,
    measured: usize,                 // contador de entradas medidas (tests)
}
struct EntryLayout { rev: u64, heights: Vec<usize> }
```

- `sync(&mut self, transcript, width, theme)`:
  - si cambia `width`, invalida todo;
  - para cada entrada cuyo `rev` no coincide (o nueva), mide sus líneas
    (`render_line` + `Paragraph::wrap(..).line_count(width)`), cuenta
    `measured += 1`;
  - reconstruye `row_starts`/`total_rows` desde la primera entrada cambiada.
- `row_of(line) -> usize`, `height_of(line) -> usize`,
  `line_at_row(row) -> (line, row_in_line)` (búsqueda binaria), `total_rows()`,
  `measured()`.

El ancho medido es el del área de texto: si hay overflow se resta la columna de
la scrollbar. Para decidir el overflow se sincroniza primero con el ancho
completo; si el total supera las filas, se vuelve a sincronizar con `ancho - 1`
(el segundo `sync` re-mide todo solo cuando aparece/desaparece la barra).

## 3. Render virtual y ancla

`render(frame, area, transcript, viewport, layout, selection, theme)` (una sola
función; `selection: Option<&Selection>`):

1. `layout.sync(...)` (con la lógica de overflow de §2).
2. Ancla: si el ancho cambió, `viewport` no sigue el final y tiene ancla
   `(line, row)`, `top = row_of(line) + min(row, height_of(line) - 1)` →
   `viewport.restore_top(top)`.
3. `viewport.set_geometry(total_rows, rows)` (acota).
4. Selección: `viewport.ensure_visible(row_of(head), row_of(head) + height_of(head))`.
5. `(first, skip) = line_at_row(top)`; se componen solo las líneas desde
   `first` hasta llenar `rows` (más una de margen) y se pintan con
   `Paragraph::scroll((skip, 0))` (`skip` < altura de una línea: cabe en `u16`).
6. Se guarda el ancla nueva `viewport.set_anchor(first, skip)`.
7. Scrollbar con `viewport.scroll()`.

`Viewport` (estado puro) gana `anchor: Option<(usize, usize)>`,
`set_anchor(line, row)`, `anchor()`, `restore_top(top)` (fija `top` sin cambiar
`follow`). El resto de su API no cambia.

## 4. Consumidores

- **loom (`codeloom-tui`)**: `App` guarda `transcript: Transcript`,
  `layout: Layout` y `assistant: Option<EntryId>`. `apply`:
  - `MessageAdded(User)` → `push_entry(User, Plain, text)`; `assistant = None`;
  - `Delta { text }` → si hay `assistant`, `append`; si no,
    `push_entry(Assistant, Markdown, text)` y se guarda el id;
  - `TurnEnded` → `assistant = None`.
  `items`/`TranscriptItem` y `App::transcript()` desaparecen; los tests usan
  helpers (`App::push_user`, `App::push_assistant`) que pasan por los mismos
  caminos que `apply`.
- **Demo `chat`**: los deltas van a `append` sobre la entrada de la respuesta
  (sin `Stream` ni copia del transcript por frame).
- **Demo `transcript`** y tests de batten: añaden un `Layout`.

## 5. Tests

- `Transcript`: `push_entry`/`append` mantienen `starts`, `len`, `line(i)`;
  `append` solo cambia la `rev` de su entrada; `locate` en bordes.
- `Layout`: tras un `append` en la última de 1 000 entradas, `measured` sube en 1;
  cambio de ancho re-mide todo; `line_at_row` en bordes y con líneas que ocupan
  varias filas.
- Render: con 10 000 líneas se pinta la última al seguir el final; leyendo
  arriba, un `append` no mueve la vista; tras un cambio de ancho la línea del
  ancla sigue arriba; filas > 65 535 alcanzables (`top` grande).
- loom: deltas consecutivos crean una sola entrada; tras `MessageAdded` el
  siguiente delta crea otra.

## 6. No-objetivos

- Parseo incremental del markdown (prefijo estable de Crush): se reparsea la
  entrada que crece entera.
- Entradas de razonamiento (`ReasoningDelta`) y de tools en el transcript.
- Borrado o edición de entradas antiguas.
