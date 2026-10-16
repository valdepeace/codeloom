# Deuda técnica conocida (batten + loom)

> Estado: **registro vivo**. Cada entrada señala un comentario `limitación:` en el
> código. Al abordar una, se diseña (spec/plan) y se borra de aquí y del código.
>
> Fecha: 2026-10-16

| # | Dónde | Limitación | Impacto | Dirección del arreglo |
|---|---|---|---|---|
| 6 | `batten/src/state/markdown.rs` (enlaces) | Los enlaces se muestran como `texto (url)`. | Ruido visual; no clicables. | Guardar la URL en la línea y pintar con OSC 8 (`widgets::link`). |
| 7 | `batten/src/state/markdown.rs` (tablas) | Las tablas salen como texto con ` │ ` sin alinear. | Tablas ilegibles. | Widget de tabla con anchos por columna y wrap. |
| 8 | `codeloom-tui/src/permission_flow.rs` | En el diff de `edit`, los números de línea son relativos al fragmento `old`/`new`. | Números que no coinciden con el fichero. | Que el core adjunte el contexto del fichero (o que la TUI lo lea) para numerar con la posición real. |
| 9 | `codeloom-cli/src/main.rs` | `-p` (no interactivo) deniega todas las tools salvo con `--yolo`. | El modo print no puede usar tools sin `--yolo`. | Política de permisos configurable (`codeloom.toml`): lista de tools permitidas en modo print. |
| 10 | `batten/src/widgets/fx.rs` | Los efectos de tachyonfx interpolan en RGB aunque el tema tenga menos profundidad. | Colores RGB transitorios en terminales de 256/16 colores o `NO_COLOR`. | Desactivar los presets con `NoColor` y aplicar el efecto `ansi256` de tachyonfx con profundidades reducidas. |

Resueltas el 2026-10-16: 1, 2, 3 (ver 2026-10-16-transcript-virtual-design.md) , 4 (ver docs/plans/2026-10-16-color-depth.md) y 5 (ver docs/plans/2026-10-16-ansi-sanitize.md).

Pendientes de diseño que no son deuda sino trabajo planificado: rueda del ratón
(junto a selección con ratón), resaltado de sintaxis en bloques de código,
selección por celdas.
