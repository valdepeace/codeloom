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

Nuestro, no del protocolo: `Interrupted → Failed`, `Stopping → Working`, los glifos y
colores, las etiquetas en `kebab-case` de la v0.3 como texto de pantalla, la línea de
causa del despertar del padre y los ficheros por hijo.

Lo que la especificación **no** dice, según lo leído: quién transiciona a cada estado,
la tabla de transiciones, que `submitted` signifique «aún no iniciada», ni que
`auth-required` exija transporte remoto.

## Tareas

Orden sugerido: A1, A2 y A3 (cierran lo ya empezado), luego A4 a A6 (tipos) y solo
después el transporte (A7, A8).

- [ ] **A1. Renombrar `SubagentStatus::Interrupted`.** Su nombre coincide con el
  «interrupted state» de A2A (`input-required` y `auth-required`), que significa otra
  cosa. Decidir el nombre, tocar el enum, la cadena en SQLite (con lectura de la
  cadena antigua `interrupted` para no perder sesiones) y la proyección. Seguirá
  proyectándose a `Failed`, decisión nuestra ya rotulada.
- [ ] **A2. Productores de los estados que faltan.** Hoy el runtime produce `working`,
  `input-required`, `completed`, `failed` y `canceled`. Faltan:
  - `submitted` («successfully submitted and acknowledged»): decidir el instante del
    acuse en el lanzador del hijo; la spec no lo fija.
  - `rejected` («the agent has decided to not perform the task», al crear la tarea o
    después): decidir qué rechazos son del agente (límites, perfil desconocido,
    permiso denegado) frente a un fallo de ejecución.
  - `auth-required`: la spec (§7.6) cita como ejemplo «requiring human approval
    before a destructive action is taken». Hoy el permiso de un hijo se delega al
    padre sin cambiar su estado (`ChildResponder::decide`); candidato a productor.
    Cuidado: «Agents MUST NOT treat the `TASK_STATE_AUTH_REQUIRED` state transition,
    by itself, as authorization».
  Requiere variantes nuevas de `SubagentStatus` y su cadena en SQLite (la columna es
  `TEXT NOT NULL`, sin cambio de esquema), y actualizar la columna «productor» del
  showcase.
- [x] **A3. Leer lo que falta de la especificación** (hecho el 2026-10-22, parcial).
  Leído en literal: el `a2a.proto` completo y, de la especificación, las secciones 3.1,
  3.3, 3.4 y 7.6. Anotado en la spec A2A qué consta y qué no. **Quedan por leer en
  literal** §4.4, §8.4, §9 a §11, §12 y §13 (se hará al abordar A4 y A8; basta
  descargar el fichero en crudo, porque la herramienta web solo da literal los primeros
  ~39 000 caracteres).
- [ ] **A4. Adaptador de nombres del cable.** v0.3 en `kebab-case`, v1.0 en
  `TASK_STATE_*`. Vive en el borde (crate del adaptador), nunca en el núcleo.
- [ ] **A5. Tipos del protocolo en el núcleo.** `Task`, `Message`, `Part`, `Artifact`,
  `TaskStatus`, con `taskId` y `contextId`. Alinear con la Fase B de la autonomía
  (artefactos) en lugar de inventar un tipo propio.
- [ ] **A6. Tarjeta del agente (`AgentCard`)** y capacidades declaradas.
- [ ] **A7. Transporte `InProcess`** detrás de `AgentTransport`: mensajes en memoria,
  sin socket, para los tests.
- [ ] **A8. Transporte `A2aRemote`** (HTTP JSON-RPC y SSE; gRPC opcional) con roles
  `serve | consume | both`, por defecto desactivados, en el crate opcional
  `codeloom-a2a`. Aquí cobra sentido `auth-required`.
- [ ] **A9. Flujo de eventos** `TaskStatusUpdateEvent` y de artefactos sobre el bus de
  eventos existente, sin canal de interfaz paralelo.
- [ ] **A10. Cancelar y reanudar.** `CancelTask` lo solicita el cliente y devuelve la
  `Task` actualizada; no consta a qué estado pasa (la inferencia `canceled` ha de
  rotularse) ni si `input-required` es cancelable. Reanudar tras `input-required` con
  un mensaje con el mismo `taskId` y `contextId` (consta literal).
- [ ] **A11. Tareas enlazadas** (`follow_up_of`, Fase C de la autonomía). Según la
  spec, una tarea terminal no se reinicia: el seguimiento es una **tarea nueva** en el
  mismo `contextId`, con `reference_task_ids`. El nombre `follow_up_of` es nuestro;
  valorar si debe alinearse con `reference_task_ids`. Los ids los genera el servidor.

## Cruces con otras líneas

- Con la pista de decisiones locales: `rejected` y `tool.risk`, y el acuse del hijo
  frente a `agent.wakeup`. Se detallan en el documento de Jevalan.
- A5 (artefactos) condiciona la Fase B de la autonomía.
