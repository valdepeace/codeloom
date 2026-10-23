# A8d: autenticación hacia remotos A2A. Plan de implementación

> **Para agentes:** ejecutar tarea a tarea; cada tarea acaba con sus pruebas, `clippy -D warnings`,
> `rustfmt --check` sobre los ficheros tocados, y un commit.

**Objetivo:** que CodeLoom hable con agentes remotos cuya tarjeta pide clave de API o `Bearer`/`Basic`,
con la credencial tomada de una variable de entorno y sin que el secreto salga de su sitio.

**Arquitectura:** un módulo puro `security` en `codeloom-a2a` interpreta `securitySchemes` y
`securityRequirements` y construye un `AuthPlan`; el cliente JSON-RPC lo aplica a cada petición. El
núcleo solo transporta datos opacos (los nombres de variable del perfil y el JSON de seguridad de la
tarjeta) y delega en la fábrica la pregunta «¿es utilizable este remoto?».

**Spec:** `docs/specs/2026-10-23-codeloom-a2a-remote-auth-design.md` (decisiones D1 a D10).

## Restricciones globales

- `codeloom-core` no conoce esquemas de seguridad: solo `Option<String>` y nombres de variable.
- Ningún secreto en `Debug`, `Display`, errores, eventos ni sesión (D6).
- Sin red real salvo `wiremock` de bucle local (D8 de A8a).
- Todo texto de un tercero que llegue a un mensaje, acotado.
- Un remoto con un esquema no soportado se rechaza con motivo; jamás se intenta sin credencial.

## Revisión: casos que la spec implica y ninguna tarea pruebaría por defecto

1. Alternativa con dos esquemas (Y) donde falta la credencial de uno: no se elige y el motivo nombra la
   variable que falta.
2. Variable de entorno definida pero vacía: cuenta como ausente.
3. Clave de API en `cookie`: rechazada con motivo, no ignorada.
4. Una redirección del remoto a otro anfitrión no recibe la clave (la política de A8a ya la detiene;
   una prueba lo fija con la clave presente).
5. Respuesta 401 del remoto cuyo cuerpo contiene la propia credencial: el mensaje de error no la repite.

---

### Tarea 1: Interpretar la seguridad de la tarjeta y construir el plan de autenticación

**Ficheros:** crear `crates/codeloom-a2a/src/security.rs`; modificar `crates/codeloom-a2a/src/lib.rs`
(`pub mod security;`).

**Interfaces — produce:**

```rust
pub struct Secret(String);                       // Debug/Display = "«secreto»"; sin Clone público ni Serialize
impl Secret { pub fn new(v: String) -> Option<Secret>; /* None si vacío */ pub(crate) fn expose(&self) -> &str }

pub enum SchemeKind { ApiKey { location: KeyLocation, name: String }, HttpBearer, HttpBasic,
                      Unsupported(String) }       // String = nombre legible: «OAuth2», «OpenID Connect», «mTLS», «clave en cookie», «HTTP Digest»…
pub enum KeyLocation { Header, Query }

pub struct CardSecurity { schemes: BTreeMap<String, SchemeKind>, alternatives: Vec<Vec<String>> }
impl CardSecurity {
    pub fn from_card_json(valor: &serde_json::Value) -> CardSecurity;   // tolerante y acotado
    pub fn is_empty(&self) -> bool;                                      // sin requisitos: remoto abierto
    pub fn to_json(&self) -> Option<String>;                             // forma opaca que guarda el núcleo
    pub fn from_json(texto: &str) -> Option<CardSecurity>;
}

pub enum AuthProblem { Unsupported { scheme: String, kind: String },
                       MissingCredential { scheme: String, env: Option<String> } }
pub struct AuthPlan { /* pasos: cabecera (nombre, valor Secret) o parámetro de consulta */ }
impl AuthPlan {
    pub fn open() -> AuthPlan;                                           // sin autenticación
    pub fn choose(security: &CardSecurity, credentials: &BTreeMap<String, String>,
                  env: &dyn Fn(&str) -> Option<String>) -> Result<AuthPlan, AuthProblem>;
    pub fn apply(&self, peticion: reqwest::RequestBuilder) -> reqwest::RequestBuilder;
}
impl std::fmt::Display for AuthProblem { /* mensaje legible, p. ej. «falta la variable AGENTE_KEY para el esquema `claveApi`» */ }
```

- [ ] **Paso 1: pruebas** (nombre → aserción):
  - `parses_the_literal_sample_card`: con `tests/fixtures/agent-card.sample.json`, `schemes["google"]` es
    `Unsupported("OpenID Connect")` y `alternatives == [["google"]]`.
  - `parses_api_key_header_and_query`, `parses_http_bearer_and_basic_case_insensitively`.
  - `cookie_api_key_digest_oauth2_and_mtls_are_unsupported_with_a_readable_name`.
  - `an_unknown_scheme_object_is_unsupported_not_ignored`.
  - `choose_takes_the_first_alternative_whose_schemes_are_all_supported_and_have_a_credential`.
  - `choose_requires_every_scheme_of_an_alternative` (caso 1) y `an_empty_env_value_counts_as_missing` (caso 2).
  - `choose_reports_the_first_alternative_when_none_works`.
  - `an_open_card_chooses_the_open_plan` (sin requisitos → `AuthPlan::open()` y ninguna cabecera).
  - `secret_debug_and_display_never_show_the_value` (D6).
  - `apply_sets_header_bearer_basic_and_query` (sobre un `RequestBuilder` real; se inspecciona con `.build()`):
    `Authorization: Bearer t`, `Authorization: Basic base64(u:p)`, cabecera con el nombre de la clave,
    parámetro de consulta con el nombre de la clave.
  - `json_roundtrip_keeps_the_security_and_bounds_it` (más de 16 KiB → `to_json()` es `None`).
- [ ] **Paso 2:** ejecutarlas y ver que fallan (el módulo no existe).
- [ ] **Paso 3:** implementar. Cotas: 20 esquemas, 10 alternativas, 10 esquemas por alternativa, 200 bytes
  por nombre. `base64` ya es dependencia del crate (`raw` de A4); si no, se añade la misma que use `wire`.
- [ ] **Paso 4:** pruebas en verde. **Paso 5:** commit `feat(a2a): la seguridad de la tarjeta y el plan de autenticación`.

### Tarea 2: El núcleo transporta credenciales (nombres) y seguridad (opaca)

**Ficheros:** `crates/codeloom-core/src/registry/source.rs`, `registry/profile.rs`, `registry/mod.rs`
(constructores de pruebas), `remote_task.rs`, `subagent.rs`.

**Interfaces — produce:**

```rust
// source.rs
pub struct RemoteSpec { pub endpoint: String, pub fetched: Option<RemoteFetched>,
                        pub credentials: BTreeMap<String, String> }      // esquema -> variable
pub struct RemoteFetched { /* lo existente */ pub requires_auth: bool,
                           pub security: Option<String>,                 // JSON opaco, acotado
                           pub auth_problem: Option<String> }            // motivo ya redactado
// remote_task.rs
pub enum RemoteTaskError { /* … */ AuthFailed(String) }                  // #[error("remote rejected the credentials: {0}")]
pub trait RemoteClientFactory { /* … */
    fn unavailable_reason(&self, _name: &str, _spec: &RemoteSpec) -> Option<String> { None } }
```

- [ ] **Paso 1: pruebas.**
  - profile: `a_remote_profile_reads_its_credentials` (`a2a.credentials: {claveApi: AGENTE_KEY}`);
    `a_credential_value_that_is_not_an_env_var_name_does_not_load` (p. ej. `sk-abc 123`, con espacio, o
    más de 128 bytes: el perfil no carga, y el mensaje **no** repite el valor); `credentials_on_a_local_profile_are_reported_as_not_applied`.
  - subagent: `a_remote_is_refused_with_the_factory_reason` (fábrica de prueba con `unavailable_reason`
    = «falta la variable X»; el error contiene el motivo) y `a_remote_that_requires_auth_is_no_longer_refused_by_the_core`
    (sustituye a `a_remote_that_requires_authentication_is_refused_for_now`).
- [ ] **Paso 2-3:** implementar. En `subagent.rs` (línea ~371) el rechazo por `requires_auth` se
  sustituye por la consulta a `remote_clients.unavailable_reason(nombre, spec)`. Validación del nombre de
  variable: `[A-Za-z_][A-Za-z0-9_]*`, máximo 128 bytes.
- [ ] **Paso 4:** `cargo test -p codeloom-core`. **Paso 5:** commit
  `feat(core): perfiles remotos con credenciales por variable de entorno`.

### Tarea 3: El cliente se autentica y el sondeo de la tarjeta evalúa la autenticación

**Ficheros:** `crates/codeloom-a2a/src/remote.rs`, `sync.rs`, `client.rs`, `card.rs`.

**Interfaces — consume:** Tarea 1 y 2. **Produce:**

```rust
// remote.rs
pub struct RemoteCard { /* … */ pub security: Option<String> }           // CardSecurity::to_json
// client.rs
impl JsonRpcClient { pub fn with_auth(self, plan: AuthPlan) -> Self }    // new(...) = AuthPlan::open()
impl RemoteClientFactory for A2aClientFactory {
    fn client_for(..) -> Option<Arc<dyn RemoteTaskClient>>;              // aplica AuthPlan::choose con std::env::var
    fn unavailable_reason(..) -> Option<String>;                         // Some(AuthProblem.to_string()) si no hay plan
}
```

- [ ] **Paso 1: pruebas.**
  - remote: `fetch_card_extracts_the_security_json`; `an_open_card_has_no_security`.
  - client (wiremock): `sends_the_api_key_header`, `sends_the_api_key_in_the_query`, `sends_bearer`,
    `sends_basic`, `an_open_remote_gets_no_authorization_header`; `a_401_becomes_auth_failed_without_the_credential_in_the_message`
    (caso 5: el cuerpo del 401 contiene el token y el error no); `a_403_is_auth_failed_too`.
  - sync: `refresh_stores_the_auth_problem_when_the_env_var_is_missing` y `…clears_it_when_present`
    (la lectura del entorno se inyecta con `&dyn Fn(&str) -> Option<String>`).
  - un remoto que redirige a otro anfitrión no recibe la clave (caso 4): el servidor "otro" de wiremock
    no ve ninguna petición.
- [ ] **Paso 2-3:** implementar. `llamar` aplica el plan con `AuthPlan::apply` a la petición y traduce
  401/403 a `AuthFailed("scheme `x`: HTTP 401")` (sin cuerpo). `requires_auth` de `RemoteFetched` se conserva
  como dato informativo.
- [ ] **Paso 4:** `cargo test -p codeloom-a2a`. **Paso 5:** commit
  `feat(a2a): el cliente remoto se autentica con clave de API, Bearer o Basic`.

### Tarea 4: Extremo a extremo y señal en `/agents`

**Ficheros:** `crates/codeloom-a2a/tests/consume_end_to_end.rs`, `crates/codeloom-tui/src/app.rs`
(`AgentRow::from_entry`), `crates/codeloom-cli/src/main.rs` si la fábrica necesita el entorno.

- [ ] **Paso 1: pruebas.**
  - e2e: `a_secured_remote_is_delegated_to_with_its_bearer_token` (servidor wiremock que exige
    `Authorization: Bearer secreto` y responde `completed`; el perfil declara `credentials`; el espejo
    llega a `completed`) y `without_the_env_var_the_remote_is_refused_and_no_request_is_sent`.
  - e2e: el secreto no aparece en ningún evento del bus recogido durante la prueba (D6).
  - tui: `a_remote_row_shows_why_it_cannot_be_used` (`execution` = `remote · auth: falta la variable X`) y
    `a_usable_remote_row_stays_plain_remote`.
- [ ] **Paso 2-3:** implementar. **Paso 4:** pruebas y `clippy -D warnings`. **Paso 5:** commit
  `feat(tui): /agents indica por qué un remoto no se puede usar`.

### Tarea 5: Prueba viva y documentación

**Ficheros:** `crates/codeloom-a2a/tests/live_remote.rs`, `docs/specs/…-remote-auth-design.md` (Estado),
`docs/plans/2026-10-22-codeloom-a2a-roadmap.md`, `README.md` (sección de agentes remotos),
`docs/specs/2026-10-23-codeloom-a2a-remote-transport-design.md` (quitar «sin autenticación» de pendientes).

- [ ] **Paso 1:** añadir a la prueba viva `A2A_LIVE_TOKEN`: si existe, el cliente envía `Bearer` contra el
  servidor del SDK oficial con un *middleware* que exige ese token (el servidor de apoyo vive fuera del
  repositorio). Contrastar con él los nombres JSON de la unión de esquemas (spec, «Lo que no consta»).
- [ ] **Paso 2:** documentar lo contrastado y lo que sigue siendo suposición; marcar A8d (cliente) hecha
  en el índice. **Paso 3:** commit `docs: A8d, autenticación hacia remotos`.
