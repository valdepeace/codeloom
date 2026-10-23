# Transporte `InProcess` de A2A detrás de `AgentTransport` (A7)

Estado: implementada el 2026-10-23 (véase «Estado» al final; tarea A7 de
`docs/plans/2026-10-22-codeloom-a2a-roadmap.md`).

## Propósito

Hoy se delega con `SubagentLauncher`, que es la cara local de una operación que A2A define de forma
neutra: enviar un mensaje, consultar una tarea, listarlas, cancelarla. La spec del perfil (§5 y §6)
prevé un contrato único, `AgentTransport`, con dos implementaciones: `InProcess` (en memoria, sin
socket, la que usan las pruebas) y `A2aRemote` (A8). Esta tarea define el contrato y la
implementación en memoria. No toca el cable ni abre nada.

## Lo que consta

- «Dos transportes, mismo contrato detrás de `AgentTransport`» y «`InProcess`: mensajes en memoria,
  sin socket» (spec del perfil, §5); el núcleo define los *traits puros* y no depende de A2A ni de
  red (§6).
- Operaciones de A2A: `SendMessage`, `GetTask`, `ListTasks`, `CancelTask`, `SubscribeToTask`
  (esta última es de A9). Enviar un mensaje con el `taskId` de una tarea en `input-required` la
  reanuda; sin `taskId` crea una tarea nueva; `referenceTaskIds` enlaza tareas (A11).
- El orden de los eventos rige «regardless of protocol binding» (§3.1 de A2A): el transporte en
  memoria debe cumplirlo también (lectura nuestra, ya anotada en el roadmap).

## Lo que NO consta (y por tanto decidimos)

- Los nombres de los métodos en Rust y la forma de las peticiones: son nuestros.
- Cómo se traduce el texto de un mensaje de reanudación a las respuestas de `answer`: el broker
  espera una respuesta por pregunta; A2A solo habla de `parts`. **Decisión nuestra:** cada `Part`
  de texto es una respuesta, en orden.
- Qué error corresponde a enviar un mensaje a una tarea que no espera entrada: A2A tiene
  `UnsupportedOperationError`, cuyo código no se contrastó. Se usa por ahora un error de
  herramienta; el mapeo al cable es de A8.

## Decisiones (nuestras salvo indicación)

- **D1. `AgentTransport` es un trait del núcleo** (`codeloom-core/src/transport.rs`), sin tipos del
  cable: usa `Message`, `SubagentDescriptor` (que hace de `Task`, porque el núcleo no tiene tipo
  `Task`), `AgentCard` y `SessionId`. Operaciones: `send_message`, `get_task`, `list_tasks`,
  `cancel_task`, `agent_card`.
- **D2. `SendRequest`** lleva `parent`, `agent: Option<String>`, `label`, `message: Message`,
  `task_id: Option<SessionId>` (reanudar) y `reference_task_ids: Vec<SessionId>` (enlazar, con la
  semántica de A11: una sola, terminal). Devuelve el descriptor de la tarea tras aceptar el envío.
- **D3. `InProcessTransport`** envuelve un `Arc<dyn SubagentLauncher>` y el registro de agentes.
  No duplica lógica: traduce a `start`, `start_follow_up`, `answer`, `list` y `cancel`.
  `SubagentTool` **no** se migra en esta tarea; sigue usando el lanzador.
- **D4. Reanudar** exige que la tarea esté en `input-required`; en cualquier otro estado es error
  y no se entrega nada.
- **D5. `agent_card(None)`** devuelve la tarjeta del agente `default`; un nombre inexistente es
  error.
- **D6. Una suite de contrato** (`transport_contract`), escrita como función genérica sobre
  `impl AgentTransport`, fija el comportamiento común. A7 la ejecuta contra `InProcess`; A8 la
  reutilizará contra el remoto con cassettes. Es lo que hace real «mismo contrato».

## Fuera de alcance, declarado

- `SubscribeToTask` y los eventos de flujo (A9).
- El cable, la autenticación y los errores con código (A8).
- Migrar `SubagentTool` y `subagent_control` al transporte.
- Un tipo `Task` propio en el núcleo.

## Preguntas abiertas

- Si `SubagentTool` debe acabar hablando con `AgentTransport` en lugar de con el lanzador, o si el
  transporte queda solo como fachada para lo remoto y los tests.
- Si el broker debe aceptar menos respuestas que preguntas pendientes.

## Estado

Implementada el 2026-10-23, **sin verla en una terminal real**: probada con proveedores simulados.
Commits: `f87c66b` (esta spec y su plan), `3101452` (contrato y transporte en memoria) y `1106c3c`
(suite de contrato).

### Lo hecho

- `crates/codeloom-core/src/transport.rs`: `SendRequest`, el trait `AgentTransport`
  (`send_message`, `get_task`, `list_tasks`, `cancel_task`, `agent_card`) y `InProcessTransport`
  sobre el lanzador y el registro. Sin tipos del cable ni red.
- Un mensaje sin `task_id` crea una tarea; con una referencia, un seguimiento (A11); con `task_id`,
  reanuda una tarea en `input-required` y cada `Part` de texto es una respuesta, en orden.
- Característica `test-support` de `codeloom-core`, que expone `transport::contract` a otros crates.

### Cómo se comprobó

Once pruebas del transporte, entre ellas las de rechazo (tarea que no espera entrada, número de
respuestas que no cuadra, partes que no son de texto, reanudar y referenciar a la vez, dos
referencias) y la suite de contrato ejecutada contra `InProcess`. A mano se quitó la comprobación de
`input-required` y la prueba correspondiente falló.

### Lo que no consta o queda pendiente

- **Decisión añadida al implementar:** el transporte en memoria rechaza cualquier `Part` que no sea
  de texto, también al crear una tarea nueva (no solo al reanudar).
- **Sin `SubscribeToTask`** (A9) y sin que `SubagentTool` use el transporte; sigue con el lanzador.
- **La suite de contrato es corta a propósito:** solo lo independiente del transporte. El ciclo
  completo de una tarea depende de cada agente y se prueba en las pruebas de cada transporte.
- **Un solo `task_id` por tarea y una sola referencia**, como en A11.
- **El error de «no espera entrada»** es un error de herramienta; su código de cable
  (`UnsupportedOperationError`, −32004) lo asigna A8.
