# Transporte `A2aRemote` (A8): plan de implementación

> **Para quien ejecute:** este plan es un **mapa de fases**. Cada fase (A8a…A8d) lleva, al
> empezar, su propio plan detallado tarea a tarea, escrito con la fuente normativa ya leída. Tras
> cada tarea: `cargo test -p codeloom-core -p codeloom-a2a`, clippy con `-D warnings`,
> `rustfmt --edition 2021 --check` y un commit. Nada de `cargo fmt --all`.

**Objetivo:** consumir (y después servir) agentes A2A por HTTP, desactivado por defecto.

**Spec:** `docs/specs/2026-10-23-codeloom-a2a-remote-transport-design.md`. **Requiere A4, A7 y A9.**

## Restricciones globales

- `codeloom-core` no conoce A2A ni HTTP del protocolo; todo vive en `codeloom-a2a`.
- Nada sin cota (tarjeta, cuerpos, tiempos). Sin red real en las pruebas.
- Los secretos solo por variables de entorno; nunca al transcript ni al registro.
- Una tarjeta remota es dato de un tercero y nada se registra sin aprobación.

## Revisión: lo que la spec implica y ninguna tarea cubre sola

- Un remoto que tarda, o devuelve una tarjeta o un resultado enormes, no cuelga ni llena el prompt.
- Una versión no soportada es un error visible, no una degradación.
- Cancelar un hijo remoto envía la cancelación y no afirma el éxito si el remoto no lo confirma.
- Un remoto no aprobado no puede recibir tareas ni ver el prompt.
- Con los roles apagados no se abre ni un socket ni se hace una petición.
- Una tarjeta con texto que intenta dar órdenes se muestra como dato y no cambia el prompt del padre.

### Tarea 0: reconciliar la fuente normativa

- [x] Hecha el 2026-10-23. La especificación está en
  `https://raw.githubusercontent.com/a2aproject/A2A/main/docs/specification.md` (158 KB; no se
  guarda entera en el repositorio). Contrastado y confirmado: métodos PascalCase (§9.1, §9.4),
  códigos de error (§5.4, §9.5), `ErrorInfo` en `error.data` (array), `A2A-Version` en cada petición
  (§3.6.1, §9.2), tarjeta en `/.well-known/agent-card.json` (§8.2), y la elección de interfaz:
  la primera entrada de `supportedInterfaces` que el cliente soporte, con su propia URL y `tenant`
  (§8.3.2). Fixtures en `crates/codeloom-a2a/tests/fixtures/` (véase su `README.md`: la tarjeta, los
  errores y las peticiones de `GetTask` y `CancelTask` son literales; el par de `SendMessage` es
  **compuesto**, porque la spec no trae ejemplo literal del binding JSON-RPC).
  Hallazgos que cambian el plan: `SendMessage` es **bloqueante por defecto** (espera estado terminal
  o interrumpido; `returnImmediately: true` lo evita, §3.2.2), así que el cliente enviará
  `returnImmediately: true` y seguirá la tarea; el endpoint JSON-RPC sale de la tarjeta, no es
  `/rpc`; el §9.3 de la spec aún dice `"method": "category/action"` y el §9.4.1 deja sin cerrar un
  bloque: erratas de la propia spec, se sigue §9.4.

### Fase A8a: consumir

- [x] **T1. Configuración.** `[a2a] role = "off"` (por defecto), `request_timeout_secs`,
  `max_card_bytes`, `max_result_bytes`. Prueba: con `off` no se construye ningún cliente.
- [x] **T2. Tarjeta remota.** Descarga de `/.well-known/agent-card.json` con tiempo y tope, y
  conversión a `AgentCard`. Pruebas con cassettes: tarjeta válida, demasiado grande, JSON roto,
  redirección a otro anfitrión (rechazada), versión no soportada.
- [x] **T3. Fuente de agentes remotos** (perfiles `.md` con `transport: a2a-remote`) para el registro, con `Approval::Pending` atada al hash de la tarjeta: si cambia, vuelve a pendiente. Pruebas: no
  aparece como usable hasta aprobarse; su texto no entra al prompt del padre sin marcarse como dato.
- [x] **T4. Cliente `A2aRemoteTransport`** (`impl AgentTransport`): `send_message`, `get_task`,
  `list_tasks`, `cancel_task`. Se ejecuta `transport_contract` (A7) con cassettes.
- [x] **T5. Espejo en el lanzador.** `Execution::Remote` deja de rechazarse: un trabajo sigue la
  tarea remota y refleja estado y artefactos. Pruebas: ciclo completo, cancelación, remoto que
  cae a mitad, remoto que no responde (agotamiento de tiempo).
- [x] **T6. Documentos** y roadmap (A8a).

**Desviaciones del plan al implementar (A8a, 2026-10-23):**

- **T3 se partió en T3a (núcleo) y T3b (`refresh_remote_cards`).** En el núcleo, `Execution::Remote`
  pasó a llevar un `RemoteSpec`; el registro guarda el resultado de consultar cada tarjeta
  (`set_remote_card`) y ata la aprobación a la huella del punto de entrada y de la tarjeta.
- **T4 no es un `impl AgentTransport`.** `AgentTransport` trabaja con `SessionId` locales y un remoto
  identifica sus tareas con un texto. El cliente (`JsonRpcClient`) implementa un contrato mínimo del
  núcleo, `RemoteTaskClient` (`send`, `get`, `cancel`); `list_tasks` no se implementó porque el
  espejo no lo necesita. `AgentTransport` se aplica, como a cualquier agente, al lanzador con espejo.
- **T5 se partió en T5a (espejo en el núcleo, con un cliente guionizado) y T5b (fábrica de clientes y
  prueba de extremo a extremo con `wiremock`).** La suite `transport_contract` no se ejecutó contra
  un remoto: lo que añade el remoto no es un transporte distinto sino el espejo, que se prueba aparte.
- **Seguimiento por sondeo**, no por SSE (eso es A8c).

### Fase A8b: servir (solo tras A8d-T1)

- [ ] Dependencia `axum` (solo en `codeloom-a2a`). Enrutador en memoria con `SendMessage`, `GetTask`, `CancelTask` sobre `InProcessTransport`;
  tarjeta publicada; escucha en `127.0.0.1`. Pruebas sin puerto, con el enrutador llamado en memoria. Las del cliente (A8a) usan `wiremock`.

### Fase A8c: SSE (tras A9)

- [ ] `SendStreamingMessage` y `SubscribeToTask` con `subscribe_task`; el cliente sigue la tarea por
  flujo en lugar de sondear.

### Fase A8d: seguridad

- [ ] Esquemas de autenticación (clave de API y portador primero), autorización por operación,
  HTTPS obligatorio fuera de bucle local, firmas declaradas pero no verificadas, `icon_url`.
  El rol `serve` no arranca sin esquema configurado.
