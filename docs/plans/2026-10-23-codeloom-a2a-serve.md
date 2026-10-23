# A8b: servir CodeLoom como agente A2A. Plan de implementación

> **Para agentes:** ejecutar tarea a tarea; cada tarea acaba con sus pruebas, `clippy -D warnings`,
> `rustfmt --check` sobre los ficheros tocados y un commit. Sin red real: el enrutador se prueba en memoria.

**Objetivo:** que otro cliente A2A pueda enviar tareas de solo lectura a un agente de CodeLoom, con
autenticación obligatoria, escuchando solo en bucle local y con todo acotado.

**Arquitectura:** un submódulo `serve` en `codeloom-a2a`. Un `Router` de `axum` sirve la tarjeta y un
punto JSON-RPC; los métodos delegan en un trait `TaskHost` (lo que el servidor necesita saber hacer con
una tarea) que implementa un adaptador sobre `SubagentLauncher`. El núcleo no cambia salvo un evento
nuevo para avisar en la conversación.

**Tecnología:** `axum` 0.8 (decisión D9 de A8a, solo en `codeloom-a2a`), `tower` 0.5 con `util` solo como
dependencia de pruebas.

**Spec:** `docs/specs/2026-10-23-codeloom-a2a-serve-design.md` (D1 a D13).

## Restricciones globales

- `codeloom-core` no depende de `axum`, `hyper` ni `tower`.
- El secreto de autenticación: comparación en tiempo constante, nunca en `Debug`, errores ni eventos.
- Escucha solo en bucle local; cuerpo ≤ 64 KiB; texto del mensaje ≤ 32 KiB; ≤ 4 tareas vivas.
- El agente servido solo dispone de `view`, `glob` y `grep`.
- Códigos de error de A2A tal como los fija §5.4; `A2A-Version: 1.0` obligatoria.

## Revisión: casos que la spec implica y nadie probaría por defecto

1. Petición a `/a2a` sin credencial, con credencial vacía y con un prefijo del secreto correcto: 401
   en los tres casos y sin cuerpo informativo.
2. `GetTask`/`CancelTask` de un id inexistente y de uno de otra sesión: respuesta idéntica byte a byte.
3. JSON-RPC mal formado, `id` ausente o `method` no cadena: error de JSON-RPC (−32700/−32600), no 500.
4. Cuerpo por encima del tope: 413 sin leerlo entero.
5. Quinta tarea con cuatro vivas: 503 y ninguna tarea nueva en el almacén.
6. `SendMessage` con parte `raw`/`url`/`data`: −32005 y ninguna tarea creada.
7. Arranque sin variable de entorno del secreto, o con la variable vacía: el servidor no arranca.
8. Dirección de escucha no local (`0.0.0.0`, una IP pública, un nombre de anfitrión): error de configuración.

---

### Tarea 1: Configuración `[a2a.serve]` y su validación

**Ficheros:** modificar `crates/codeloom-a2a/src/config.rs`; crear `crates/codeloom-a2a/src/serve/mod.rs`
(vacío salvo `pub mod config_check;` si procede) solo si hace falta para compilar.

**Interfaces — produce:**

```rust
pub enum ServeAuthScheme { Bearer, ApiKey { name: String } }       // "bearer" | "apikey" + name
pub struct ServeAuthConfig { pub scheme: ServeAuthScheme, pub env: String }
pub struct ServeConfig { pub listen: String, pub agent: String, pub auth: Option<ServeAuthConfig>,
                         pub public_url: Option<String>, pub max_concurrent_tasks: usize,
                         pub max_wait_secs: u64, pub max_task_secs: u64 }   // por defecto 127.0.0.1:7077, "default", 4, 30, 600
impl A2aConfig { pub serve: ServeConfig }                          // sección [a2a.serve]
impl ServeConfig {
    pub fn listen_addr(&self) -> Result<std::net::SocketAddr>;     // solo IP literal de bucle local
    pub fn validate(&self, env: &dyn Fn(&str) -> Option<String>) -> Result<ResolvedAuth>;
}
pub struct ResolvedAuth { /* esquema + secreto (Secret) */ }     // Debug redactado
```

- [ ] **Paso 1: pruebas:** valores por defecto; `[a2a.serve]` parsea ambos esquemas; casos 7 y 8 de la
  revisión (`0.0.0.0:1`, `192.0.2.1:1`, `localhost:1`, `[::1]:1` sí); cotas en cero o sobre su techo son error;
  el `Debug` de `ResolvedAuth` no contiene el secreto; `public_url` solo `https://` o bucle local.
- [ ] **Paso 2-4:** implementar, pruebas en verde. **Paso 5:** commit
  `feat(a2a): la sección [a2a.serve] y su validación`.

### Tarea 2: Autenticación de entrada y tarjeta servida (puro, sin `axum`)

**Ficheros:** crear `crates/codeloom-a2a/src/serve/auth.rs`, `serve/card.rs`.

**Interfaces — produce:**

```rust
// auth.rs
pub struct Authenticator { /* ResolvedAuth */ }
impl Authenticator {
    pub fn new(auth: ResolvedAuth) -> Self;
    /// Recibe las cabeceras ya extraídas; comparación en tiempo constante (se comparan los SHA-256).
    pub fn check(&self, authorization: Option<&str>, header_value: &dyn Fn(&str) -> Option<String>) -> bool;
}
// card.rs
pub struct ServedCard { pub json: serde_json::Value, pub etag: String }
pub fn served_card(card: &codeloom_core::card::AgentCard, interface_url: &str,
                   scheme: &ServeAuthScheme) -> ServedCard;
```

- [ ] **Paso 1: pruebas:** caso 1 de la revisión (sin credencial, vacía, prefijo, `bearer` con mayúsculas
  en el esquema, `Basic` donde se espera `Bearer`); `apikey` por su cabecera; la tarjeta servida
  contiene `securitySchemes` y `securityRequirements` con los nombres del cable y `capabilities`
  con `streaming`/`pushNotifications`/`extendedAgentCard` a `false`; el `etag` cambia si cambia la tarjeta;
  la tarjeta servida **se parsea con `WireAgentCard`** (ida y vuelta con nuestro propio cliente) y
  `CardSecurity::from_card_json` la interpreta con el mismo esquema.
- [ ] **Paso 2-4.** **Paso 5:** commit `feat(a2a): la autenticación de entrada y la tarjeta servida`.

### Tarea 3: Enrutador JSON-RPC sobre un `TaskHost`

**Ficheros:** `crates/codeloom-a2a/Cargo.toml` (`axum = "0.8"`; `[dev-dependencies] tower = { version = "0.5", features = ["util"] }`),
crear `serve/rpc.rs`, `serve/mod.rs` (estado y `pub fn router(...)`).

**Interfaces — produce:**

```rust
#[async_trait]
pub trait TaskHost: Send + Sync {
    async fn start(&self, prompt: String) -> Result<WireTaskView, HostError>;           // crea la tarea
    async fn get(&self, id: &str) -> Result<WireTaskView, HostError>;                    // NotFound si no es nuestra
    async fn cancel(&self, id: &str) -> Result<WireTaskView, HostError>;
    async fn wait_terminal(&self, id: &str, max: Duration) -> Result<WireTaskView, HostError>;
}
pub enum HostError { NotFound, NotCancelable, Busy, Other(String) }
pub struct WireTaskView(pub serde_json::Value);                                          // ya en nombres del cable
pub struct ServeState { pub host: Arc<dyn TaskHost>, pub auth: Authenticator, pub card: ServedCard,
                        pub max_wait: Duration }
pub fn router(state: Arc<ServeState>) -> axum::Router;
```

Rutas: `GET /.well-known/agent-card.json` (pública, con `ETag` y `Cache-Control: max-age=60`) y
`POST /a2a` (autenticada). Orden en `/a2a`: autenticación → tope de cuerpo (64 KiB) → versión → JSON-RPC → método.

- [ ] **Paso 1: pruebas** con un `TaskHost` falso y `tower::ServiceExt::oneshot`, sin puertos:
  los casos 1 a 6 de la revisión; `SendMessage` bloqueante espera con `wait_terminal` y con
  `returnImmediately` no; `SendMessage` con `taskId` da −32004; `ListTasks`, `SendStreamingMessage`,
  `SubscribeToTask` dan −32004; push dan −32003; método desconocido −32601; versión ausente/`0.3`/`1.1` dan
  −32009; `GetTask` de ajeno e inexistente son idénticos; el cuerpo de un 401 está vacío.
- [ ] **Paso 2-4.** **Paso 5:** commit `feat(a2a): el enrutador JSON-RPC del servidor`.

### Tarea 4: Adaptador sobre el lanzador

**Ficheros:** crear `serve/host.rs`.

**Interfaces — produce:**

```rust
pub struct LauncherHost { /* launcher, store, parent: SessionId, agent: String, límites, bus */ }
impl LauncherHost {
    pub async fn new(launcher: Arc<dyn SubagentLauncher>, store: Arc<dyn SessionStore>,
                     cwd: &str, agent: String, limits: &ServeConfig, bus: EventBus) -> Result<Self>;
}
impl TaskHost for LauncherHost { /* … */ }
pub fn served_tools() -> codeloom_core::tools::ToolRegistry;   // view + glob + grep, nada más
```

La sesión padre del servidor se crea al construir. Las tareas se identifican por el id del hijo; `get`
busca entre `launcher.list(parent)` y convierte con `WireTask::from(&descriptor)`. `start` rechaza
con `Busy` si ya hay `max_concurrent_tasks` vivas. Un vigilante cancela lo que pase de `max_task_secs`.

- [ ] **Paso 1: pruebas** con el lanzador real en proceso y un proveedor de prueba que responde texto
  (definido en el test): `served_tools` contiene exactamente `view`, `glob`, `grep`; una tarea completa y
  su artefacto sale acotado; la quinta con cuatro vivas es `Busy` y no crea tarea (caso 5);
  una tarea que supera `max_task_secs` queda `canceled`; un id de otra sesión es `NotFound` (caso 2).
- [ ] **Paso 2-4.** **Paso 5:** commit `feat(a2a): el adaptador del servidor sobre el lanzador`.

### Tarea 5: Arranque desde la CLI y aviso en la conversación

**Ficheros:** `crates/codeloom-a2a/src/serve/mod.rs` (`pub async fn serve(...)`), `crates/codeloom-cli/src/main.rs`,
`crates/codeloom-core/src/event/mod.rs`, `crates/codeloom-tui/src/app.rs`.

**Interfaces — produce:** `pub async fn serve(state: Arc<ServeState>, addr: SocketAddr, shutdown: CancellationToken) -> io::Result<SocketAddr>`
(devuelve la dirección real, útil con el puerto 0 en pruebas) y `LiveEvent::A2aServed { task: String, agent: String, state: TaskState }`.

- [ ] **Paso 1: pruebas:** rol `serve` sin secreto en el entorno no arranca (mensaje que nombra la variable) y no abre
  ningún puerto; rol `off` y `consume` no sirven; `LiveEvent::A2aServed` genera una línea de sistema en la conversación
  con el estado A2A; el aviso se acota.
- [ ] **Paso 2-4.** **Paso 5:** commit `feat(cli): arrancar el servidor A2A con aviso en la conversación`.

### Tarea 6: Recorrido completo, prueba viva y documentación

**Ficheros:** `crates/codeloom-a2a/tests/serve_round_trip.rs`, `crates/codeloom-a2a/tests/live_serve.rs`,
docs, índice y README.

- [ ] **Paso 1:** el cliente de A8a (`JsonRpcClient` con `AuthPlan`) contra el servidor real por bucle local
  (puerto 0): tarjeta descargada y aprobada, envío, sondeo hasta `completed`, cancelación, y con credencial
  equivocada `AuthFailed`.
- [ ] **Paso 2:** prueba a mano con el **cliente del SDK oficial** (`a2a-sdk`) contra el servidor de CodeLoom;
  anotar lo contrastado y lo que no.
- [ ] **Paso 3:** documentar (Estado en la spec, índice A2A, README) con honestidad sobre lo no visto en terminal.
  Commit `docs: A8b, servir CodeLoom como agente A2A`.
