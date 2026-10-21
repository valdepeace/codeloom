# `ask_user` en subagentes como `input-required` (A2A)

Estado: aprobada en conversación el 2026-10-21 (entrega por tareas). Plan:
`docs/plans/2026-10-21-codeloom-subagent-input-required.md`.
Relacionado: `2026-10-15-loom-agent-profile-a2a.md` (§5, §8 y hallazgos de la
lectura de la especificación oficial),
`2026-10-20-codeloom-todos-and-questions-design.md`.

## Intención

Hoy un subagente no puede preguntar nada: `InProcessSubagentLauncher::new`
le quita `ask_user` para que no bloquee la interfaz sin que se sepa quién
pregunta. Cabe señalar que ese veto es un parche. En A2A, cuando un agente
necesita algo de quien le encargó la tarea, la tarea pasa a `input-required`
y **responde el cliente**, que puede contestar por sí mismo o escalar al
humano. Se adopta ese modelo con las piezas locales, sin inventar protocolo.

## Flujo

1. El hijo invoca `ask_user`. Su herramienta queda bloqueada (no el proceso)
   y la tarea queda en `input-required` (proyección de `TaskState`).
2. La pregunta **no llega a la interfaz**: se registra en un buzón del
   padre y se anuncia a su LLM con un aviso en el siguiente vaciado de
   avisos (`[Subagent X needs input]` + preguntas + cómo responder).
3. El LLM del padre decide (el protocolo no prescribe este paso para
   `input-required`: es inferencia nuestra): contesta con `subagent_control` (acción
   `answer`) o, si no lo sabe, usa **su propio** `ask_user` con el usuario.
4. La respuesta despierta al hijo, que sigue (`working`) con las respuestas
   como resultado de su `ask_user`, en el mismo formato de siempre.

Solo el agente raíz habla con el usuario. Los hijos no tienen un camino
directo a la pantalla.

## Objetivos

- Un hijo puede preguntar y reanudarse sin perder su turno.
- `interrupt` y la cancelación del padre desbloquean al hijo (la pregunta
  pendiente se descarta y su `ask_user` devuelve el error habitual de «sin
  respuesta»).
- La interfaz muestra que un hijo espera respuesta.
- Los permisos de un hijo siguen yendo a la interfaz como hasta ahora.

## No objetivos

- Transporte A2A real, Agent Card, subagentes remotos.
- Despertar al padre si está ocioso (ver pregunta abierta).
- Persistir preguntas pendientes entre ejecuciones de la aplicación.
- Responder desde la TUI a la pregunta de un hijo.

## Anti-objetivos

- Que un hijo bloquee la interfaz o robe el diálogo de preguntas del padre.
- Que el estado «esperando» se guarde en SQLite (es transitorio).
- Cambiar `ToolCtx` o el esquema de sesiones.

## Decisiones de diseño

- **Buzón en el core** (`InputBroker`, módulo `subagent_input`): mapa
  hijo → petición pendiente con un `oneshot`. Puro, sin red ni UI.
- **Sin tocar `ToolCtx`**: cada hijo recibe su propio `PermissionService`
  cuyo responder (`ChildResponder`) delega los permisos en el servicio del
  padre y encamina `ask` al buzón. Así `AskUserTool` funciona tal cual.
- **Respuesta estructurada**: `SubagentLauncher::answer(parent, child,
  answers)` y, en la herramienta, el parámetro `answers` (una cadena por
  pregunta, en orden). Se descarta reutilizar `message` por ser ambiguo.
- **Estado transitorio**: `SubagentDescriptor` no cambia; la TUI conoce los
  hijos en espera por eventos nuevos y la proyección A2A la da una función
  pura `subagent_task_state(status, awaiting_input)`.
- **Aviso al padre** por el mismo canal que los avisos de finalización
  (`flush_subagent_notices`), una sola vez por petición.

## Preguntas abiertas

- **Padre ocioso** (decidido en principio, plan aparte). Los avisos solo se
  leen al comenzar un turno del padre, de modo que una pregunta de un hijo
  queda dormida mientras el padre no actúe. La especificación A2A prevé que el
  cliente reaccione a `input-required` y a los estados terminales mediante
  suscripción o *push*; lo coherente es que un evento del hijo arranque un
  turno del padre cuando este esté ocioso. Esta entrega solo marca la espera
  en la TUI; el despertar automático se planifica después, una vez leído el
  bucle de ejecución de `codeloom-tui`.
- **Tiempo máximo de espera** de un hijo: no lo hay; `interrupt` basta por
  ahora.

## Verificación

Pruebas del buzón (petición, respuesta, cancelación, descarte), del
responder del hijo (permisos delegados, preguntas encaminadas), del
lanzador con un hijo que pregunta y se reanuda, de la acción `answer`, de
los avisos y de la TUI (marca de espera). Validación visual en terminal
real: pendiente.
