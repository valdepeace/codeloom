# A8c: streaming SSE, al servir y al consumir

Estado: la persona usuaria ordenó abordarla el 2026-10-24 («si a8c y después a12b»). Las decisiones
marcadas como nuestras quedan rotuladas para su revisión. Continúa
`2026-10-23-codeloom-a2a-remote-transport-design.md` (fase A8c), `…-task-events-design.md` (A9, de donde
sale el flujo) y `…-serve-design.md` (A8b, que hoy responde −32004 a los métodos de flujo).

## Premisa

Cabe señalar que el flujo ya existe dentro de CodeLoom: A9 deriva `StreamResponse` del bus
(`flujo_de_tarea`) y `wire.rs` ya lo traduce a los nombres del cable. Lo que falta es solo el
**transporte**: emitirlo como Server-Sent Events al servir, y leerlo al consumir en lugar de sondear.
No se abre un canal paralelo ni se inventan eventos.

## Lo que consta (especificación A2A v1.0, binding JSON-RPC, §3.1.2, §3.1.6, §3.3.4, §9.4.2, §9.4.6)

- Los métodos son `SendStreamingMessage` (misma petición que `SendMessage`) y `SubscribeToTask`
  (`params.id`). La respuesta es HTTP 200 con `Content-Type: text/event-stream`; cada evento es una
  línea `data:` con una respuesta JSON-RPC completa (`jsonrpc`, `id`, `result`) cuyo `result` es un
  `StreamResponse` (exactamente uno de `task`, `message`, `statusUpdate`, `artifactUpdate`).
- Con una tarea, el flujo **empieza por el objeto `Task`** y se cierra al llegar a un estado
  terminal. `SubscribeToTask` también empieza por el `Task` actual, para no perder nada entre un
  `GetTask` y la suscripción.
- `SubscribeToTask` sobre una tarea terminal devuelve `UnsupportedOperationError` (−32004); sobre una
  inexistente o ajena, `TaskNotFoundError` (−32001).
- Si la tarjeta no declara `capabilities.streaming = true`, los dos métodos **deben** devolver
  `UnsupportedOperationError`, y el cliente **debería** comprobar la tarjeta antes de usarlos.

## Lo que no consta (y por tanto es nuestro)

- Cómo reanudar un flujo cortado: la spec lo delega en cada binding y no define `Last-Event-ID`. No se
  inventa: ante un corte, el cliente vuelve al sondeo.
- Comentarios de latido, tamaños máximos, número de flujos abiertos y duración: todo cota nuestra.

## Decisiones (nuestras salvo indicación)

- **D1. Servir:** `SendStreamingMessage` y `SubscribeToTask` por SSE, y la tarjeta servida declara
  `streaming: true`. El resto de métodos no soportados siguen igual (−32004).
- **D2. El flujo es `flujo_de_tarea` (A9).** El anfitrión de A8b gana `subscribe(id)`; no hay otro canal.
  Se suscribe al bus **antes** de leer la tarea, como ya hace `InProcessTransport`.
- **D3. `SendStreamingMessage` no bloquea:** crea la tarea y emite el flujo desde el `Task` inicial;
  `max_wait_secs` no interviene. Un mensaje con `taskId` sigue siendo −32004 (D7 de A8b).
- **D4. Cotas al servir (decálogo):** como mucho 16 flujos abiertos a la vez (el siguiente recibe el
  mismo «servidor ocupado» de A8b); el flujo se cierra solo al terminal, y la tarea no pasa de
  `max_task_secs`, así que un flujo no vive más; el texto de cada `Task`/`artifactUpdate` se acota a
  `MAX_RESULT_BYTES` como en `GetTask`; latido (comentario SSE) cada 15 s para que ningún proxy cierre
  un flujo largo por inactividad. Al soltar el cliente la conexión se libera el hueco; la tarea
  **no** se cancela (la spec no lo exige).
- **D5. Aislamiento:** `SubscribeToTask` de una tarea de otra sesión es −32001 igual que si no existiera.
- **D6. Consumir:** si la tarjeta del remoto declara `streaming: true`, el espejo usa `SubscribeToTask`
  tras el `SendMessage` (que ya vuelve sin esperar) en lugar de sondear con `GetTask`. Se prefiere
  `SubscribeToTask` a `SendStreamingMessage` porque reutiliza el envío ya verificado y el flujo empieza
  por el `Task` actual, sin pérdida. Si el remoto no declara streaming, o la suscripción falla, o el
  flujo se corta antes de un estado terminal, **se vuelve al sondeo**: el seguimiento nunca se pierde.
- **D7. Cotas al consumir:** línea SSE de 64 KiB como máximo, evento de 256 KiB, y el seguimiento
  total no pasa de lo que ya acota `remote_max_follow`. Un evento mal formado o de otra tarea corta el
  flujo y manda al sondeo; nunca se ejecuta ni se obedece su contenido (es dato de un tercero).
- **D8. Los eventos del remoto no inventan estados:** `statusUpdate` y `artifactUpdate` se aplican con
  los mismos tipos y la misma traducción que el sondeo (`RemoteTask`), rotulando lo que es de A2A.

## Fuera de alcance, declarado

- Notificaciones push (webhooks) y gRPC.
- Reanudación de flujos (no consta en la spec).
- `SendStreamingMessage` desde el cliente (D6).
- Mostrar el progreso parcial de un remoto token a token: el flujo mantiene la tarea al día, la TUI
  sigue viviendo de `SubagentChanged`.

## Preguntas abiertas (resueltas con la opción recomendada, a revisar)

1. ¿`SubscribeToTask` en lugar de `SendStreamingMessage` al consumir (D6)? Recomendado: sí.
2. ¿Cancelar la tarea cuando el cliente suelta el flujo? Recomendado: no (la spec no lo pide).
3. ¿Tope de 16 flujos (D4)? Sin medir; es una apuesta.

## Estado

Implementada el 2026-10-24 (commits `b95f3bb` servidor y `e749a83` cliente).

- **Verificado:** pruebas unitarias y de integración con HTTP real (nuestro cliente contra nuestro
  servidor) y, a mano, el servidor frente al SDK oficial de Python (`a2a-sdk`) en modo
  `streaming=True`: cuatro eventos (tarea, `WORKING`, artefacto, `COMPLETED`).
- **Hallazgo corregido:** con estado terminal, los artefactos se emiten antes y el estado cierra la
  secuencia (lo contrario dejaba al cliente sin los artefactos finales).
- **No consta:** el seguimiento por flujo contra un remoto real de terceros que lo ofrezca (solo se
  ha probado con el SDK y con dobles), ni en una terminal real. El sondeo sigue de reserva ante
  cualquier fallo del flujo.
