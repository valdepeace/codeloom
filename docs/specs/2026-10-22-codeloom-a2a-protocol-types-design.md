# Los tipos de A2A en el núcleo: `Message`, `Part` y `Artifact`

Estado: propuesta del 2026-10-22 (tarea A5 de `docs/plans/2026-10-22-codeloom-a2a-roadmap.md`).
Plan: `docs/plans/2026-10-22-codeloom-a2a-protocol-types.md`. Continúa
`2026-10-22-codeloom-a2a-task-model-design.md`, que dejó tres divergencias temporales
declaradas: el mensaje de estado como texto, el resultado como texto y la ausencia de
`contextId`. Fuente normativa: `a2a.proto` (`lf.a2a.v1`), mensajes `Task`, `TaskStatus`,
`Message`, `Part`, `Artifact` y enumerado `Role`.

## Intención

Premisa del usuario: no inventar nada y acoplarse al estándar. Hoy el estado de un
subagente ya es `TaskState`, pero su mensaje de estado y su resultado son cadenas. Cada
cadena es una decisión nuestra que habría que deshacer al abrir A2A al exterior. Se
adoptan los tipos del proto tal cual en `codeloom-core` y se retiran las cadenas.

## Qué dice el proto (literal)

- `Message { message_id (obligatorio), context_id, task_id, role (obligatorio), parts (obligatorio), metadata, extensions, reference_task_ids }`.
- `Role { ROLE_UNSPECIFIED, ROLE_USER, ROLE_AGENT }`; como con `TaskState`, se omite `UNSPECIFIED`.
- `Part { oneof content { text, raw (bytes), url, data (Value) }, metadata, filename, media_type }`.
- `Artifact { artifact_id (obligatorio), name, description, parts (obligatorio), metadata, extensions }`.
- `TaskStatus { state, message: Message, timestamp }`.
- `Task { id, context_id, status, artifacts, history, metadata }`.

## Decisiones

### Respaldadas por el protocolo

| Decisión | Respaldo |
|---|---|
| `TaskStatus.message` pasa de `Option<String>` a `Option<Message>` | proto `TaskStatus.message` |
| El resultado de un subagente es una lista de `Artifact`; al completarse lleva uno con una `Part` de texto | proto `Task.artifacts`; la salida de una tarea es un artefacto |
| Los campos de `Message`, `Part` y `Artifact` son los del proto, con los mismos nombres en snake_case | proto |
| `Part` conserva el `oneof` como enumerado de cuatro variantes | proto `Part.content` |

### Lecturas o simplificaciones nuestras (rotuladas)

- **`context_id` de una tarea de subagente = id de la sesión del padre.** A2A dice que el
  servidor genera los ids y que `contextId` agrupa tareas relacionadas; no fija cómo se
  elige. Se deriva en lectura (`SubagentDescriptor::context_id`), sin columna nueva.
- **El `Message` de estado lo emite el agente:** `role = ROLE_AGENT`, una `Part` de texto,
  `message_id` generado por el runtime. El proto no dice qué rol corresponde a un mensaje
  de estado; `ROLE_AGENT` es la lectura natural («from the server to the client»).
- **Persistencia:** el mensaje de estado y los artefactos se guardan como JSON de
  nuestros tipos (`serde`). Es un formato interno de almacenamiento, **no** el del cable:
  los nombres JSON del cable son la tarea A4.
- **Lectura de lo antiguo:** una `status_message` que no sea JSON válido se lee como
  mensaje de agente con una `Part` de texto; una fila con `result` y sin artefactos se
  lee como un artefacto de texto.
- **El aviso al padre sigue siendo texto**, derivado de las `Part`s de texto del artefacto
  (presentación del cliente hacia su propio modelo, como ya se declaró).

## No objetivos

- `Task.history`, `Task.metadata` y envío de `Message` entre agentes (A7, A9).
- `reference_task_ids` en uso: el campo existe porque lo trae el proto, pero nada lo escribe
  hasta A11.
- Nombres del cable, transporte, tarjeta del agente (A4, A6, A8).
- `JobResult` de los trabajos (`bash` y similares): no son tareas A2A.

## Anti-objetivos

- Tipos «inspirados en» A2A con campos distintos. Un campo del proto se llama igual o no
  se incluye (y se dice).
- Mostrar el artefacto en la interfaz con otra forma que el texto que ya se mostraba:
  la presentación no cambia en esta tarea.

## Qué se deja al implementador

Cómo se generan los `message_id` y `artifact_id` (reutilizar el generador de ids del
núcleo; si no hay uno, contador atómico más `now_ms`), y la ubicación exacta de los
ayudantes de lectura de texto.

## Verificación

Pruebas puras: ida y vuelta serde de cada variante de `Part`; construcción y lectura de
texto de `Message` y `Artifact`; persistencia de estado con `Message` por SQLite;
lectura de filas antiguas (texto plano y `result`); migración idempotente; que el aviso
al padre conserve el texto exacto de hoy; que la interfaz muestre lo mismo que antes.
