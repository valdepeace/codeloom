# Cancelar un subagente según A2A (A10)

Estado: propuesta del 2026-10-23 (tarea A10 de `docs/plans/2026-10-22-codeloom-a2a-roadmap.md`).
Cierra las dudas 2, 3 y 4 de la sección «Dudas que siguen abiertas» de
`2026-10-15-loom-agent-profile-a2a.md`, **como decisiones nuestras, rotuladas como tales**: la
especificación de A2A no las resuelve.

## Propósito

Hoy se cancela a un hijo con la acción `interrupt` de `subagent_control`. Funciona, pero de forma
ad hoc: responde «interrupt requested» sin decir en qué estado quedó la tarea, y los fallos son
cadenas sueltas. A2A define `CancelTask` con un contrato más preciso. Se pretende que la
cancelación local se comporte como ese contrato, sin inventar nada donde el protocolo calla.

## Lo que consta en A2A (literal, ya recogido en la spec del perfil)

- «Requests the cancellation of an ongoing task. The server will attempt to cancel the task, but
  success is not guaranteed.» Salida: «Updated Task with cancellation status».
- Errores: `TaskNotCancelableError` («The task is not in a cancelable state (e.g., already
  completed, failed, or canceled).») y `TaskNotFoundError`.
- Una tarea terminal «cannot restart».
- Reanudar tras `input-required`: nuevo mensaje con el mismo `taskId` y `contextId`.

## Lo que NO consta (y por tanto decidimos)

1. A qué estado pasa la tarea al cancelarse («cancellation status»).
2. Si `input-required` o `auth-required` son cancelables.
3. Si cancelar una tarea ya cancelada es idempotente (§3.3.1) o error (§3.1.5 y §3.3.2).

## Lo que ya hace el código (sin cambios)

`interrupt` mata el trabajo (`JobRegistry::kill`), el estado `Killed` se proyecta a `Canceled`, y
`DiscardOnExit` descarta la pregunta pendiente de un hijo en `input-required`, que por tanto ya
es cancelable (test `interrupt_discards_the_pending_question`). Contestar con `answer` reanuda la
tarea; el `taskId` es la sesión del hijo y el `contextId` se deriva de la sesión del padre.

## Decisiones (nuestras)

- **D1. La operación se llama `cancel`** y es un método del lanzador:
  `SubagentLauncher::cancel(parent, child) -> Result<SubagentDescriptor>`. Devuelve la tarea
  actualizada, como el «Updated Task» de A2A. `control(.., "interrupt", ..)` sigue existiendo y
  delega en `cancel`, descartando el resultado.
- **D2. Estado resultante: `Canceled`**, la inferencia coherente con su definición. Como el éxito
  «no está garantizado», `cancel` espera como máximo `CANCEL_WAIT` (2 s) a que el hijo se asiente;
  si no lo hace, devuelve su estado actual (p. ej. `Working`) **sin afirmar que se canceló**.
- **D3. `input-required` y `auth-required` son cancelables.** Nada en el texto lo excluye y los
  ejemplos de `TaskNotCancelableError` son todos terminales. Es una lectura nuestra.
- **D4. Cancelar una tarea terminal es un error `TaskNotCancelable`**, también si ya estaba
  cancelada: se resuelve la contradicción a favor del error explícito de §3.1.5 y §3.3.2. En
  cambio, pedir otra cancelación mientras el hijo aún se detiene (el trabajo ya recibió la orden
  de matar y no se ha asentado) **es idempotente**: devuelve el estado actual sin matar de nuevo.
- **D5. Hijo desconocido o de otro padre: `TaskNotFound`.**
- **D6. Errores tipados.** `CoreError::TaskNotFound(i64)` y
  `CoreError::TaskNotCancelable { child: i64, state: TaskState }`. Los nombres imitan los de A2A;
  el texto va en inglés porque llega al modelo como resultado de herramienta.
- **D7. La herramienta informa del estado.** `subagent_control` con `interrupt` responde
  `Subagent N: canceled.` si se asentó, o `Subagent N: cancel requested; still stopping (state:
  working).` si no. Antes decía siempre «interrupt requested».

## Fuera de alcance, declarado

- El cable (`CancelTask` por JSON-RPC, gRPC o REST) y sus códigos de error: A4, A7 y A8.
- `UnsupportedOperationError` al enviar un mensaje a una tarea terminal (hoy `send_message` ya
  falla con «subagent has already finished»; no se reformula aquí).
- Un mando de la interfaz para cancelar un hijo concreto.
- Cancelar al padre ya cancela a sus hijos (comportamiento existente, sin cambios).

## Preguntas abiertas

- Si cancelar en `auth-required` debe conservar algún rastro de que no se concedió la
  autorización. El protocolo no lo dice; hoy ningún hijo llega a ese estado por sí solo.
