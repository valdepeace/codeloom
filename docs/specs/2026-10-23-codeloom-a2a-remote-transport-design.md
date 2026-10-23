# Transporte `A2aRemote` de A2A: consumir y servir agentes por HTTP (A8)

Estado: propuesta del 2026-10-23 con las decisiones de la persona usuaria incorporadas (tarea A8 de
`docs/plans/2026-10-22-codeloom-a2a-roadmap.md`). No implementar todavía. Depende de A4, A7 y A9.

## Propósito

Dos usos, ambos **desactivados por defecto**:

- **Consumir:** registrar un agente ya desplegado (por su tarjeta) y delegarle tareas como a un
  subagente más, visible en el árbol y cancelable.
- **Servir:** exponer un agente local a otros clientes de A2A.

El caso común (un proceso, sin red) no cambia: sin endpoints ni rol, todo es `InProcess`.

## Lo que consta

- Roles `serve | consume | both` en `codeloom.toml`, por defecto apagados; crate opcional
  `codeloom-a2a`; el núcleo no conoce A2A (spec del perfil, §5 y §6).
- Tarjeta en `/.well-known/agent-card.json` (§8.1–8.2). Un perfil con `transport: a2a-remote` y
  `a2a.endpoint` es un **puntero** a un remoto.
- `Execution::Remote` ya existe en el registro; el lanzador lo rechaza hoy con «agente remoto no
  disponible». Una fuente remota entra con `Approval::Pending`.
- **Resumen** de la spec v1.0: cabecera `A2A-Version` en cada petición (vacía = `0.3`); errores
  `VersionNotSupported`; HTTPS en producción; autorización por operación; push y tarjeta
  extendida con error si no se declaran.
- Seguridad de la tarjeta (`security_schemes`, `security_requirements`, `signatures`, `icon_url`)
  no la modela A6: queda para esta tarea.

## Lo que ya consta y lo que sigue abierto

La lectura literal de la spec v1.0 que recoge `2026-10-15-loom-agent-profile-a2a.md` (sección
«Lectura de la especificación: descubrimiento, bindings y seguridad») cierra varias dudas que esta
spec dejó abiertas en su primer borrador:

- **Métodos de JSON-RPC:** PascalCase, «matching gRPC conventions (e.g., `SendMessage`,
  `GetTask`)» (§9.1). El `message/send` del resumen era de la v0.3.
- **Códigos de error:** `TaskNotFoundError` −32001, `TaskNotCancelableError` −32002,
  `UnsupportedOperationError` −32004, `VersionNotSupportedError` −32009, etc. (§5.4, filas
  literales); rango −32001 a −32099 (§9.5). Los detalles usan `google.rpc.ErrorInfo` (SHOULD en
  JSON-RPC, MUST en REST y gRPC).
- **Seguridad (§13):** autorización en cada operación y antes de consultar datos; no revelar la
  existencia de recursos ajenos; tarjeta extendida solo con autenticación; HTTPS en producción.
- **`A2A-Version`:** MUST en cada petición; vacía equivale a `0.3`; sin degradación automática.

Sigue abierto o sin contrastar:

- **Cuerpo de error de REST:** los ejemplos de §6 muestran `application/problem+json`, mientras
  §11.6 exige `ErrorInfo`. Lectura nuestra: prevalece la norma (MUST). Solo importa si se sirve REST.
- **Nombres de los eventos de flujo y sus campos** (A9, tarea 0).
- **Codificación de `raw`:** en ProtoJSON los `bytes` van en base64; es la regla general del
  formato, **no** una frase de la spec leída. La copia local del proto no se ha localizado.
- **Cómo se autentica un cliente ante un remoto concreto:** lo dice su tarjeta, dato de un tercero.

## Decisiones (nuestras salvo indicación)

- **D1. Fases independientes**, cada una con su plan y su commit: **A8a consumir** (cliente,
  tarjeta, fuente remota, espejo en el lanzador), **A8b servir** (servidor JSON-RPC sin SSE),
  **A8c SSE** (necesita A9), **A8d seguridad** (esquemas de autenticación, HTTPS, firmas).
  Servir **no se activa** sin A8d.
- **D2. Una tarjeta remota es dato, no instrucción.** Sus textos se muestran como datos, se acotan
  (decálogo: tamaño máximo de la tarjeta y de cada campo) y no se registra ningún remoto sin que la
  persona lo apruebe. No hay descubrimiento automático.
- **D3. Un remoto se refleja como un subagente local:** un trabajo en el `JobRegistry` que envía
  el mensaje, sigue la tarea (SSE si hay A9, si no sondeo con intervalo acotado) y copia su estado
  y artefactos al descriptor. Así árbol, cancelación y presupuesto de tareas funcionan igual.
  Cancelar el trabajo envía `CancelTask` al remoto; su resultado se informa sin afirmar
  éxito (como en A10).
- **D4. Todo con tiempo máximo y tope de tamaño:** conexión, petición, respuesta de tarjeta,
  cuerpo de resultado (se acota a `MAX_NOTICE_BYTES` al llegar al modelo, como cualquier
  subagente). Un remoto lento o enorme no cuelga ni llena el prompt.
- **D5. Solo HTTP+JSON-RPC primero.** gRPC y la rama REST quedan fuera.
- **D6. `A2A-Version`** se envía siempre; una respuesta de versión no soportada es error visible,
  **no se degrada en silencio**.
- **D7. Servir exige autenticación** y escucha por defecto en `127.0.0.1`; sin esquema de
  seguridad configurado, el rol `serve` no arranca (nunca «un agente sin puerta»).
- **D8. Pruebas del servidor sin puerto** (se prueba el enrutador en memoria). **Las del cliente usan
  `wiremock`**, que abre un puerto de bucle local, igual que los proveedores (decisión de la persona
  usuaria, 2026-10-23, que prevalece sobre el «prohibido bindear puertos» de la spec del perfil para
  este caso).
- **D9. Pila HTTP:** cliente con `reqwest` (ya en el núcleo, `rustls`). Servidor con **`axum`**
  (decisión de la persona usuaria, 2026-10-23), dentro de `codeloom-a2a` y solo a partir de A8b: el
  resto del programa no lo ve.
- **D11. Los remotos se dan de alta en perfiles `.md`** con `transport: a2a-remote` y
  `a2a.endpoint` (global en `~/.agents`, de proyecto en el repositorio); `codeloom.toml` guarda solo
  el rol y los límites. La aprobación se ata al **hash del contenido de la tarjeta**: si el remoto la
  cambia, vuelve a `Pending`. Se respetan `Cache-Control` y `ETag` (§8.6) y, si la tarjeta lleva
  firma, se intenta verificar al menos una (SHOULD, §8.4; en A8d).
- **D10. Secretos:** las credenciales de un remoto se leen de variables de entorno nombradas en la
  configuración, nunca del propio `codeloom.toml` en claro, y nunca se escriben en el registro de
  eventos ni en el transcript.

## Fuera de alcance, declarado

- gRPC, notificaciones push, tarjeta extendida, firmas JCS (A8d las declara, no las verifica aún).
- Descubrimiento automático de agentes.
- Que un remoto use nuestras herramientas o nuestro `scope`: no viaja en A2A.
- `model` y `effort` hacia un remoto: A2A no los contempla.

## Decisiones de la persona usuaria (2026-10-23)

1. **Pruebas del cliente con `wiremock`:** sí (D8).
2. **Servir además de consumir:** sí; se conserva el corte en fases y servir exige A8d (D1, D7).
3. **Pila del servidor:** `axum` (D9).
4. **Alta de remotos:** perfiles `.md`, con `codeloom.toml` solo para rol y límites (D11).

## Preguntas abiertas

- Cómo se almacenan y rotan las credenciales de un remoto más allá de variables de entorno.
- Si el flujo SSE (A8c) debe reanudarse tras una caída: la spec delega en cada binding y no define
  identificadores de evento.
