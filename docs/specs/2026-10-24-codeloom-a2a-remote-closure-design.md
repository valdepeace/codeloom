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
  motivo se muestra en `/agents`.
- **D17. Firmas de la tarjeta (JWS + JCS), verificación opcional contra claves de confianza.** Si el
  perfil del remoto declara `a2a.trusted_jwks` (ruta a un JWKS local), la tarjeta ha de traer una firma
  válida con una de esas claves o el remoto no es utilizable. Sin `trusted_jwks`, la tarjeta firmada se
  trata como la no firmada, y así lo dice la documentación. Nunca se descarga una clave de la URL que
  la propia tarjeta indica (`jku`): la confianza no puede salir del documento que se verifica.
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

Sin implementar.
