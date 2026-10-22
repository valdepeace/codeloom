# El subagente como nodo de un árbol en la conversación

Estado: implementada el 2026-10-22 (véase «Estado» al final). Continúa `2026-10-22-codeloom-a2a-delegation-choice-design.md`
(adenda «el transcript es el diario de toda llamada») y se apoya en la regla de trazabilidad A2A
(`a2a-visual-traceability`).

## Intención

Hoy un subagente se ve como líneas sueltas en la conversación del padre: su cabecera, y las
llamadas `job_output` con las que el padre lo sondea. No se ve qué hace por dentro. La persona
usuaria pide organizarlo **como un árbol de elementos**, igual que hacen otros editores: el
subagente es un **nodo** que, desplegado, enseña su propia conversación y sus herramientas, y
del que cuelgan las acciones del padre sobre él. El estado del nodo se ve de un vistazo por
**iconos de estado**, que la persona usuaria declaró «muy importantes».

## Qué dice A2A

La relación padre-hijo, el estado de una tarea y su resultado sí constan (A5, A6, A13). **Cómo se
dibuja un árbol de tareas no consta**: es presentación local y se rotula como tal. Lo que se
muestra del estado es el estado A2A **exacto** (`submitted`, `working`, `input-required`,
`auth-required`, `completed`, `failed`, `canceled`, `rejected`); un icono nunca lo sustituye ni lo
agrupa sin nombrarlo.

## Decisiones

### De la persona usuaria (2026-10-22)

- **El subagente es un nodo; su conversación y sus herramientas cuelgan de él.**
- **Plegado por defecto**, también mientras trabaja. La persona usuaria lo despliega a voluntad.
- **Los iconos de estado son una prioridad.**

### Lecturas nuestras (rotuladas como tales)

- **Iconos: los ocho glifos A2A que ya usa el rail** (`○ ● ? ⚿ ✓ ✗ ⊘ ⊖`, de
  `AgentState::glyph`), distintos entre sí y de una sola columna, con el color del estado, para
  que se distinga sin color. Sustituyen en la cabecera del nodo al glifo agrupado de la
  herramienta. El nombre A2A exacto sigue entre paréntesis tras la etiqueta, como hasta ahora.
- **Cabecera plegada, una línea:** `▸ <icono> [agente] etiqueta (estado) · paso 2/30 · herramienta`.
  Desplegado, `▾` en lugar de `▸` y debajo, con sangría y guías (`├─`, `└─`), el contenido.
- **Contenido del nodo desplegado:** los mensajes del hijo (texto del asistente) y una línea por
  cada llamada a herramienta suya, con su propio icono de estado (`ToolStatus`) y su resultado
  resumido. **No** el cuerpo completo de cada herramienta: para eso se sigue entrando en la
  sesión del hijo desde el rail (Enter), que no se sustituye.
- **Las acciones del padre sobre el hijo cuelgan de su nodo**: `job_output`/`job_kill` sobre el
  job del hijo y `subagent_control` sobre su identificador **dejan de ser líneas sueltas del
  padre** y aparecen, desplegado el nodo, como `↳ padre: job_output`. Plegado, el nodo muestra
  cuántas lleva (`· 3 consultas`). Son las llamadas cuyo `id` coincide con el `job_id` del hijo,
  o cuyo `child_id` es el suyo; se resuelve al pintar, sin tocar el modelo de eventos.
- **De dónde salen los datos del hijo.** Hoy al padre solo llegan eventos de progreso del hijo
  (`TurnStarted`, `StepStarted`, `ToolStarted`, `ToolFinished`, `Usage`, `FileDiff`, `TurnEnded`,
  `Error`); **no** sus mensajes ni los argumentos de sus herramientas. Se reenvía también el
  `MessageAdded` del hijo por el mismo canal etiquetado (`ChildEvent`), y la interfaz guarda por
  hijo un registro **acotado** (últimos 200 mensajes, textos de herramienta recortados). No hay
  streaming de deltas del hijo: se ven mensajes completos.
- **Despliegue:** con el foco en el transcript y el bloque activo sobre el nodo, `→` o Enter lo
  despliega y `←` lo pliega. El estado plegado/desplegado vive en memoria de la sesión de la
  interfaz; no se persiste.
- **Profundidad:** solo hijos directos. Un nieto aparece en el registro de su padre-hijo como una
  llamada `subagent` más, sin anidarse un nivel más. (`max_depth` aún no se aplica.)

## Qué se construye

1. **Core:** `MessageAdded` pasa a ser relevante para el reenvío de un hijo
   (`LiveEvent::is_child_relevant`), con prueba de que llega etiquetado.
2. **TUI, datos:** un registro por hijo (`ChildLog`) alimentado por los `ChildEvent`, acotado.
3. **TUI, nodo:** la entrada de actividad del subagente se dibuja como nodo plegable (cabecera de
   una línea con icono A2A, y cuerpo con sangría y guías cuando está desplegado).
4. **TUI, llamadas del padre:** las llamadas cuyo objetivo es un hijo no salen como líneas
   sueltas, sino bajo su nodo.
5. **TUI, teclas:** `→`/Enter despliega y `←` pliega con el bloque activo sobre un nodo.

## No objetivos

- Streaming de texto del hijo token a token.
- El cuerpo completo de las herramientas del hijo dentro del árbol (sigue en su sesión).
- Árbol genérico para jobs que no sean subagentes.
- Persistir qué nodos estaban desplegados.
- Más de un nivel de anidación.

## Anti-objetivos

- **Perjudicar la fluidez del composer**: sin repintados por token, sin trabajo proporcional a
  todo el historial del hijo en cada fotograma (se cachean las líneas del nodo y se invalidan
  solo cuando cambia su registro).
- Que un icono oculte el estado A2A real o lo agrupe sin nombrarlo.
- Memoria sin cota por un hijo muy verboso.

## Qué se deja al implementador

El dibujo exacto de las guías y de la sangría, el texto de los resúmenes de herramienta, cómo se
cachean las líneas, y los nombres de los tests. Si el glifo `⚿` no se ve bien en la fuente de la
persona usuaria (punto ya abierto), el icono de `auth-required` puede cambiarse sin tocar el resto.

## Verificación

- Core: un `MessageAdded` del hijo llega al padre como `ChildEvent` con la identidad del hijo.
- TUI: un hijo plegado ocupa **una línea** con su icono A2A y su estado; para cada uno de los
  ocho estados, icono distinto y nombre exacto entre paréntesis.
- Desplegado, muestra su texto y una línea por herramienta; plegar lo devuelve a una línea.
- Un `job_output` sobre el job del hijo no genera línea suelta en el padre y se cuenta en el nodo.
- El registro del hijo respeta su cota.
- Medición declarada: el coste de repintar con un nodo desplegado y un hijo muy largo no crece con
  el historial (se mide, no se supone).

## Preguntas abiertas

- ¿Debe el nodo desplegarse solo cuando el hijo pasa a `input-required` (porque necesita a la
  persona usuaria)? Aquí no: sigue plegado y solo cambia su icono.
- Comando o atajo para desplegar/plegar todos a la vez.

## Estado

Implementada el 2026-10-22, **sin verla aún en una terminal real** y con la medición de fluidez
del composer con un nodo desplegado pendiente. Commits: `a059b49` (el `MessageAdded` del hijo llega
al padre), `1bf6ec6` (líneas de nodo en batten), `3d4ee31` (registro acotado por hijo), `7cdce9b`
(nodo plegable con icono A2A), `865539e` (→/Enter despliega, ← pliega) y `193e141` (las consultas
del padre cuelgan del nodo).

Lecturas propias que conviene recordar: las filas de acción se rotulan `↳ parent: {herramienta}`
(en inglés, como el resto de etiquetas del nodo) y el resumen plegado dice `n query` o `n queries`;
una llamada que llega antes que su hijo se reclasifica cuando este aparece; el aviso de fin del
subagente en el look clásico sigue siendo una línea de sistema sin etiqueta propia.
