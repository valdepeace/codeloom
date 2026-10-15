# batten — Idea: selección de bloques y copiar

> Estado: **idea en exploración**. No implementar.
>
> Fecha: 2026-10-15
>
> Relacionado: `docs/specs/2026-10-14-batten-design.md` (§3 `selection`, §6
> roadmap v1: `selection`/copiar). Referencia de UX: la selección por bloques
> de Crush.

## 0. La idea

Poder **seleccionar** bloques del transcript (una línea/item, o un rango) y
**copiarlos**: resaltado de la selección, navegación con teclado (y/o ratón) y
copiar al portapapeles. Opcionalmente, expandir/colapsar un tool item al
seleccionarlo.

## 1. Modelo (state, puro)

El diseño de batten ya prevé `state::selection` (sin estilo):

```rust
pub struct Selection { pub anchor: usize, pub head: usize } // índices de Line
```

- `anchor`/`head` definen un rango `[min, max]` de líneas; el widget resuelve el
  resaltado (token `selection_bg` previsto en el tema).
- Selección **por bloques** (líneas enteras) primero; por texto (columnas)
  después.

## 2. Interacción

- Teclado: entrar en modo selección, `Shift+↓/↑` extiende, `Enter`/`y` copia,
  `Esc` sale.
- Ratón: opcional (necesita eventos de ratón del backend; fuera de batten, que
  no conoce crossterm).
- Copiar: no hay API universal en terminal. Opciones: **OSC 52** (funciona por
  SSH sin dependencias) o un crate de portapapeles (`arboard`). Es decisión de
  la **app**, no de batten: batten expone el **texto seleccionado** y quien
  integra decide cómo copiarlo.

## 3. Encaje

- **batten `state::Selection`** (puro) + **`widgets`** que resaltan el rango
  usando el token de fondo de selección.
- `Transcript`/`Viewport` ya dan la ventana visible; la selección se mapea a
  líneas de esa ventana.
- La **app** (codeloom-tui) gestiona el modo, el teclado y el copiado.

## 4. Gotchas

- **Resaltado por líneas, no por spans**: hoy `Line` lleva spans con offsets;
  resaltar por encima del estilo requiere componer el fondo sobre los spans.
- **Terminal sin portapapeles**: degradar con elegancia (mostrar "copiado" solo
  si funcionó) o usar OSC 52.
- **Anchura de display**: el resaltado debe respetar `unicode-width`.
- `state` no conoce ratatui ni crossterm: los eventos de ratón los traduce la app.

## 5. Preguntas abiertas

- ¿Selección por líneas o por celdas de texto?
- ¿Teclado, ratón o ambos?
- ¿OSC 52 o crate de portapapeles?
- ¿La selección permite expandir/colapsar el bloque (tool item) o solo copiar?
