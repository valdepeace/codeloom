# Selección por caracteres en el transcript y navegación por bloques

Estado: aprobada en conversación el 2026-10-21 (petición del usuario: «poder
movernos por bloque igual que hace Crush» y «arrastrar con el ratón sobre el
texto no selecciona»). Plan: `docs/plans/2026-10-21-codeloom-transcript-selection-blocks.md`.
Relacionado: `2026-10-21-codeloom-mouse-focus-selection-design.md` (selección del
composer; allí el transcript quedó fuera de alcance).

## Intención

Con la captura de ratón activa la terminal ya no selecciona por su cuenta, y el
arrastre solo funcionaba en el composer: sobre la conversación no ocurría nada.
Se pretende poder seleccionar y copiar **caracteres** de la conversación con el
ratón, y poder moverse y copiar **bloque a bloque** (un bloque es una entrada
del transcript: un mensaje, una llamada a herramienta…), viendo siempre cuál es
el bloque activo.

## Objetivos

1. Arrastrar con el botón izquierdo sobre el transcript selecciona texto por
   caracteres (varias filas, con ajuste de línea y caracteres anchos), con
   resaltado; al soltar se copia (como en el composer) con aviso `Copied N chars`.
2. Arrastrar fuera del área (arriba o abajo) desplaza el transcript para poder
   seleccionar más de lo visible.
3. Con el foco en el transcript existe un **bloque activo**, visible, y se
   navega entre bloques con `[` (anterior) y `]` (siguiente); la vista se
   desplaza para mostrarlo entero (o su comienzo si no cabe).
4. `y`/`c` copian el bloque activo (si no hay selección de líneas con `v`).
5. Un clic sobre el transcript le da el foco y activa el bloque pulsado.
6. Ctrl+C con selección de texto copia y no sale; Esc la limpia (sin contar para
   la doble Esc).

## No objetivos

- Doble/triple clic, arrastrar y soltar, plegar o desplegar bloques, vista de
  índice de bloques (encaja con la observabilidad O2, fase posterior).
- Conservar la unión de filas partidas por ajuste de línea: el texto copiado
  separa con `\n` cada fila visual (limitación anotada).
- Selección en rail, paneles o el chrome.

## Anti-objetivos

- Que el arrastre cueste tirones: el resaltado es un parcheo de celdas del
  búfer ya pintado, sin re-render del transcript; el texto solo se calcula al
  copiar.
- Que una selección apunte fuera de rango o sobreviva a un cambio de ancho.
- Cambiar el significado de teclas existentes: `j/k`, PgUp/PgDn, `u/d`, `g/G`, `v`
  mantienen su comportamiento.

## Decisiones de diseño (mías, marcadas para revisión)

- **Coordenadas de documento** `(fila, columna)` sobre el diseño de filas del
  `Layout` (independientes del scroll). Ancla y cabeza; se normaliza al leer.
- **Resaltado por parcheo del búfer**: tras pintar el transcript se aplica
  `theme.styles.selection` a las celdas dentro del rango visible.
- **Extracción por re-render fuera de pantalla** de las líneas implicadas a un
  `Buffer` del ancho del texto, leyendo celdas y recortando espacios finales: el
  texto copiado es exactamente lo que se ve (incluidos prefijos como `❯ `).
- **Se limpia** la selección de texto al cambiar el ancho del diseño, al cargar
  sesión y si queda fuera de rango.
- **`[` / `]` para bloques** y no `j/k`, para no cambiar lo que ya hace `j/k`
  (por líneas). Si el usuario prefiere `j/k` por bloques, es un cambio de una
  tabla de teclas.
- **Marcador del bloque activo**: con margen libre (chrome `Flat`) un `▎` con
  `theme.styles.accent` en la columna de margen de cada fila del bloque; sin
  margen, se recolorea con `accent` la primera celda de cada fila.
- **Autodesplazamiento**: un evento de arrastre sobre la fila superior/inferior
  del área o fuera de ella desplaza 1 fila por evento hacia ese lado.

## Qué se deja al implementador

Nombres internos, forma exacta de la lectura del búfer fuera de pantalla y
colocación de las pruebas.

## Verificación

Pruebas puras de: normalización y rango de la selección, traducción celda →
`(fila, columna)` con scroll, resaltado en `TestBackend`, extracción de texto
(multifila, ancho, prefijos), navegación de bloques, enrutado del ratón y del
teclado. Validación visual en terminal real: imprescindible y pendiente.

## Estado (2026-10-21)

Implementada y commiteada, con 329 pruebas en `batten` y 236 en `codeloom-tui`,
`clippy -D warnings` y `rustfmt` limpios:

- `TextSelection`/`CellPos` en coordenadas de documento; `hit_cell`,
  `paint_text_selection` (parcheo del búfer, sin re-render) y `selected_text`
  (dibujado fuera de pantalla línea a línea, solo al copiar).
- Enrutado del ratón: clic enfoca el transcript y activa el bloque pulsado;
  arrastrar selecciona, con autodesplazamiento de una fila por evento en los
  bordes; al soltar se copia con `Copied N chars` y la selección permanece;
  clic simple la descarta; Ctrl+C copia sin salir; Esc la limpia sin contar
  para la doble Esc.
- Bloque activo: `[` y `]` navegan, `y`/`c` copian el bloque; la vista se
  desplaza para mostrarlo.

Decisiones tomadas al implementar:

- El extremo de la selección es **inclusivo** (la celda donde se suelta entra),
  como en las terminales.
- Un carácter ancho solo se copia si empieza dentro del rango.
- El marcador del bloque activo es `█` de acento en el margen (con chrome
  `Flat`), porque el `▎` ya lo usa el filete de foco del transcript y sería
  indistinguible; sin margen se recolorea la primera celda de cada fila.
- La selección se anula al cambiar el ancho del diseño, al cargar sesión y al
  editar el composer; las teclas de navegación no la anulan.

Limitaciones conocidas:

- Las filas partidas por ajuste de línea se copian separadas por `\n`.
- Si el transcript pasa de caber a desbordar durante un stream, la scrollbar
  estrecha el diseño y la selección se descarta en ese instante.
- Ctrl+C copia la selección aunque haya un diálogo abierto.
- Sin prueba del bloque más alto que la vista; la prueba de la rama sin margen
  es débil.
- Validación visual en terminal real: pendiente.
