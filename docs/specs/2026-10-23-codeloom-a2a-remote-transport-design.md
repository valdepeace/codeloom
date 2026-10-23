# Transporte `A2aRemote` de A2A: consumir y servir agentes por HTTP (A8)

Estado: propuesta del 2026-10-23, pendiente de aprobación (tarea A8 de
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

## Lo que NO consta o se contradice (resolver en la tarea 0)

- **Nombres de los métodos de JSON-RPC:** el resumen dice `SendMessage` (PascalCase) y a la vez
  `message/send`. **No se sabe cuál es el de la v1.0.**
- **Cuerpo de error de HTTP+JSON:** `google.rpc.Status` frente a `application/problem+json`.
- **Códigos de error** (`-32001` TaskNotFound, `-32002` TaskNotCancelable, …): vienen de un resumen.
- **Codificación de `raw`** y negociación de versión entre menores.
- **Cómo se autentica un cliente ante un remoto concreto:** lo dice su tarjeta, que es dato de un
  tercero.

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
- **D8. Sin bindear puertos en las pruebas del servidor:** se prueba el enrutador en memoria. El
  cliente se prueba con `wiremock`, como los proveedores (véase la pregunta abierta).
- **D9. Pila HTTP:** cliente con `reqwest` (ya en el núcleo, `rustls`). Servidor: crate opcional
  del borde; la elección (`axum` u otra) se decide al empezar A8b.
- **D10. Secretos:** las credenciales de un remoto se leen de variables de entorno nombradas en la
  configuración, nunca del propio `codeloom.toml` en claro, y nunca se escriben en el registro de
  eventos ni en el transcript.

## Fuera de alcance, declarado

- gRPC, notificaciones push, tarjeta extendida, firmas JCS (A8d las declara, no las verifica aún).
- Descubrimiento automático de agentes.
- Que un remoto use nuestras herramientas o nuestro `scope`: no viaja en A2A.
- `model` y `effort` hacia un remoto: A2A no los contempla.

## Preguntas abiertas (para la persona usuaria)

1. **Pruebas con `wiremock`:** la spec del perfil dice «prohibido bindear puertos; `InProcess` o
   cassettes», pero `AGENTS.md` admite `wiremock` (que abre un puerto de bucle local) y los
   proveedores ya lo usan. ¿Se permite para el cliente remoto?
2. **¿Se quiere servir, o solo consumir?** Consumir da valor inmediato; servir exige cerrar la
   seguridad antes. Se propone empezar por A8a.
3. **Pila del servidor** (si se sirve): `axum` frente a otra.
4. **Dónde se configuran los remotos:** perfil `.md` con `transport: a2a-remote` (ya previsto)
   frente a una lista en `codeloom.toml`.
