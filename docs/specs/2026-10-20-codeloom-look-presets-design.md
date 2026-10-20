# Looks de TUI: presets que cambian la presentación, empezando por Crush

Estado: borrador pendiente de aprobación. Fecha: 2026-10-20.
Revisión previa: `docs/plans/2026-10-20-codeloom-conversation-composer-review.md`.

## Intención

Mejorar la conversación de CodeLoom (transcript y composer) tomando Crush como
referencia de estructura, con estilo propio. Además, convertir los presets de
tema (Crush, Martty, Mindfork, Pawan, MastraCode y, más adelante, Codex) de
simples paletas en **looks** completos: paleta más una variante de
presentación por elemento. Así cambiar de preset cambia también la forma de
los elementos, y cada referencia visual se puede incorporar sin tocar la
lógica.

Motivo: hoy `render.rs` elige a mano qué función de dibujo usa cada elemento
y el resultado es una mezcla sin identidad (composer propio con caja,
tarjeta de tool de Crush, sugerencias de Martty). Además, las tools no
aparecen en orden dentro de la conversación y el composer no gestiona bien
el texto largo.

## Objetivos

1. Un tipo `Look` dentro de `Theme` que selecciona la variante de cada
   elemento. Los widgets despachan según `theme.look`; `render.rs` deja de
   elegir funciones concretas.
2. Un look **Crush** completo para transcript y composer (ver §Look Crush).
3. Un look **Classic** que conserva exactamente la presentación actual y lo
   usan todos los presets que aún no tienen look propio.
4. Composer con wrap, scroll vertical y cursor siempre visible, sin tocar su
   contrato de teclas.
5. Cada widget informa de su altura; desaparece el `+ 2` fijo de
   `draw_transcript`.

## No objetivos

- Looks propios de Martty, Mindfork, Pawan, MastraCode o Codex. Se migran
  después, uno por uno, sobre la misma costura. Codex necesita antes una
  referencia (paleta y demo) que hoy no existe.
- Jobs y subagentes como actividad estructurada en el transcript. Es el
  siguiente paso y reutilizará el estilo de línea de las tools.
- Mezclar paleta y look por separado. Un preset es paleta y look a la vez.
- Looks definibles desde TOML. `Theme::from_toml` usa `Look::classic()`.
- Cambiar el modelo de eventos del core, el inspector (Ctrl+T) o la cola.

## Anti-objetivos

- Que el composer pierda fluidez, parpadee o se cuelgue al teclear o pegar
  (prioridad máxima del proyecto). Cambiar de look no puede tocar el estado
  del composer ni el bucle de entrada.
- Que `codeloom-core` dependa de `ratatui` o `crossterm`.
- Que copiar una selección del transcript incluya marcadores de presentación
  (cabeceras de rol, glifos de tool, bordes).
- Que cambie el comportamiento de los presets sin look propio.

## Restricciones

- Reglas de `AGENTS.md`: build, test, fmt y clippy (`-D warnings`) por commit.
  El plan del 19-20 de octubre documenta siete avisos previos de clippy en
  Batten; no se añaden nuevos y la línea base se comprueba antes de empezar.
- Sin red en tests. Pruebas con `TestBackend` para estados y tamaños; no
  cuentan como validación visual.
- Validación visual en terminal real obligatoria antes de dar por cerrado el
  composer y el transcript (normal, vacío/cargando, error, terminal estrecha).
- El árbol de trabajo tiene cambios sin confirmar en estos mismos ficheros
  (`batten/src/widgets/{composer,transcript,tool_item,theme}.rs`,
  `codeloom-tui/src/{app,render}.rs`). El plan parte del estado actual del
  árbol, no de `HEAD`.
- Entorno de compilación: ver las notas del proyecto sobre el enlazador MSVC
  y el directorio `target` (usar uno local, el compartido llena `E:`).

## Estado actual relevante (verificado en el código)

- `ThemePreset` (`theme.rs`) solo cambia la paleta; `Theme` contiene
  `styles`, `glyphs`, `extensions`, `depth`, `hyperlinks`.
- Existen funciones por referencia: `composer::{render_crush, render_martty,
  render_mindfork}`, `tool_item::{render_crush, render_pawan, render_martty,
  render_mindfork}`, `thinking::{render_crush, render_martty,
  render_mindfork}`, entre otras.
- `draw_transcript` usa `composer::render_focused_with_model` (caja
  redondeada, prompt `❯`/`│`, scroll horizontal por `horizontal_view`, sin
  wrap), `tool_item::render_crush` para **solo** `app.tools.back()` y reserva
  alturas a mano (`composer.lines() + 2`, filas de tool y de thinking).
- `composer::render_crush` envuelve con `Paragraph::wrap` pero coloca el cursor
  con `cursor_row_col()` lógico: si una línea se envuelve, el cursor queda mal.
  No tiene scroll vertical.
- `Transcript` (`state/transcript.rs`) es una lista de `Entry` (`kind`,
  `format`, `source`, líneas derivadas, `rev`) con `push_entry`, `push` y
  `append`. No hay forma de reemplazar el contenido de una entrada existente.
- `App.tools` es una `VecDeque<ToolItem>` limitada a 8 y alimenta el
  inspector. `ToolItem` guarda estado, resumen, salida y `expanded`.
- La fila superior de `draw_transcript` ("CodeLoom · project conversation ·
  tema") duplica el título del panel.

## Diseño

### Look

`Look` es una estructura `Copy` con un enum por elemento. En esta entrega:

- `ComposerStyle { Classic, Crush }`
- `ToolStyle { Pinned, Inline }` (`Pinned` = tarjeta fija junto al composer,
  como hoy; `Inline` = entradas dentro del transcript).

`Look::classic()` y `Look::crush()` fijan cada variante. `Theme` gana el
campo `look: Look`; `Theme::preset(p)` lo rellena (`Crush` → `Look::crush()`,
resto → `Look::classic()`); `default_dark`, `test` y `from_toml` usan Classic.
Se añaden variantes a los enums solo cuando un look nuevo las necesite.

Los widgets que cambian de aspecto ofrecen una función de entrada por
elemento (`composer::render`, `composer::height`, etc.) que despacha con
`match theme.look.<elemento>`. Las funciones `render_<referencia>` actuales
se conservan; el despacho las llama.

### Look Crush: composer

- Sin marco. Primera línea con prompt `> `, continuaciones con `::: `, tal
  como `composer::render_crush`. Placeholder en el primer renglón cuando está
  vacío.
- Altura = filas **visuales** del texto (tras el wrap) acotada a 3–15 y a
  lo que quepa en el panel.
- Wrap propio, no `Paragraph::wrap`: se calcula una vez por frame la lista de
  filas visuales (por grafemas y ancho Unicode) y se usa tanto para pintar
  como para el cursor, de modo que ambos coinciden siempre. El ancho útil
  descuenta el prompt (2 o 4 columnas).
- Scroll vertical que mantiene visible la fila del cursor cuando el texto
  excede la altura.
- El contrato de teclas no cambia: Enter envía, Ctrl+J inserta salto, el resto
  como hoy. El estado `batten::state::Composer` no cambia; el cálculo del
  layout es una función pura que recibe texto, cursor y ancho.
- El coste por frame debe ser lineal en la longitud del texto y sin
  asignaciones por carácter; pegar texto grande no puede degradar la entrada.
- Etiqueta de modelo (decisión mía): sin fila extra; se añade al footer
  global tras el estado, p. ej. ` working · <modelo>`.

### Look Crush: transcript

- **Tools inline y en orden.** Cada llamada es una entrada del transcript
  creada en `ToolStarted`/al restaurar sesión, en el punto exacto de la
  conversación:
  - cabecera `● <Tool>  <resumen>` con el glifo coloreado por estado
    (pendiente, ejecución, permiso, éxito, error, cancelada) usando los
    `StyleToken::Tool*` existentes;
  - salida debajo con borde izquierdo, plegada a 2 líneas más
    `… (N lines hidden) [click or space to expand]` y completa si
    `expanded`;
  - error visible en el estilo de error.
- **Nuevo en `Transcript`:** una operación para reemplazar las líneas de una
  entrada existente (actualiza `rev` y reindexa), usada al cambiar el estado,
  resumen, salida o plegado de una tool. `App` mantiene el mapa
  `call_id → EntryId`. Mientras la tool está en `app.tools` (límite 8) la
  entrada se re-deriva desde su `ToolItem`; al salir de esa ventana la
  entrada queda con su último contenido.
- El inspector y `app.tools` no cambian. En `ToolStyle::Inline` desaparece la
  tarjeta fija y su reserva de filas; en `Pinned` todo sigue igual.
- Ctrl+O sigue alternando el plegado de la última tool. Alternar el de una
  tool concreta con clic en su cabecera es deseable; si complica la
  geometría de selección, queda fuera de esta entrega.
- **Spinner (decisión mía):** el glifo de la cabecera es estático (color
  según estado). La animación se queda en la fila de thinking y el footer.
  Re-derivar una entrada en cada tick rompería la virtualización y el
  planificador de frames.
- **Cabeceras de rol "You"/"Assistant" (decisión mía, a confirmar en el
  plan):** marcador no seleccionable; si no se puede añadir sin afectar a
  selección, copia y geometría, se descarta y se deja una distinción por
  color/prefijo en el gutter. Criterio de aceptación: copiar nunca incluye
  marcadores.
- **Fila "CodeLoom · project conversation · tema"** se elimina solo con el
  look Crush. El nombre del tema activo debe seguir visible tras `/theme`
  (p. ej. en el feedback del footer); el plan decide dónde.

### Alturas

`draw_transcript` calcula el espacio con `composer::height(theme, composer,
ancho, máximo)` y, en `Pinned`, con la altura de la tarjeta. No quedan
constantes de marco en `render.rs`.

## Qué se deja al implementador

- Nombres exactos de funciones y módulos nuevos.
- Cómo se modela el mapa `call_id → EntryId` y la restauración de sesión
  (mensajes `ToolCall`/`ToolResult` ya guardados) con el mismo camino que los
  eventos en vivo.
- Estructura interna de las filas visuales del composer.
- Dónde mostrar el tema activo tras `/theme`.

## Verificación

- Pruebas de estado: reemplazo de entrada (índices, `rev`, selección vigente),
  ciclo de vida de tool a entrada, restauración de sesión en orden.
- Pruebas del layout del composer: texto vacío, una línea, varias líneas,
  línea más larga que el ancho, grafemas anchos/combinados, ancho mínimo,
  cursor en cada posición y en los bordes de línea envuelta, altura 3–15.
- Pruebas de render (`TestBackend`) en Crush y en Classic, en varios
  tamaños; Classic debe producir la misma salida que antes.
- build, test, fmt, clippy sin avisos nuevos.
- Inspección en terminal real: composer vacío, multilínea, texto largo,
  pegado grande, terminal estrecha; tools en curso, éxito, error, plegadas y
  expandidas dentro de una conversación larga; `/theme` rotando todos los
  presets.

## Preguntas abiertas

- Codex: qué referencia tomar y dónde obtenerla.
- Clic en cabecera de tool para plegar cualquiera (ver arriba).
- Persistencia del preset elegido entre sesiones (hoy no se ha verificado
  que `/theme` persista).

## Estado (2026-10-20)

Plan: `docs/plans/2026-10-20-codeloom-look-presets.md`. Implementado en el
árbol de trabajo, sin commitear (el árbol contiene además trabajo previo del
usuario en los mismos ficheros): `Look` en `Theme`, layout puro del composer,
composer Crush con wrap/scroll/altura, `Transcript::push_lines`/`replace`,
líneas de tool y su integración en `App`/`render.rs`. Tests: 246 (`batten`) y
101 (`codeloom-tui`); clippy `-D warnings` y rustfmt limpios en lo tocado.

Correcciones a lo anterior, descubiertas al leer el código:

- Las cabeceras "You"/"Assistant" ya existían en `widgets/transcript.rs`
  (`render_line`) como decoración no seleccionable. No hizo falta trabajo.
- El modelo ya aparece en la cabecera global (`draw_workspace_header`), así
  que el composer Crush no lo repite y el footer no cambia.
- `Theme::for_profile` (`widgets/profile.rs`) propaga ahora `look`, para que
  el perfil de terminal no devuelva Crush a Classic.

Decisiones de implementación:

- La entrada de cada tool se crea siempre; con el look Classic nace vacía (0
  líneas) y no corta el texto del asistente. Así, cambiar a Crush a mitad de
  conversación deja las tools ya ejecutadas en su sitio cronológico. En
  Classic el texto posterior sigue añadiéndose a la entrada anterior del
  asistente, de modo que la tool puede aparecer ligeramente después de ese
  texto al cambiar de look.
- Las entradas de tool que salen de la ventana de 8 del inspector conservan en
  Inline su último contenido; es deliberado, para no perder historial visible.
- Validación visual en terminal real: **pendiente**.
