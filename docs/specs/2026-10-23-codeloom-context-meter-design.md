# Medidor de contexto en la barra inferior

Estado: diseño, 2026-10-23.

## Propósito

Ver cuánto se ha llenado la ventana del modelo y **cuándo va a saltar la compactación**
(`2026-10-23-codeloom-context-summary-design.md`). Es observabilidad: hoy el aviso de «contexto
compactado» llega después del hecho y nada anticipa el momento.

## Qué dicen las referencias y qué es propio

Contrastado el 2026-10-23 (solo lectura; véase `docs/references/contexto-y-coste/`):

- **mastra code:** barra de diez celdas `━`, pero **no mide la ventana del modelo**: mide el
  relleno de su memoria observacional frente a sus umbrales. Su `/context` avisa de que sus
  porcentajes son «del total auditado, no de la ventana del modelo».
- **qwen-code:** porcentaje `tokens del prompt / ventana` en el pie, en rojo por encima del 100 %.
- **codex:** porcentaje de ventana **restante**, descontando una base fija.
- **Pawan** (ya reproducido en batten, `status_bar::render_pawan`): `ctx N%` y barra `█░`.
- opencode, crush y claude-code-rust: no consultados para esto.

De mastra se toma **solo el aspecto** (celdas `━`); lo que se mide es lo de qwen-code y codex:
la ventana real. El marcador del 80 % y los colores son **decisiones propias**.

## Lo que más gusta de mastra: la memoria observacional

La persona usuaria pidió expresamente **no olvidar** la memoria observacional de mastra y su forma
de representarla. Está estudiada en
`docs/references/contexto-y-coste/memoria-observacional-mastra.md` (qué es, qué se leyó y qué
no, y las ideas que podría tomar CodeLoom). Lo que afecta a este medidor:

- Su barra es de **tres clases de celda**: memoria (atenuada), mensajes (normal) y libres
  (apagadas), repartidas en proporción a lo que pesa cada parte, con al menos una celda de cada
  clase si hay de ambas.
- Su equivalente propio y barato: tras compactar, la petición es «resumen + cola». El resumen
  hace de **memoria**; la cola, de **mensajes**.

## Decisiones (de la persona usuaria, 2026-10-23, y propias)

1. **Abajo**, en el pie de la app (de la persona usuaria).
2. **Widget en batten** con **los dos estilos**, para poder compararlos (de la persona usuaria):
   `Segments` (celdas `━`, aspecto mastra) y `Blocks` (`█░` con `ctx N%`, aspecto Pawan).
   *Interpretación propia de «puesto los dos en batten»; se corrige si era otra cosa.*
3. **En el showcase de batten** (único modo demo que existe hoy; la app no tiene uno propio).
4. La app usa `Segments`.
5. **Dos segmentos, resumen y mensajes** (inspirado en mastra; tarea 1b): `ContextMeter` lleva un
   campo `memory` (tokens del resumen vigente, parte de `used`). Con `memory > 0`, `Segments`
   pinta tres clases de celda y `Blocks` usa `▒` para la memoria; el texto añade
   `· resumen {n}`. **`memory` es una estimación** (bytes/4 del texto del resumen) mientras que
   `used` lo cuenta el proveedor: el reparto de celdas es aproximado y así se rotula.

## Diseño

### Qué se mide

`used` = tokens de entrada que el proveedor contó en la **última** petición (`input_tokens` de
`LiveEvent::Usage` cuando es mayor que cero): es el tamaño real del contexto enviado, la misma
ancla que usa la compactación. `window` = `Catalog::context_window(base_url, model)`.

- Sin ventana conocida **no se pinta barra**: solo `ctx {used} (sin ventana)`. Sin dato no se
  afirma nada.
- Sin ningún uso todavía, no se pinta nada.
- Al compactar, la barra baja sola con la siguiente respuesta.
- Al recargar una sesión, `used` sale del último mensaje del asistente que lleve `usage`.

### Aspecto

- `Segments`: diez celdas `━`; las ocupadas con el color del nivel, las libres atenuadas; detrás
  `{used}/{window}  {pct} %`. Ejemplo: `━━━━━━━━───  142k/200k  71 %`.
- `Blocks`: `ctx {pct}% ██████░░░░`, como Pawan.
- **Niveles** (propios): por debajo del 80 % neutro; desde el 80 % (donde salta la compactación)
  aviso; desde el 100 % error.
- El 80 % es `COMPACT_RATIO` de `codeloom_core::agent::compact`; batten no depende de core, así
  que el widget recibe `mark_percent` como dato (por defecto 80).

### Reglas de espacio

Con menos de 80 columnas de pie no se pinta (primero ceden los atajos). El widget calcula su
propio ancho para que el pie reparta el espacio.

## Fuera de alcance, declarado

- Desglose de qué llena la ventana (el `/context` de mastra): spec aparte.
- Modo demo de la **app** (con datos de mentira): no existe; el showcase de batten es el demo de
  los widgets.
- El apunte de la llamada de resumen publica su propio `Usage` y puede mover un instante la barra
  al tamaño de esa petición hasta la siguiente respuesta.

## Estado

Implementado el 2026-10-23, **sin verlo aún en una terminal real**: solo hay pruebas con
`TestBackend`, de modo que el aspecto, los colores y la ausencia de parpadeo del pie están sin
comprobar a ojo. Commits: `36f5162` (widget con dos estilos), `81d04b8` (memoria y mensajes),
`7d9e38e` (demo del showcase) y `b8d0120` (pie de la app).

### Cómo verlo

- Demo: `cargo run -p batten --example showcase` y elegir «Context meter».
- App: `cargo run -p codeloom-cli --bin loom`; el medidor aparece en el pie con 80 columnas o más
  en cuanto llega la primera respuesta del proveedor.

### Decisiones tomadas durante la implementación

- **El nivel de aviso usa aritmética entera exacta**, no el porcentaje redondeado: 79,6 % se
  muestra como «80 %» pero no es aviso hasta alcanzarlo, igual que la compactación.
- **La memoria se pinta con el estilo de acento del tema**, no atenuada como en mastra: el estilo
  atenuado era casi idéntico al de las celdas libres y no se distinguían.
- **La ventana se cachea** (comparando `Arc` del catálogo, URL base y modelo) para que el pie no
  recorra el catálogo en cada fotograma.
- **El uso de un subagente no mueve el medidor del padre** (llega por `ChildEvent`).

### Lo que no consta o queda pendiente

- **El `Usage` de la propia llamada de resumen** mueve la barra al tamaño de esa petición hasta
  la siguiente respuesta normal.
- **Tras compactar**, la barra mantiene el uso anterior hasta que llega la siguiente respuesta; la
  memoria nueva se acota a ese uso.
- **Con menos de dos celdas ocupadas** todas son de mensajes (difiere de mastra en ese caso).
- **Solo se mide el padre**; el gasto de contexto de un hijo que se esté viendo se atribuye a la
  sesión mostrada.
- La velocidad de la animación del demo es a ojo.
- Quedan 5 lints de `clippy --examples` preexistentes en otros demos del showcase, ajenos a esto.
