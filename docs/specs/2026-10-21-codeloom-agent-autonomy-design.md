# Autonomía entre agentes: contexto, tareas enlazadas y artefactos

Estado: **propuesta**, pendiente de aprobación. No implementar todavía.
Relacionado: `2026-10-15-loom-agent-profile-a2a.md` (§5, §8 y hallazgos de la
lectura de la especificación oficial), `2026-10-21-codeloom-subagent-input-required-design.md`,
`2026-10-13-loom-design.md` (§6, subagentes y jobs).

## Intención

Hoy el padre lanza un subagente, este trabaja en un job y, al terminar, deja un
aviso de texto que el padre lee **en su siguiente turno**. Funciona, pero el
trabajo entre agentes es, en el fondo, un recado con acuse de recibo manual.
Se pretende que los agentes colaboren con más autonomía (el padre reacciona
solo, encadena tareas y se pasa resultados con nombre) **tomando prestados los
conceptos de A2A** en lugar de inventar los propios, de modo que el día que
exista el transporte remoto sea una traducción y no una reescritura.

Cabe señalar que A2A no resuelve la autonomía por nosotros: define el
vocabulario y el ciclo de vida de una tarea, no cuándo debe un agente actuar
por iniciativa propia ni cuánto se le permite gastar. Esa parte es nuestra.

## Qué se toma de A2A (y qué no)

| Concepto A2A | Uso en CodeLoom |
|---|---|
| `contextId` | Hilo de una delegación; agrupa las tareas de una misma conversación. |
| `taskId` | Ya existe: el id del job del subagente. |
| Tarea terminal inmutable | Un subagente terminado no se reabre; lo que siga es una tarea nueva. |
| `referenceTaskIds` | Una tarea nueva puede apuntar a las anteriores de las que parte. |
| `Artifact` (id, nombre, partes) | Resultado con nombre y reutilizable, en lugar de un texto pegado en un aviso. |
| Notificación ante `input-required` o estado terminal | El padre ocioso **reacciona**, no espera a que el usuario escriba. |
| Agent Card, transporte, autenticación | **Fuera**: es el futuro `codeloom-a2a`. |

No se adopta `Message` frente a `Task` (el agente híbrido) ni `ListTasks` con
paginación: no hace falta mientras todo sea local.

## Objetivos

1. **Reacción**: cuando un hijo llega a `input-required` o a un estado
   terminal y el padre está ocioso, la aplicación arranca un turno del padre.
2. **Presupuesto de autonomía**: esa reacción automática tiene un tope por
   cada mensaje del usuario, para que no haya bucles de gasto.
3. **Artefactos**: un hijo publica resultados con nombre; el padre los recibe
   como lista (nombre, tamaño, descripción) y lee solo los que necesita.
4. **Tareas enlazadas**: un subagente nuevo puede declararse continuación de
   otro ya terminado y recibe sus artefactos como punto de partida.
5. **Contexto**: todas las tareas de una delegación comparten un identificador
   de contexto visible en eventos y en el rail.

## No objetivos

- Subagentes que lancen subagentes (la profundidad sigue siendo 1).
- Comunicación entre hermanos o entre procesos; transporte, tarjeta o red.
- Memoria compartida entre agentes: el agente es opaco para los demás, como
  exige A2A.
- Editar desde la TUI los artefactos.

## Anti-objetivos

- Que la autonomía gaste tokens sin que el usuario lo note: todo turno
  automático debe verse como tal y poder detenerse con Esc.
- Que un artefacto grande engorde el contexto del padre: el aviso lleva
  metadatos, nunca el contenido completo.
- Que se rompa la fluidez del composer por el trabajo en segundo plano.
- Cambiar el esquema SQLite si puede evitarse.

## Propuesta de diseño

### Fase A — Reacción y presupuesto (implementada, 2026-10-21)

- Un evento del hijo (`SubagentInputRequested`, o `SubagentSettled` con
  `Completed` o `Failed`) hace que la TUI, si el padre no tiene turno activo,
  pida un despertar. El trabajador ejecuta `AgentLoop::run_woken`, que entrega
  los avisos pendientes y **solo si había alguno** ejecuta un turno, **sin
  fabricar mensaje de usuario**; sin avisos publica `WakeIdle` y no gasta nada.
- **Presupuesto**: `MAX_AUTO_TURNS = 8` por mensaje del usuario (decisión mía,
  a confirmar); se gasta al empezar el turno despertado y se rearma al enviar
  un mensaje. Agotado, la TUI muestra «auto-turns exhausted — write to
  continue» y aguarda.
- El turno despertado se señala con el estado `auto-turn` en el pie; Esc lo
  cancela como cualquier otro y descarta el despertar pendiente. Un hijo
  interrumpido por el usuario (`Killed`) no despierta al padre.
- No se despierta con turno en curso, mensaje en cola, diálogo de permisos o
  preguntas abierto, selector o ajustes abiertos, ni mientras se carga una
  sesión. Un prompt en cola tiene prioridad sobre el despertar.
- Para evitar carreras, el lanzador publica `SubagentSettled` **después** de
  persistir el estado final del hijo, y `SubagentInputRequested` solo tras
  registrar la pregunta en el buzón (`request_with`).

Limitaciones conocidas: durante un turno despertado herdr sigue viendo `Idle`
hasta el final (no se emite `Working`); el turno automático no se marca en el
transcript, solo en el pie; validación en terminal real pendiente.

### Fase B — Artefactos

- Tipo puro en el core: `Artifact { id, name, description, parts }` con
  `Part::{Text(String), Data(serde_json::Value)}` (sin ficheros binarios por
  ahora).
- Herramienta `artifact_publish` para el hijo (y `artifact_read` para el
  padre). Se persisten **dentro del historial de la sesión del hijo**, como ya
  se hizo con `todo_write`: sin tabla nueva, restaurables.
- El aviso de finalización lista los artefactos (nombre, tamaño) en lugar de
  volcar el contenido; `artifact_read(child_id, artifact_id)` trae el texto.

### Fase C — Tareas enlazadas y contexto

- `subagent` gana `follow_up_of: child_id` opcional. El hijo nuevo recibe, en
  su prompt, la lista de artefactos de la tarea referida y puede leerlos.
- La acción `send_message` de `subagent_control` se restringe a lo que A2A
  permite: responder a una tarea en `input-required`. Enviar texto a un hijo
  en marcha sin pregunta pendiente deja de ser válido (hoy es ambiguo: se
  añade al historial del hijo pero no hay garantía de cuándo lo leerá).
- `ContextId` = identificador de la sesión raíz; viaja en los eventos de
  subagente y job. Es opaco y solo sirve para agrupar y mostrar.

### Fase D — Más adelante

Profundidad mayor que 1, pares remotos y transporte A2A (crate opcional).

## Decisiones que son mías (marcadas para revisión)

- El presupuesto por defecto (8 turnos automáticos) y que se cuente por
  mensaje del usuario.
- Persistir artefactos en el historial del hijo, y no en una tabla.
- Restringir `send_message` a responder preguntas.
- `ContextId` = sesión raíz, y no un identificador independiente.

## Preguntas abiertas

- ¿El turno automático debe pedir confirmación la primera vez, o basta con
  avisar y poder cancelar?
- ¿Cuántos artefactos y de qué tamaño máximo por tarea? (propuesta: 16 y 64 KiB
  cada uno, igual que el límite de salida de un job).
- ¿Un padre ocioso con varios hijos que terminan a la vez recibe un único
  turno con todos los avisos o uno por hijo? (propuesta: uno solo, agrupado).
- El comportamiento real de `send_message` con un hijo en marcha: está
  **sin verificar** cómo el bucle del hijo relee su historial; antes de
  restringirlo hay que leerlo.

## Verificación

Tests de presupuesto (se detiene al agotarse y se rearma con el siguiente
mensaje del usuario), de que no se arranca turno con diálogos abiertos, de
artefactos (publicar, leer, límites, restauración desde el historial), de
tareas enlazadas y de la restricción de `send_message`. Validación visual en
terminal real: pendiente.
