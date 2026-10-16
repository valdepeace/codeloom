# Deuda técnica conocida (batten + loom)

> Estado: **registro vivo**. Cada entrada señala un comentario `limitación:` en el
> código. Al abordar una, se diseña (spec/plan) y se borra de aquí y del código.
>
> Fecha: 2026-10-16

| # | Dónde | Limitación | Impacto | Dirección del arreglo |
|---|---|---|---|---|
| 4 | `batten/src/widgets/profile.rs` (`for_profile`) | Los degradados (logo, progress, panel, spinner) generan RGB al pintar y no se rebajan con el perfil del terminal. | Colores RGB en terminales de 256/16 colores o `NO_COLOR`. | Pasar el perfil a `gradient` (o al `Theme`) para adaptar cada color generado. |
| 5 | `batten/src/widgets/ansi.rs` | Si el parser ANSI falla, se muestra el texto con pérdida y las secuencias rotas quedan visibles. | Salidas de herramientas con escapes corruptos. | Limpiar secuencias de control antes del fallback (como Crush con su remapeo ANSI-16). |
| 6 | `batten/src/state/markdown.rs` (enlaces) | Los enlaces se muestran como `texto (url)`. | Ruido visual; no clicables. | Guardar la URL en la línea y pintar con OSC 8 (`widgets::link`). |
| 7 | `batten/src/state/markdown.rs` (tablas) | Las tablas salen como texto con ` │ ` sin alinear. | Tablas ilegibles. | Widget de tabla con anchos por columna y wrap. |
| 8 | `codeloom-tui/src/permission_flow.rs` | En el diff de `edit`, los números de línea son relativos al fragmento `old`/`new`. | Números que no coinciden con el fichero. | Que el core adjunte el contexto del fichero (o que la TUI lo lea) para numerar con la posición real. |
| 9 | `codeloom-cli/src/main.rs` | `-p` (no interactivo) deniega todas las tools salvo con `--yolo`. | El modo print no puede usar tools sin `--yolo`. | Política de permisos configurable (`codeloom.toml`): lista de tools permitidas en modo print. |

Resueltas el 2026-10-16: 1, 2 y 3 (ver 2026-10-16-transcript-virtual-design.md).

Pendientes de diseño que no son deuda sino trabajo planificado: rueda del ratón
(junto a selección con ratón), resaltado de sintaxis en bloques de código,
selección por celdas.
