# Adaptador de nombres del cable de A2A (A4)

Estado: propuesta del 2026-10-23 (tarea A4 de `docs/plans/2026-10-22-codeloom-a2a-roadmap.md`).

## Propósito

El núcleo guarda los tipos de A2A con nombres internos (`snake_case`, enumerados de Rust). El
cable de A2A v1.0 usa otros: JSON en `camelCase`, enumerados como `TASK_STATE_WORKING` y una
`Part` cuyo contenido se identifica por el nombre de la clave. Esta tarea fija, en un crate del
borde, la traducción de ida y vuelta entre ambos, de modo que A7 (transporte en memoria) y A8
(HTTP) la reutilicen sin que el núcleo conozca el cable. No abre ningún socket.

## Lo que consta en A2A v1.0 (a2a-protocol.org/latest/specification, leído el 2026-10-23)

Se distingue lo leído literalmente de lo leído en un resumen de la página, que no pudo
contrastarse palabra por palabra.

- **Literal:** los ejemplos de la sección 6 usan `"role": "ROLE_USER"`, `"role": "ROLE_AGENT"` y
  `"state": "TASK_STATE_WORKING"`; `StreamResponse` lleva exactamente uno de `task`, `message`,
  `statusUpdate`, `artifactUpdate`; una respuesta con `includeArtifacts` falso omite `artifacts`
  «entirely», «not as an empty array or null»; con `historyLength` 0 se omite `history`; en los
  ejemplos de `raw` y `url` figuran `filename` y `mediaType`.
- **Resumen:** nombres JSON en `camelCase` (§5.5); enumerados como cadenas de ProtoJSON; `Part`
  con «exactly one of `text`, `raw`, `url`, `data`» más `metadata`, `filename`, `mediaType`; sin
  clave `kind`; campos de `Message` (`messageId`, `contextId`, `taskId`, `role`, `parts`,
  `metadata`, `extensions`, `referenceTaskIds`), de `Artifact` (`artifactId`, `name`,
  `description`, `parts`, `metadata`, `extensions`), de `TaskStatus` (`state`, `message`,
  `timestamp`) y de `Task` (`id`, `contextId`, `status`, `artifacts`, `history`, `metadata`);
  marcas de tiempo ISO 8601 en UTC `YYYY-MM-DDTHH:mm:ss.sssZ` (§5.6.1); «Implementations SHOULD
  ignore unrecognized fields».

## Lo que NO consta (y por tanto decidimos o aplazamos)

- **Cómo se codifican los bytes de `raw`.** El texto leído no lo dice; el ejemplo parece base64.
  Decisión nuestra: base64 estándar (la convención de ProtoJSON). No se leyó `a2a.proto`.
- **Si la v1.0 aún acepta los nombres de la v0.3** (`working`, `user`): no consta. Se aplaza a A8,
  junto con `A2A-Version`.
- **Una regla general de omisión** para `Task` y `Message`: solo consta en los casos citados y
  en la canonicalización de la tarjeta. Los ejemplos omiten lo no puesto. Decisión nuestra: se
  omiten los opcionales ausentes y las listas vacías.
- **Los nombres de los métodos JSON-RPC** (`SendMessage` frente a `message/send`): el resumen
  se contradice. No es de esta tarea.

## Decisiones (nuestras salvo indicación)

- **D1. Un crate nuevo, `codeloom-a2a`**, opcional y fuera de `codeloom-cli`: depende de
  `codeloom-core`, `serde`, `serde_json` y `base64`. El roadmap ya lo prevé para A8; A4 lo crea
  con un único módulo, `wire`. El núcleo no cambia.
- **D2. Solo la v1.0.** La v0.3 queda para A8.
- **D3. Tipos de cable propios** (`WireRole`, `WirePart`, `WireMessage`, `WireArtifact`,
  `WireTaskStatus`, `WireTask`), serializables con `camelCase`, y conversiones con los del
  núcleo: `From` hacia el cable y `TryFrom` desde él, con un error `WireError` (`thiserror`).
- **D4. Enumerados**: `TaskState` ⇄ `TASK_STATE_*` (reutiliza `proto_name`/`from_proto_name`),
  `Role` ⇄ `ROLE_USER` / `ROLE_AGENT`. `*_UNSPECIFIED` y cualquier cadena desconocida son
  error al entrar.
- **D5. `Part`**: se serializa con una sola de las claves `text`, `raw` (base64), `url`, `data`,
  más `filename`, `mediaType` y `metadata` si existen. Al entrar, cero o varias claves de
  contenido son error.
- **D6. Marca de tiempo**: `timestamp_ms` ⇄ `YYYY-MM-DDTHH:mm:ss.sssZ`, calculada a mano (sin
  dependencia de fechas). Una marca anterior a 1970 no se produce nunca; al entrar se rechaza.
- **D7. `Task` desde un `SubagentDescriptor`**: `id` es la sesión del hijo en texto,
  `contextId` es `descriptor.context_id()`, `status` y `artifacts` los suyos; `history` y
  `metadata` se omiten porque el núcleo no los modela. Solo hay conversión de ida.
- **D8. Tolerancia al entrar**: los campos desconocidos se ignoran (SHOULD de §5.7); los
  obligatorios que falten son error.

## Fuera de alcance, declarado

- Transporte, métodos JSON-RPC, rutas HTTP, cabecera `A2A-Version`, códigos de error del cable
  (A7, A8).
- Nombres de la v0.3.
- Eventos de flujo (`statusUpdate`, `artifactUpdate`): A9.
- La tarjeta del agente en el cable.

## Preguntas abiertas

- La codificación real de `raw` y los nombres exactos del proto, a contrastar con `a2a.proto`.
- Si el mensaje de estado debe llevar `taskId` y `contextId` al salir (hoy el núcleo los deja
  vacíos).
