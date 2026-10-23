# A2A en CodeLoom: índice y tareas pendientes

Estado: documento de referencia del 2026-10-22. No es un plan de ejecución para
agentes: cada tarea que se emprenda necesita antes su spec y su plan
(`docs/specs/`, `docs/plans/`), según el flujo habitual.

Complementario, pero deliberadamente separado, de
`2026-10-22-codeloom-jevalan-roadmap.md` (decisiones locales con Jev, Kev y Laya).

## Premisa

Cabe señalar que el modelo de agentes de CodeLoom parte del protocolo A2A y no se
inventa nada propio donde el protocolo ya dice algo. La fuente normativa es
`https://a2a-protocol.org/latest/definitions/` (proto `lf.a2a.v1`). Lo que no conste
allí se rotula como decisión nuestra, y ante la duda se cita el texto literal o se
dice que no consta.

## Dónde está cada cosa

| Tema | Documento |
|---|---|
| Diseño general de A2A (profile, scope, Agent Card, transportes, crate opcional `codeloom-a2a`, SDK `a2a-lf`) | `docs/specs/2026-10-15-loom-agent-profile-a2a.md` |
| Estados de tarea: definiciones literales del proto y decisiones nuestras | misma spec, secciones «Estado de tarea» y «Fuente normativa de los estados» |
| Proyección interna → `TaskState` (código) | `crates/codeloom-core/src/task.rs` |
| Estados en pantalla: `AgentState`, glifos, colores | `crates/batten/src/state/agent_state.rs`, `crates/batten/src/widgets/theme.rs`; rail en `crates/codeloom-tui/src/rail.rs`; demo «Agent state» del showcase |
| `input-required`: pregunta del hijo al padre | `docs/specs/2026-10-21-codeloom-subagent-input-required-design.md` |
| Autonomía: despertar del padre (Fase A), artefactos (B), tareas enlazadas (C) | `docs/specs/2026-10-21-codeloom-agent-autonomy-design.md` |
| Observabilidad: métricas por agente, navegación, causalidad y ficheros (O1 a O3) | `docs/specs/2026-10-21-codeloom-agent-observability-design.md`, `...-o2-design.md`, `2026-10-22-codeloom-agent-observability-o3-design.md` |
| Herramientas de tarea y preguntas | `docs/specs/2026-10-20-codeloom-todos-and-questions-design.md`, `docs/specs/2026-10-21-codeloom-tool-atoms-design.md` |
| Mapa general | `docs/specs/2026-10-14-loom-architecture-overview.md` |

## Qué es de A2A y qué es nuestro

De A2A (según el proto): los ocho estados de tarea más `UNSPECIFIED`; terminales
(`completed`, `failed`, `canceled`, `rejected`); interrumpidos (`input-required`,
`auth-required`); `TaskStatusUpdateEvent` emitido por el agente; `CancelTask` pedido
por el cliente.

Nuestro, no del protocolo: `Unfinished → Failed`, `Stopping → Working`, los glifos y
colores, las etiquetas en `kebab-case` de la v0.3 como texto de pantalla, la línea de
causa del despertar del padre y los ficheros por hijo.

Lo que la especificación **no** dice, según lo leído: quién transiciona a cada estado,
la tabla de transiciones, que `submitted` signifique «aún no iniciada», ni que
`auth-required` exija transporte remoto.

## Cambio de enfoque (2026-10-22)

Por petición del usuario, el estado de un subagente pasa a ser **directamente** el
`TaskState` de A2A, sin enum propio ni proyección (la capa visual sí puede mostrarlo de
forma distinta). Spec: `docs/specs/2026-10-22-codeloom-a2a-task-model-design.md`; plan:
`docs/plans/2026-10-22-codeloom-a2a-task-model.md`. **A1 y A2 quedan absorbidas por ese
plan**: el renombrado a `Unfinished` ya hecho desaparece con la variante, y los
productores se implementan sobre el modelo nuevo.

## Tareas

Orden sugerido: A1, A2 y A3 (cierran lo ya empezado), luego A4 a A6 (tipos) y solo
después el transporte (A7, A8).

- [x] **A1. Renombrar `SubagentStatus::Interrupted`.** Su nombre coincidía con el
  «interrupted state» de A2A (`input-required` y `auth-required`), que significa otra
  cosa. Hecho el 2026-10-22: nuevo nombre `SubagentStatus::Unfinished`, cadena en SQLite
  `unfinished` con lectura de la cadena antigua `interrupted` para no perder sesiones.
  Sigue proyectándose a `Failed`, decisión nuestra ya rotulada.
  Absorbida por el plan `2026-10-22-codeloom-a2a-task-model.md` (el renombrado desaparece
  con la variante `Unfinished`).
- [x] **A2. Productores de los estados que faltan.** Hecho el 2026-10-22, absorbido por el
  plan `2026-10-22-codeloom-a2a-task-model.md` (commit `6365c58`). El runtime produce los
  ocho estados de tarea. Lo implementado y las decisiones tomadas (que A2A no fija) figuran
  en `docs/specs/2026-10-22-codeloom-a2a-states-producers-design.md`, sección «Estado».
  Cuidado de siempre: «Agents MUST NOT treat the `TASK_STATE_AUTH_REQUIRED` state transition,
  by itself, as authorization».
  - **Resuelto en A5 (2026-10-22):** `result` y el mensaje de estado ya son `Artifact` y
    `Message`, según la spec de tipos. Historial: hasta A5 eran texto (divergencia temporal
    declarada). Sigue abierto: no hay `contextId` compartido; `reference_task_ids` lo cerró A11.
  - **Abierto (no consta en A2A):** qué pasa con `input-required` o `auth-required` si el
    usuario cancela al hijo (hoy pasa a `canceled`; ver A10).
- [x] **A3. Leer lo que falta de la especificación** (completa el 2026-10-22: ya se ha
  leído todo). Leído en literal: el `a2a.proto` completo y, de la especificación, las
  secciones 3.1, 3.3, 3.4, 7.6, 4.4, 8, 9 a 11, 12 y 13, con los apartados de los que
  dependen. Qué consta y qué no queda anotado en `docs/specs/2026-10-15-loom-agent-profile-a2a.md`,
  sección «Lectura de la especificación: descubrimiento, bindings y seguridad (2026-10-22)».
  Las dudas que siguen abiertas están en esa misma sección.
- [x] **A4. Adaptador de nombres del cable.** Hecho el 2026-10-23 solo para la v1.0, en el
  crate `codeloom-a2a` (módulo `wire`): enumerados, `Part`, `Message`, `Artifact`, `TaskStatus` y
  `Task`. La v0.3 queda para A8. Spec `2026-10-23-codeloom-a2a-wire-names-design.md`. Texto
  original: v0.3 en `kebab-case`, v1.0 en `TASK_STATE_*`. Vive en el borde (crate del
  adaptador), nunca en el núcleo.
- [x] **A5. Tipos del protocolo en el núcleo.** `Task`, `Message`, `Part`, `Artifact`,
  `TaskStatus`, con `taskId` y `contextId`. Alinear con la Fase B de la autonomía
  (artefactos) en lugar de inventar un tipo propio.
  - **Hecho el 2026-10-22** en los commits `ad2952a` (tipos `a2a`: `Role`, `Part`, `Message`,
    `Artifact`) y `3ea5ee2` (`TaskStatus.message` y `SubagentDescriptor.artifacts`, con la
    migración de lectura en SQLite). Spec: `docs/specs/2026-10-22-codeloom-a2a-protocol-types-design.md`,
    sección «Estado». Plan: `docs/plans/2026-10-22-codeloom-a2a-protocol-types.md`.
  - **Abierto:** no existe un tipo `Task` en el núcleo, así que `history` y `metadata` no se
    modelan. `contextId` solo se deriva en lectura (id de la sesión del padre), sin
    `contextId` compartido entre tareas. `reference_task_ids` lo escribe el descriptor del
    subagente desde A11 (2026-10-23). La serialización JSON es interna y la del cable queda para A4. La validación
    visual en terminal real está pendiente.
- [x] **A6. Tarjeta del agente (`AgentCard`)** y capacidades declaradas.
  - **Hecho el 2026-10-22** en el commit `77b8de1` (`feat(core): los tipos de la tarjeta de agente de A2A`),
    módulo `crates/codeloom-core/src/card.rs`. Spec y plan en `cf82838`. Spec:
    `docs/specs/2026-10-22-codeloom-a2a-agent-card-design.md`, sección «Estado». Plan:
    `docs/plans/2026-10-22-codeloom-a2a-agent-card.md`.
  - **Fuera de A6:** seguridad, firmas e icono de la tarjeta, que quedan para A8.
- [ ] **A7. Transporte `InProcess`** detrás de `AgentTransport`: mensajes en memoria,
  sin socket, para los tests.
- [ ] **A8. Transporte `A2aRemote`** (HTTP JSON-RPC y SSE; gRPC opcional) con roles
  `serve | consume | both`, por defecto desactivados, en el crate opcional
  `codeloom-a2a`. Aquí cobra sentido `auth-required`.
  - Seguridad, firmas e icono de la tarjeta (`security_schemes`, `security_requirements`,
    `signatures`, `icon_url`) quedan para esta tarea. A6 no los modela; ver la spec de la
    tarjeta, sección «Estado».
- [ ] **A9. Flujo de eventos** `TaskStatusUpdateEvent` y de artefactos sobre el bus de
  eventos existente, sin canal de interfaz paralelo.
- [x] **A10. Cancelar y reanudar.** **Hecho el 2026-10-23** (decisiones nuestras, rotuladas):
  spec `docs/specs/2026-10-23-codeloom-a2a-cancel-design.md`, plan
  `docs/plans/2026-10-23-codeloom-a2a-cancel.md`. `SubagentLauncher::cancel` devuelve la tarea
  actualizada y falla con `TaskNotFound` / `TaskNotCancelable`. Sin mando en la interfaz ni cable.
  Texto original de la tarea: `CancelTask` lo solicita el cliente y devuelve la
  `Task` actualizada; no consta a qué estado pasa (la inferencia `canceled` ha de
  rotularse) ni si `input-required` es cancelable. Reanudar tras `input-required` con
  un mensaje con el mismo `taskId` y `contextId` (consta literal).
- [x] **A11. Tareas enlazadas** (`follow_up_of`, Fase C de la autonomía). Hecho el
  2026-10-23: la herramienta `subagent` acepta `follow_up_of`, el hijo nuevo recibe el resultado
  de la tarea terminal referida (acotado) y el enlace queda en `reference_task_ids`. Sin pintar
  en la interfaz ni cable. Spec `2026-10-23-codeloom-a2a-follow-up-design.md`. Texto original:
  Según la
  spec, una tarea terminal no se reinicia: el seguimiento es una **tarea nueva** en el
  mismo `contextId`, con `reference_task_ids`. El nombre `follow_up_of` es nuestro;
  valorar si debe alinearse con `reference_task_ids`. Los ids los genera el servidor.
- [x] **A12. Delegar por perfil, con modelo y esfuerzo (idea del 2026-10-22).** Elegir
  al especialista según la tarea (front, Rust, etc.) a partir de las `skills[]` con
  `tags` de su tarjeta (A6) y de su `AgentProfile`. **A2A no fija cómo se elige** al
  agente: es decisión nuestra y se rotula como propia. Opciones: que decida el modelo
  padre viendo las tarjetas, regla determinista por etiquetas, o un enrutador
  (`model.route` de la pista Jevalan, con la guarda «regla antes que clasificador»).
  Debe quedar visible qué especialista se eligió y por qué (observabilidad).
  **Modelo y esfuerzo:** en la especificación (`spec.md` y `a2a.proto`) **no consta**
  ningún campo de modelo ni de esfuerzo de razonamiento; `SendMessageConfiguration` solo
  trae `accepted_output_modes`, la configuración de notificaciones push, `history_length`
  y `return_immediately`. Para un subagente local es un parámetro nuestro del lanzador,
  no del protocolo; para uno remoto, el agente es opaco y solo cabría una extensión
  declarada en su tarjeta o `metadata` libre, ambas cosas propias y sin garantía de que
  el otro lado las atienda. Requiere su propia spec antes de implementar.
    - **Hecho (2026-10-22):** el padre ve la lista de agentes en su prompt y, según
      `[agents] confirm_delegation` (`doubt` por defecto, `always`, `never`), pregunta con
      `ask_user` cuando duda entre varios; la interfaz muestra siempre el agente de cada
      hijo. Commits `871576e` (spec), `9631385` (plan), `7cd4aab`, `1c12360` y `c71f53e`.
      Spec: `docs/specs/2026-10-22-codeloom-a2a-delegation-choice-design.md`, sección «Estado».
    - **A12b, pendiente:** clasificador o enrutador automático (idea del usuario,
      «interesante más adelante»). El hueco es `regla_de_eleccion` en `registry/mod.rs`.
- [x] **Árbol de subagentes (2026-10-22).** El subagente es un nodo plegable de la conversación,
  con el icono del estado A2A y su nombre exacto; desplegado, muestra su texto y sus herramientas
  y las consultas del padre sobre él. Commits `a059b49`, `1bf6ec6`, `3d4ee31`, `7cdce9b`,
  `865539e`, `193e141`. Spec: `docs/specs/2026-10-22-codeloom-subagent-tree-design.md`.
- [x] **A13. Registro de agentes con fuentes intercambiables (prerrequisito de A12).**
  Replanteada el 2026-10-22 por el usuario: la fuente de agentes **no debe ser solo un
  fichero**. El denominador común es la **tarjeta** (`AgentCard`, A6): quien delega
  consulta un registro de tarjetas y no sabe, ni le importa, de dónde vienen. Una fuente
  es cualquier cosa que produzca tarjetas junto con la forma de hablar con el agente.
  - **Fuente de ficheros (primera):** los agentes propios, un `.md` por agente (formato de
    la sección 2 de `docs/specs/2026-10-15-loom-agent-profile-a2a.md`), editable a mano o
    desde la interfaz. De cada perfil se sintetiza su tarjeta local.
  - **Fuente remota (con A8):** agentes ya desplegados, cuya tarjeta se obtiene por HTTP
    (`/.well-known/agent-card.json`, §8.1-8.2). La spec del perfil ya lo prevé: un perfil
    con `transport: a2a-remote` y `a2a.endpoint` es un **puntero** a un agente remoto, de
    modo que también se pueden registrar remotos desde un fichero.
  - **Diferencias que el registro no puede ocultar:** un agente remoto no admite nuestro
    `scope` (allowlist) ni nuestros `model`/`effort` (A2A no los contempla, ver A12), y su
    tarjeta es **dato de un tercero, no instrucción**: sus textos (`description`, `skills`)
    pueden intentar manipular al modelo padre, así que se muestran como datos, no se
    registran sin que el usuario lo apruebe y no hay descubrimiento automático.
  - **Ejecución:** la elige el transporte (`InProcess`, A7; `A2aRemote`, A8), no la
    fuente.
  - **Decisiones del usuario (2026-10-22).** Ubicación: global en `~/.agents/agents/` y
    por proyecto en una carpeta del repo, configurables ambas; el proyecto prevalece sobre
    lo global si coinciden (precedencia nuestra). A2A **no** recomienda carpeta alguna:
    su §8.2 solo da tres vías de descubrimiento («Well-Known URI», «Registries/Catalogs»
    y «Direct Configuration: Pre-configured Agent Card URLs or content»); nuestros
    ficheros son la tercera y los remotos la primera. Recarga en caliente: sí, desde el
    principio.
  - **Tres niveles de agente** (modelo y esfuerzo son configuración de **ejecución local**,
    no parte del protocolo ni de la tarjeta, y no viajan en ella):
    1. **Default:** agente genérico local, para cuando no hay candidato o el modelo padre
       no sabe cuál elegir; hereda modelo y esfuerzo del padre. La caída al default debe
       ser visible y razonada en la interfaz.
    2. **Propios (ficheros):** mandan el `model` y el `effort` del perfil; si faltan,
       heredan del padre.
    3. **Externos (remotos):** no se pueden enviar modelo ni esfuerzo; la interfaz indica
       que los decide el agente remoto, y un `model` en un perfil remoto se ignora con
       aviso.
  - **Hecho el 2026-10-22** en los commits `1f4787a`, `73a4e41`, `a492f73`, `128cfcd`,
    `89329b7`, `f3f0240`, `27dd8e4` y `05dc0ac`. Módulo `crates/codeloom-core/src/registry/`.
    Spec: `docs/specs/2026-10-22-codeloom-a2a-agent-registry-design.md`, sección «Estado
    (2026-10-22)». Plan: `docs/plans/2026-10-22-codeloom-a2a-agent-registry.md`.
  - **Abierto:**
    - La elección del agente la resuelve A12: el padre pregunta cuando duda (regla del prompt).
      La elección automática, sin preguntar, queda para A12b.
    - La aprobación ya se hace desde el popup de `/agents` (`a`/`r`); quedan sin detallar
      los problemas de carga y el popup no se refresca con la recarga en caliente.
    - `AgentRegistryChanged` se emite, pero la TUI no lo muestra.
    - Sin fuente remota (A8) ni edición desde la TUI.
    - `scope`, `delegation`, `transport` y `a2a` se leen y no se aplican; `max_depth` tampoco.
    - `skills[]` de la tarjeta sintetizada va vacío.
    - La recarga por sondeo no detecta un cambio con misma fecha y tamaño.
    - Validación visual en terminal real: pendiente.
    - Lista completa de limitaciones en la spec del registro, sección «Estado (2026-10-22)».

## Cruces con otras líneas

- Con la pista de decisiones locales: `rejected` y `tool.risk`, y el acuse del hijo
  frente a `agent.wakeup`. Se detallan en el documento de Jevalan.
- A5 (artefactos) condiciona la Fase B de la autonomía.
