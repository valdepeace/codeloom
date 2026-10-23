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

## Decisiones (de la persona usuaria, 2026-10-23, y propias)

1. **Abajo**, en el pie de la app (de la persona usuaria).
2. **Widget en batten** con **los dos estilos**, para poder compararlos (de la persona usuaria):
   `Segments` (celdas `━`, aspecto mastra) y `Blocks` (`█░` con `ctx N%`, aspecto Pawan).
   *Interpretación propia de «puesto los dos en batten»; se corrige si era otra cosa.*
3. **En el showcase de batten** (único modo demo que existe hoy; la app no tiene uno propio).
4. La app usa `Segments`.

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
