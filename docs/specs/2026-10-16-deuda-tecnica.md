# Deuda técnica conocida (batten + loom)

> Estado: **registro vivo**. Cada entrada señala un comentario `limitación:` en el
> código. Al abordar una, se diseña (spec/plan) y se borra de aquí y del código.
>
> Fecha: 2026-10-16

| # | Dónde | Limitación | Impacto | Dirección del arreglo |
|---|---|---|---|---|
| 1 | `batten/src/widgets/transcript.rs` (render) | Se compone y se mide todo el historial en cada frame. | CPU/memoria en conversaciones largas. | Caché de filas por línea lógica (invalidada por ancho y por línea modificada) y medir solo lo visible; ver P0 del spec de hallazgos. |
| 2 | `batten/src/state/viewport.rs` (`set_geometry`) | En un resize solo se acota `top`; no se conserva la línea que había arriba. | La lectura "salta" al cambiar el tamaño. | Anclar `top` a (línea lógica, fila dentro de la línea) y recalcular tras el wrap. Depende de 1. |
| 3 | `batten/src/widgets/transcript.rs` (scroll) | `Paragraph::scroll` usa `u16`: más de 65 535 filas no se alcanzan. | Solo en historiales enormes. | Se resuelve con 1: pintar únicamente las líneas visibles con su desplazamiento local. |
| 4 | `batten/src/widgets/profile.rs` (`for_profile`) | Los degradados (logo, progress, panel, spinner) generan RGB al pintar y no se rebajan con el perfil del terminal. | Colores RGB en terminales de 256/16 colores o `NO_COLOR`. | Pasar el perfil a `gradient` (o al `Theme`) para adaptar cada color generado. |
| 5 | `batten/src/widgets/ansi.rs` | Si el parser ANSI falla, se muestra el texto con pérdida y las secuencias rotas quedan visibles. | Salidas de herramientas con escapes corruptos. | Limpiar secuencias de control antes del fallback (como Crush con su remapeo ANSI-16). |
| 6 | `batten/src/state/markdown.rs` (enlaces) | Los enlaces se muestran como `texto (url)`. | Ruido visual; no clicables. | Guardar la URL en la línea y pintar con OSC 8 (`widgets::link`). |
| 7 | `batten/src/state/markdown.rs` (tablas) | Las tablas salen como texto con ` │ ` sin alinear. | Tablas ilegibles. | Widget de tabla con anchos por columna y wrap. |
| 8 | `codeloom-tui/src/permission_flow.rs` | En el diff de `edit`, los números de línea son relativos al fragmento `old`/`new`. | Números que no coinciden con el fichero. | Que el core adjunte el contexto del fichero (o que la TUI lo lea) para numerar con la posición real. |
| 9 | `codeloom-cli/src/main.rs` | `-p` (no interactivo) deniega todas las tools salvo con `--yolo`. | El modo print no puede usar tools sin `--yolo`. | Política de permisos configurable (`codeloom.toml`): lista de tools permitidas en modo print. |

Pendientes de diseño que no son deuda sino trabajo planificado: rueda del ratón
(junto a selección con ratón), resaltado de sintaxis en bloques de código,
selección por celdas.
