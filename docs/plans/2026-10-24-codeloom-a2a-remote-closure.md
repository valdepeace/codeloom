# Cierre de los agentes remotos: plan de implementación

Spec: `docs/specs/2026-10-24-codeloom-a2a-remote-closure-design.md` (D1–D18). Cabe señalar que cada
tarea termina con las cuatro comprobaciones del proyecto (`cargo build`, `cargo test`,
`cargo fmt --check`, `cargo clippy -- -D warnings`) y con un commit.

## Restricciones globales

- `codeloom-core` no depende de `ratatui`, `crossterm` ni de HTTP; todo lo de cable va en `codeloom-a2a`.
- Nada sin cota llega al modelo ni a la pantalla (decálogo); sin red real en pruebas (`wiremock`).
- Un secreto nunca aparece en `Debug`, errores ni registros.
- No se tocan `codeloom.toml` ni los directorios hermanos de `E:\workspace\personal\`.

## Revisión de foco (entradas que la spec implica y ninguna tarea cubriría de oficio)

- Una pregunta remota vacía, enorme o con saltos de control: se acota y se limpia antes de mostrarse.
- Dos `input-required` seguidos y una respuesta tardía tras cancelar: ni se pierde la cancelación ni
  se envía a una tarea ya cerrada.
- Un refresco que llega mientras se delega: la delegación usa una tarjeta coherente (la anterior o la
  nueva, nunca una mezcla).
- Un `tokenUrl` que apunta a otro host, a `http://` o redirige: se rechaza.
- Un `expires_in` absurdo (0, negativo, enorme, no numérico): se acota.

## Parte 1: `input-required` de un remoto

### Tarea 1: preguntar al usuario y reanudar la tarea remota

**Ficheros:** `crates/codeloom-core/src/subagent.rs`.

**Interfaces:** consume `InputBroker::request_with`, `LiveEvent::SubagentInputRequested`,
`RemoteTaskClient::send` con `task_id`. Produce que `EjecucionRemota` lleve `broker`, `bus` y `label`.

- [ ] Pruebas con el cliente guionizado: `a_remote_input_required_is_asked_and_the_answer_resumes_the_task`
  (la respuesta llega al remoto como `SendMessage` con `taskId`; la tarea acaba `Completed`),
  `the_remote_question_is_capped_and_cleaned`, `a_cancel_while_waiting_cancels_the_remote_task`,
  `too_many_question_rounds_fail_the_task`, `an_unanswered_question_times_out_and_cancels`,
  `the_wait_for_the_user_does_not_count_against_the_follow_limit`, `a_message_reply_to_the_answer_is_the_result`.
- [ ] Implementar `preguntar_al_usuario` en el espejo y el bucle de rondas en `seguir_remoto`
  (D1–D5); constantes `REMOTE_MAX_RONDAS_ENTRADA = 5`, `REMOTE_ESPERA_RESPUESTA = 30 min`,
  `REMOTE_MAX_BYTES_PREGUNTA = 2000`. Mutación a mano: ignorar el `task_id` al responder.
- [ ] Commitear.

## Parte 2: tarjetas vivas

### Tarea 2: conservar la última tarjeta buena y refrescar

**Ficheros:** `crates/codeloom-a2a/src/{config,sync}.rs`, `crates/codeloom-core/src/registry/mod.rs`,
`crates/codeloom-cli/src/main.rs`, orden `/refresh-agents` en `codeloom-tui`.

**Interfaces:** `A2aConfig::card_refresh_secs: u64`; `refresh_remote_cards` conserva la última buena
(D9); `arrancar_remotos` lanza el bucle periódico (D7) y expone el refresco a demanda (D8).

- [ ] Pruebas: `a_failed_refresh_keeps_the_last_good_card`, `a_never_downloaded_remote_still_reports_the_problem`,
  `a_changed_card_goes_back_to_pending_approval`, `refresh_secs_zero_disables_the_loop`,
  `the_interval_has_a_floor`, `the_command_reports_ok_and_failed`.
- [ ] Implementar y commitear.

## Parte 3: seguridad A8d-2

### Tarea 3: OAuth2 `client_credentials` y caché del token

**Ficheros:** `crates/codeloom-a2a/src/security.rs` (+ `oauth.rs` nuevo), `client.rs`.

**Interfaces:** `SchemeKind::OAuth2 { token_url, scopes }`; `AuthPlan::apply` pasa a ser `async` y
`Paso::Token(Arc<TokenSource>)` obtiene y cachea el token (D12, D14, D15).

- [ ] Pruebas con `wiremock`: token pedido con Basic y `grant_type=client_credentials`, cacheado hasta
  caducar, renovado tras caducar, invalidado por un 401 con un solo reintento, `token_type` distinto de
  Bearer rechazado, respuesta enorme rechazada, `tokenUrl` http no local rechazado, redirección no
  seguida, `expires_in` absurdo acotado, el secreto no sale en `Debug` ni en errores.
- [ ] Implementar y commitear.

### Tarea 4: OpenID Connect por descubrimiento

**Ficheros:** `crates/codeloom-a2a/src/{security,oauth}.rs`.

- [ ] Pruebas: descubrimiento correcto, documento sin `token_endpoint`, `grant_types_supported` sin
  `client_credentials`, endpoint no HTTPS, documento enorme.
- [ ] Implementar (D13) y commitear.

### Tarea 5: mTLS

**Ficheros:** `crates/codeloom-a2a/src/{security,remote,client}.rs`.

- [ ] Pruebas: identidad PEM válida construye cliente; fichero ausente o ilegible da un motivo legible en
  el aviso; la clave privada no sale en `Debug`.
- [ ] Implementar (D16) y commitear. Si `reqwest` no ofrece identidad con la pila TLS actual, se declara.

### Tarea 6: firmas de la tarjeta con claves de confianza locales

**Ficheros:** `crates/codeloom-a2a/src/{signature,card,sync}.rs`, perfiles `a2a.trusted_jwks`.

- [ ] Pruebas: firma válida acepta; firma alterada, clave desconocida, sin firma con claves
  configuradas y algoritmo `none` rechazan; `jku` ignorado; sin claves configuradas no cambia nada.
- [ ] Implementar (D17) y commitear.

### Tarea 7: documentos y verificación viva

- [ ] «Estado» de la spec; roadmap (A8d-2 hecha o lo que conste); README; `AGENTS.md`. Pruebas vivas
  contra los SDK de Python y JavaScript donde apliquen. Lo no verificado se declara.
