# A8d: autenticación hacia agentes remotos A2A (lado cliente)

Estado: propuesta del 2026-10-23. La persona usuaria ordenó abordarla («haz la 1, 2 y 3») sin
responder a las preguntas abiertas; se adoptan sus opciones recomendadas, rotuladas como tales abajo.
Continúa `2026-10-23-codeloom-a2a-remote-transport-design.md` (A8a, ya hecha) y es
prerrequisito de A8b (servir), cuya autenticación del lado servidor se especifica allí.

## Premisa

Cabe señalar que A2A **no** prescribe un modelo de autorización: «Authorization boundaries are
defined by each agent's authorization model, not prescribed by the protocol» (§13.1). Lo que sí
fija es cómo una tarjeta **declara** lo que pide (`securitySchemes`, `securityRequirements`) y que
«Production deployments MUST use encrypted communication» (§13.4). Todo lo demás es decisión
nuestra y se rotula como tal.

## Problema

Hoy un remoto cuya tarjeta declara `securityRequirements` se rechaza con «exige autenticación»
(`subagent.rs`, `requires_auth`). En la práctica eso deja fuera a casi cualquier agente real
desplegado: sin A8d, CodeLoom solo habla con servidores abiertos de bucle local.

## Lo que consta

- La tarjeta lleva `security_schemes: map<string, SecurityScheme>` y
  `security_requirements: repeated SecurityRequirement` («Security requirements for contacting the
  agent», proto). `SecurityRequirement.schemes` es `map<string, StringList>` (esquema → ámbitos).
- `SecurityScheme` es una unión de cinco: `api_key_security_scheme` (`location`: «query», «header»
  o «cookie»; `name`), `http_auth_security_scheme` (`scheme`, p. ej. «Bearer»; `bearer_format`),
  `oauth2_security_scheme` (`flows`, `oauth2_metadata_url`), `open_id_connect_security_scheme`
  (`open_id_connect_url`) y `mtls_security_scheme`.
- Forma en JSON: camelCase (§5.5). El ejemplo literal de §8.5 (fixture `agent-card.sample.json`)
  muestra `"securitySchemes": {"google": {"openIdConnectSecurityScheme": {...}}}` y
  `"securityRequirements": [{"schemes": {"google": {"list": ["openid", ...]}}}]`.
- `auth-required` es un estado de tarea; y «Agents MUST NOT treat the `TASK_STATE_AUTH_REQUIRED`
  state transition, by itself, as authorization».
- La huella de la tarjeta (A8a) ya se calcula sobre el JSON normalizado **entero**, así que un cambio
  en los esquemas de seguridad devuelve al remoto a «pendiente» sin trabajo adicional.

## Lo que no consta (y por tanto es nuestro)

- Que `securityRequirements` sea «alternativas (O) de conjuntos (Y)»: es la semántica de OpenAPI, de
  la que el proto dice derivarse (`SecurityScheme`: «based on the OpenAPI 3.2 Security Scheme
  Object»), pero A2A no la escribe. **Lectura nuestra.**
- Los nombres JSON de los miembros de la unión distintos de `openIdConnectSecurityScheme`: se
  infieren del camelCase del proto (`apiKeySecurityScheme`, `httpAuthSecurityScheme`,
  `oauth2SecurityScheme`, `mtlsSecurityScheme`). Se contrastan contra un servidor real en la
  prueba viva (tarea 6 del plan) y, hasta entonces, son una suposición.
- Cómo viaja un token de OAuth o de OIDC ni cómo se obtiene: fuera de alcance.
- Si la clave en `cookie` es admisible: no se admite (D2).

## Decisiones (nuestras salvo indicación)

- **D1. Solo el lado cliente.** La autenticación para *servir* se especifica con A8b. Aquí nace el
  modelo común de la tarjeta (`security.rs` en `codeloom-a2a`), que A8b reutilizará para declarar.
- **D2. Esquemas admitidos:** clave de API en cabecera o en consulta, y HTTP `Bearer` y `Basic`.
  OAuth2, OpenID Connect, mTLS y clave en cookie **no** se implementan: un remoto que solo ofrezca
  esos queda rechazado con un motivo legible («el esquema `x` (OAuth2) no está soportado todavía»),
  nunca con un silencio ni con un intento sin credencial. Es una frontera honesta, no un hueco
  escondido. *(Actualizado el 2026-10-24: OAuth2 `client_credentials`, OpenID Connect y mTLS ya se
  soportan; la clave en cookie y los demás flujos OAuth2 siguen sin soporte.)*
- **D3. Elección.** `securityRequirements` se lee como alternativas; el cliente toma la **primera**
  cuyos esquemas admite todos y para los que hay credencial. Si ninguna sirve, el motivo nombra la
  primera alternativa y lo que le falta. Los ámbitos (`scopes`) se ignoran en los esquemas
  admitidos, que no los usan.
- **D4. Credenciales en variables de entorno, nombradas en el perfil.** El perfil remoto gana
  `a2a.credentials`, un mapa «nombre del esquema en la tarjeta → nombre de la variable de entorno»:

  ```yaml
  a2a:
    endpoint: https://agente.example
    credentials:
      claveApi: AGENTE_EJEMPLO_API_KEY
  ```

  El valor **nunca** se escribe en el perfil ni en `codeloom.toml`. Para `Basic`, la variable
  contiene `usuario:contraseña`. Se lee al construir el cliente, no al cargar el perfil, y una
  variable ausente o vacía es «falta la credencial `X`», no un cliente sin autenticación.
- **D5. El secreto no sale de su sitio.** Se envía solo a la URL de la interfaz aprobada, solo por
  HTTPS (o por HTTP en bucle local, como ya admite A8a). No se sigue ninguna redirección a otro
  anfitrión, de modo que ni una cabecera ni un parámetro de clave viajan a un tercero.
- **D6. El secreto no se registra.** Tipo `Secret` con `Debug` y `Display` redactados; no entra en el
  bus de eventos, el transcript, los errores ni la sesión. Una prueba lo vigila.
- **D7. Errores.** HTTP 401/403 produce `RemoteTaskError::AuthFailed` (variante nueva), distinto de
  un fallo de red; el mensaje nombra el esquema usado y **no** repite el cuerpo del remoto salvo
  acotado. Un 401 sin credencial configurada ya no ocurre: se detecta antes (D3).
- **D8. Aviso antes de delegar.** El motivo de que un remoto no sea utilizable por autenticación
  (esquema no soportado, variable ausente) se calcula al descargar la tarjeta y se guarda junto a
  ella; el lanzador lo devuelve tal cual y el popup `/agents` lo muestra en la fila. Así la persona
  sabe qué variable exportar sin leer un registro.
- **D9. `auth-required` remoto se refleja, no se resuelve.** Si el remoto pasa a `auth-required`, la
  tarea se refleja con ese estado y el mensaje del remoto, como dato. CodeLoom no reintenta con otra
  credencial ni da la transición por autorización.
- **D10. Firmas y tarjeta extendida: fuera.** Siguen sin verificarse y sin pedirse (declarado en
  A8a). La tarjeta firmada se trata como la no firmada, y así lo dice la documentación.

## Fuera de alcance, declarado

- ~~OAuth2 (incluido `client_credentials`), OpenID Connect y mTLS~~: hechos el 2026-10-24 (A8d-2), véase
  `2026-10-24-codeloom-a2a-remote-closure-design.md`.
- Rotación o caducidad de credenciales: la variable se lee cuando se construye el cliente.
- ~~Verificación de firmas JCS~~: hecha en A8d-2. Siguen fuera `Cache-Control` y `ETag` de la tarjeta.
- Cualquier cosa del lado servidor.

## Preguntas abiertas (resueltas con la opción recomendada, a revisar por la persona usuaria)

1. ¿Basta con clave de API y `Bearer`/`Basic` como primera entrega, dejando OAuth2 para cuando haya un
   remoto real que lo exija? (Recomendado: sí.)
2. ¿Credenciales por variable de entorno (D4) o prefieres además un fichero de secretos aparte?
   (Recomendado: solo variables; un fichero de secretos es otra superficie que proteger.)
3. ¿El motivo de «no utilizable» en la fila de `/agents` (D8) te vale como única señal, o quieres
   también un aviso en el transcript al arrancar?

## Estado

**Implementada el 2026-10-23** (lado cliente), con decisiones D1 a D10 tal como se escribieron, y las
tres preguntas abiertas resueltas con la opción recomendada. Commits: `513a814` (módulo `security`),
`d915619` (perfiles con `a2a.credentials`; el lanzador consulta a la fábrica), `dc8145c` (el cliente se
autentica, 401/403 como `AuthFailed`, la fábrica y la sincronización) y `bb094ac` (la fila de `/agents`
y la prueba de extremo a extremo con un remoto protegido).

### Contrastado contra un servidor real

Con el servidor del SDK oficial (`a2a-sdk` 1.2.3) y una puerta de `Bearer` delante, el 2026-10-23:

- La tarjeta que emite usa exactamente `apiKeySecurityScheme` y `httpAuthSecurityScheme`: **la
  inferencia de nombres de la sección «Lo que no consta» queda confirmada** para esos dos. Los de
  OAuth2 (`oauth2SecurityScheme`) y mTLS (`mtlsSecurityScheme`) siguen sin contrastar; no importa
  porque no se soportan y se rechazan igual.
- Un `StringList` vacío sale como `{}` (sin la clave `list`); el lector solo usa las claves de
  `schemes`, así que no depende de ello.
- Sin variable de entorno no hay cliente y el motivo nombra la variable; con un token equivocado el
  servidor contesta 401 y se reconoce como `AuthFailed` sin repetir nada; con el token bueno la tarea
  completa. Pruebas `a_real_secured_server_*` en `crates/codeloom-a2a/tests/live_remote.rs`
  (`#[ignore]`, a mano).

### Lo que no consta o queda pendiente

- **OAuth2, OpenID Connect, mTLS y clave en cookie:** sin soporte (D2). Es el siguiente paso si algún
  remoto real los pide.
- **La variable de entorno se lee al construir el cliente:** una rotación del secreto en mitad de una
  tarea larga no se nota hasta la siguiente. El motivo de «no usable» que muestra `/agents` se calcula
  al descargar la tarjeta (arranque); el lanzador, en cambio, lo reevalúa justo antes de delegar.
- Sin respuesta a `auth-required` de un remoto: se refleja como estado, no se resuelve (D9).
- Visto en una terminal real: **no** (solo pruebas automáticas y la prueba viva contra el SDK).
- Un secreto en la URL (clave de API en consulta) puede quedar en los registros del servidor remoto o
  de un proxy intermedio: es una propiedad de ese esquema, no de CodeLoom. Se prefiere la cabecera.
