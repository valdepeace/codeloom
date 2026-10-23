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

- [ ] Localizar la copia local de la spec (`spec.md`) y de `a2a.proto` citada en la spec del perfil
  y, con ella, contrastar lo ya anotado (métodos PascalCase, códigos de error, `ErrorInfo`, cabecera
  de versión) y cerrar lo abierto (`raw`, campos de los eventos). Guardar **fixtures literales**
  (tarjeta, `SendMessage`, `GetTask`, `CancelTask`, errores) en
  `crates/codeloom-a2a/tests/fixtures/`. Corregir la spec y `wire.rs` si algo no coincide.
  Commitear solo documentos y fixtures.

### Fase A8a: consumir

- [ ] **T1. Configuración.** `[a2a] role = "off"` (por defecto), `request_timeout_secs`,
  `max_card_bytes`, `max_result_bytes`. Prueba: con `off` no se construye ningún cliente.
- [ ] **T2. Tarjeta remota.** Descarga de `/.well-known/agent-card.json` con tiempo y tope, y
  conversión a `AgentCard`. Pruebas con cassettes: tarjeta válida, demasiado grande, JSON roto,
  redirección a otro anfitrión (rechazada), versión no soportada.
- [ ] **T3. Fuente de agentes remotos** (perfiles `.md` con `transport: a2a-remote`) para el registro, con `Approval::Pending` atada al hash de la tarjeta: si cambia, vuelve a pendiente. Pruebas: no
  aparece como usable hasta aprobarse; su texto no entra al prompt del padre sin marcarse como dato.
- [ ] **T4. Cliente `A2aRemoteTransport`** (`impl AgentTransport`): `send_message`, `get_task`,
  `list_tasks`, `cancel_task`. Se ejecuta `transport_contract` (A7) con cassettes.
- [ ] **T5. Espejo en el lanzador.** `Execution::Remote` deja de rechazarse: un trabajo sigue la
  tarea remota y refleja estado y artefactos. Pruebas: ciclo completo, cancelación, remoto que
  cae a mitad, remoto que no responde (agotamiento de tiempo).
- [ ] **T6. Documentos** y roadmap (A8a).

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
