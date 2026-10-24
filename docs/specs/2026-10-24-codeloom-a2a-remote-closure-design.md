# Cierre de los agentes remotos: `input-required`, tarjetas vivas y seguridad A8d-2

Fecha: 2026-10-24. Estado: ver el final. Completa `2026-10-23-codeloom-a2a-remote-transport-design.md`
(A8a), `…-remote-auth-design.md` (A8d, primera entrega) y `2026-10-24-codeloom-a2a-streaming-design.md`
(A8c). Cabe señalar que todo lo que sigue es decisión propia de CodeLoom salvo donde se cite la
especificación A2A, y que lo no verificado se declara como tal («no consta»).

## Por qué

Tras A8a–A8d y A12b queda un puñado de cabos sueltos que impiden dar por terminada la parte de
agentes: un remoto que pide datos se queda sin respuesta, las tarjetas remotas solo se descargan al
arrancar, y los remotos con OAuth2, OpenID Connect o mTLS no se pueden usar.

## Parte 1. `input-required` de un remoto

**Hoy.** El espejo refleja el estado `input-required` y el mensaje del remoto, pero nadie contesta: la
tarea espera hasta agotar el seguimiento.

- **D1. Se reutiliza el buzón de preguntas de los hijos locales** (`InputBroker`). El espejo registra
  la pregunta del remoto como una `Question` (cabecera «remoto», sin opciones) y emite
  `LiveEvent::SubagentInputRequested`; la persona usuaria o el padre la contestan por el mismo camino
  que las de un hijo local (`SubagentLauncher::answer`). No se inventa interfaz nueva.
- **D2. La respuesta continúa la misma tarea.** Se envía `SendMessage` con `taskId` (y `contextId` si
  la tarea lo trae), que es lo que la especificación A2A prevé para tareas interrumpidas. Lo que el
  remoto conteste manda: si devuelve la tarea, se sigue; si devuelve un mensaje en vez de tarea, ese
  mensaje es el resultado.
- **D3. Cotas** (decálogo): como mucho 5 rondas de pregunta por tarea remota; cada espera de respuesta
  tiene un máximo de 30 minutos, pasado el cual la tarea se cancela en el remoto y falla con motivo; el
  texto de la pregunta se acota a 2 000 bytes antes de mostrarse. El tiempo que se espera a la persona
  no cuenta contra el límite de seguimiento (`max_follow`), que se reinicia al reanudar.
- **D4. La pregunta es dato de un tercero.** Se muestra tal cual, sin interpretarla ni ejecutarla.
- **D5. Cancelación.** Cancelar el hijo o el padre mientras se espera cancela la tarea remota y
  descarta la pregunta pendiente.
- **D6. `auth-required` remoto sigue como estaba (D9 de A8d):** se refleja, no se resuelve.

## Parte 2. Tarjetas remotas vivas

**Hoy.** `refresh_remote_cards` se llama una sola vez, al arrancar.

- **D7. Refresco periódico** en segundo plano, con `[a2a] card_refresh_secs` (por defecto 3 600; `0`
  lo desactiva; mínimo efectivo 60 para no martillear a nadie). Solo si el rol consume.
- **D8. Refresco a demanda** con la orden `/refresh-agents`, que devuelve el informe (cuántas bien,
  cuáles fallaron).
- **D9. Un fallo de red no estropea lo que funciona.** Si el refresco falla y ya había una tarjeta
  buena, se conserva la última buena y el fallo se anota solo como aviso; únicamente un remoto que
  nunca se descargó queda como problema de carga. Hoy cualquier fallo sustituiría la tarjeta buena.
- **D10. Una tarjeta que cambia sigue exigiendo aprobación nueva.** La aprobación va atada a la huella
  de la tarjeta (A8a): si cambia, vuelve a «pendiente». El refresco nunca aprueba nada.
- **D11. Las tareas en curso no se tocan.** Solo las delegaciones nuevas usan la tarjeta refrescada.

## Parte 3. Seguridad A8d-2

**Hoy.** Clave de API y `Bearer`/`Basic`; OAuth2, OpenID Connect, mTLS y las firmas figuran como «no
soportado».

- **D12. OAuth2 con `client_credentials` solamente.** Es el único flujo sin persona delante, el que
  encaja con un agente que habla con otro. La tarjeta aporta `tokenUrl` y `scopes`; la credencial
  (`client_id:client_secret`) viene de la variable de entorno que nombra `a2a.credentials`, como las
  demás. Los demás flujos (código, dispositivo, implícito) se declaran no soportados.
- **D13. OpenID Connect por descubrimiento.** Se lee `openIdConnectUrl` (documento
  `.well-known/openid-configuration`), se toma `token_endpoint` y se aplica D12. Si el proveedor no
  admite `client_credentials` (`grant_types_supported` sin él), no soportado.
- **D14. El punto de token es dato de un tercero.** Se exige HTTPS, o HTTP solo hacia bucle local; no
  se siguen redirecciones; la respuesta se acota a 16 KiB; el `access_token` se trata como secreto (no
  aparece en registros, errores ni `Debug`); `token_type` ha de ser `Bearer`.
- **D15. Caché del token** en memoria, hasta `expires_in` menos 30 s (mínimo 30 s, máximo 1 h); un 401
  del remoto invalida la caché y se reintenta una sola vez. Un fallo al obtener token es un fallo de
  autenticación del remoto, nunca se envía la petición sin credencial.
- **D16. mTLS con identidad de cliente en PEM.** La ruta del fichero (certificado y clave privada) se
  da por variable de entorno nombrada en `a2a.credentials` para ese esquema; se construye un cliente
  HTTP propio del remoto con esa identidad. Sin fichero legible, el remoto no es utilizable y el
  motivo se muestra en `/agents`. Si el cliente propio no se puede construir, jamás se cae a hablarle
  sin identidad. El fichero se acota a 64 KiB.
- **D16b. CA privada (decisión nueva, no estaba en el plan inicial).** `[a2a] ca_file` apunta a un PEM
  con autoridades de certificación adicionales en las que confiar; se suman a las raíces públicas, no
  las sustituyen. Un remoto con mTLS suele vivir tras una CA privada, y sin esto no habría forma de
  alcanzarlo. La descarga de la tarjeta, que también puede exigir certificado, reintenta **una sola
  vez** presentando la identidad declarada ante un fallo de red.
- **D17. Firmas de la tarjeta (JWS + JCS), verificación opcional contra claves de confianza.** Si
  `[a2a] trusted_jwks` (ruta a un JWKS local; **global**, no por perfil: así se decidió al implementar)
  está definido, **toda** tarjeta remota ha de traer una firma válida con una de esas claves o el
  remoto no es utilizable, y el motivo se muestra. Sin `trusted_jwks`, la tarjeta firmada se trata como
  la no firmada. Nunca se descarga una clave de la URL que la propia tarjeta indica (`jku`): la
  confianza no puede salir del documento que se verifica. Un JWKS inutilizable falla cerrado.
  - Algoritmos: `ES256`, `ES384`, `RS256` y `EdDSA`, y el de la firma ha de ser el del material de la
    clave con ese `kid`. `none`, los simétricos, `crit` y `b64` se rechazan.
  - Carga firmada: la tarjeta sin `signatures`, sin cadenas, listas ni objetos vacíos (como hace el
    SDK oficial de Python), canonizada con RFC 8785. Los números no enteros se rechazan por no tener
    aquí forma canónica comprobada.
  - El *hash* de aprobación **excluye** `signatures`: una firma ECDSA cambia en cada emisión y, con el
    refresco periódico, invalidaría la aprobación en cada pasada.
- **D18. Combinaciones.** Una alternativa de seguridad que combine esquemas se cumple si se cumplen
  todos; el plan de autenticación sigue eligiendo la primera alternativa viable.

## Fuera de alcance, declarado

- Flujos OAuth2 con persona (código con PKCE, dispositivo): requieren interacción y almacén de
  refresco.
- Renovación de credenciales más allá de lo dicho en D15, y rotación de la identidad mTLS sin reiniciar.
- Autenticación con cookie; tarjeta extendida autenticada.
- TLS propio al servir y varios principales al servir (lado servidor).

## Verificación prevista y límites

Pruebas con `wiremock` para todo el cable; ida y vuelta contra el servidor propio para
`input-required`; prueba viva contra los servidores del SDK de Python y de JavaScript para
`input-required` y OAuth2 (el SDK trae ejemplos de ambos). No consta que haya un proveedor OAuth2/OIDC
real de terceros alcanzable sin cuenta: si no se prueba contra uno, se dirá.

## Preguntas abiertas (resueltas con la opción recomendada)

1. ¿Basta `client_credentials` para OAuth2? (Recomendado: sí; lo demás exige persona.)
2. ¿Firmas solo con claves locales de confianza? (Recomendado: sí; D17.)
3. ¿Refresco por defecto cada hora? (Recomendado: sí.)

## Estado

Implementado el 2026-10-24 en `master`. Verificado:

- `input-required` del remoto: pruebas con simulado y **prueba viva** contra el servidor del SDK oficial
  de Python (la respuesta viaja con el mismo `taskId` y `contextId` y la tarea se completa).
- Tarjetas vivas: refresco periódico, `/refresh-agents`, última tarjeta buena conservada ante fallo.
- OAuth2 `client_credentials` y OpenID Connect: pruebas con `wiremock`; **prueba viva de OAuth2**
  contra el SDK de Python con un `/token` propio (token pedido solo, secreto equivocado rechazado).
- mTLS: **prueba viva** contra el SDK de Python con `CERT_REQUIRED` (sin identidad el apretón falla; con
  ella la tarea se completa).
- Firmas: **interoperabilidad** con una tarjeta firmada por el SDK de Python en ES256, RS256 y EdDSA.

**No verificado** (se dice tal cual): ningún proveedor OAuth2/OIDC real de terceros (no consta uno
abierto sin cuenta), ni OIDC con descubrimiento contra un proveedor real; mTLS y firmas contra un remoto
desplegado de un tercero; el SDK de JavaScript para estas partes; `/refresh-agents` y la línea de
preguntas del remoto en una terminal real.

**Límites conocidos:** la identidad mTLS no se rota en caliente. La firma no cubre lo que el SDK
serializa vacío (p. ej. un `securityRequirements` con ámbitos vacíos), porque la spec manda quitar los
valores por defecto antes de canonizar. Una tarjeta con valores por defecto no vacíos (`false` de un
campo no opcional) que otra implementación haya firmado sin ellos no verifica y queda inutilizable
(falla cerrado, con el motivo).
