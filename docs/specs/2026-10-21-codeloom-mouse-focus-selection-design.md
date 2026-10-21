# Foco por clic y selección de texto acotada al composer

Estado: aprobada en conversación el 2026-10-21 (petición del usuario). Plan:
`docs/plans/2026-10-21-codeloom-mouse-focus-selection.md`.
Relacionado: `/mouse` (captura de ratón conmutable, commit `a257bca`).

## Intención

Hoy el ratón solo sirve para la rueda y el composer carece de selección: no se
puede llevar texto de un sitio a otro. Se pretende que el ratón sea un
ciudadano de primera: un clic dirige el foco a la ventana pulsada (conversación,
composer, Agent, Activity, panel lateral, rail…) y arrastrar dentro del composer
selecciona **solo** el texto del composer, sin arrastrar consigo el resto de la
pantalla, como ocurre con la selección nativa de la terminal.

## Objetivos

1. Clic en una ventana → el foco pasa a ella y se nota (indicador visual).
2. Clic en el composer coloca el cursor donde se pulsó; arrastrar selecciona.
3. Selección con teclado: Shift + flechas, Inicio y Fin.
4. Copiar: al soltar el ratón la selección se copia al portapapeles (OSC52, ya
   existente); Ctrl+C copia también si hay selección (si no la hay, conserva su
   comportamiento actual).
5. Escribir, borrar o pegar con selección la reemplaza o elimina.
6. Válido en los dos estilos de composer (Classic y Crush) y con texto
   multilínea, con ajuste de línea y con caracteres anchos.

## No objetivos

- Selección de texto en el transcript, el rail o los paneles (el transcript
  conserva su modo de selección por teclado). Queda para una fase posterior.
- Doble y triple clic (palabra y línea), arrastrar y soltar, desplazamiento
  automático al arrastrar fuera del composer.
- Funcionar con la captura de ratón apagada (`/mouse`): entonces manda la
  selección nativa de la terminal.

## Anti-objetivos

- Que el ratón o la selección introduzcan el menor tirón en el composer
  (prioridad máxima del proyecto): los eventos de arrastre son baratos y se
  agrupan con el resto de eventos del bucle.
- Que una selección sobreviva a un cambio de texto y apunte fuera de rango.
- Que Esc o Ctrl+C pierdan sus significados actuales cuando no hay selección.

## Decisiones de diseño (mías, marcadas para revisión)

- **Foco ampliado**: `Focus` pasa de `{Composer, Transcript}` a incluir los
  paneles laterales (`Sidebar`, `Agent`, `Activity`, `Rail`). Hoy solo el
  composer y el transcript hacen algo con él; los demás paneles reciben foco y
  se resaltan, y sirven de punto de enganche para interacciones futuras
  (inspección, desplazamiento). Con foco en un panel lateral: Esc devuelve el
  foco al composer y una tecla imprimible lo devuelve y escribe. Tab conserva su
  ciclo composer ↔ transcript.
- **Mapa de ventanas por fotograma**: `render` registra en la `App` los
  rectángulos de cada ventana visible (`pane_rects`) y el área del composer; el
  ratón se resuelve contra ese mapa. Es un `Vec` pequeño reescrito por frame.
- **Selección en el modelo del composer**: ancla (en grafemas, la misma unidad
  que el cursor) y cursor. Las operaciones de edición reemplazan la selección y
  los movimientos sin Shift la anulan.
- **Desplazamiento estable (Crush)**: hoy la fila superior visible se deduce del
  cursor, de modo que al arrastrar el contenido se movería bajo el ratón. Pasa a
  ser un estado persistido (mínimo desplazamiento necesario para mantener el
  cursor visible), lo que coincide con el comportamiento actual al escribir al
  final del texto.
- **Copiar al soltar**: seleccionar copia (como en las terminales), sin gesto
  adicional; Ctrl+C con selección también copia.
- **Indicador de foco**: en paneles con borde, el borde en color de acento; en
  los sin borde (look Crush), un marcador de acento en la columna de margen si
  existe. Si no hay columna libre, sin indicador (limitación anotada).

## Qué se deja al implementador

Geometría exacta del composer Classic (borde, prompt, recorte horizontal),
estilo de la selección (se usa `theme.styles.selection`) y nombres internos.

## Verificación

Pruebas puras del modelo de selección, del resaltado, de la traducción
posición → cursor en ambos estilos (incluidos ajuste de línea, texto ancho y
desplazamiento), del mapa de ventanas y del enrutado de eventos de ratón.
Validación visual en terminal real: imprescindible (es una entrega de
interacción) y pendiente.
