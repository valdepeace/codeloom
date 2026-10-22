# Hoja de ruta: A2A como premisa y decisiones locales (Jev, Kev, Laya)

Estado: lista de tareas del 2026-10-22 para retomar el trabajo. No es un plan de
ejecución para agentes: cada tarea que se emprenda necesita antes su spec y su plan
(`docs/specs/`, `docs/plans/`), según el flujo habitual.

## Premisa

Cabe señalar que el modelo de agentes de CodeLoom parte del protocolo A2A y no se
inventa nada propio donde el protocolo ya dice algo. La fuente normativa es
`https://a2a-protocol.org/latest/definitions/` (proto `lf.a2a.v1`). Lo que no conste
allí se rotula como decisión nuestra (véase «Fuente normativa de los estados» en
`docs/specs/2026-10-15-loom-agent-profile-a2a.md`).

Sobre las decisiones locales: `jevalan` es el hub; **Jev** (nube), **Kev-4B/0.8B**
(GPU, puerto 8009) y **Laya** (CPU, puerto 8021) sirven el mismo contrato
`POST /v1/systemone` y devuelven probabilidades tipadas, no texto. Fuente en este
repositorio: `docs/specs/2026-10-14-loom-decision-hub-jevalan.md` (**idea en
exploración, «no implementar»**). El proyecto `jevalan` en sí **no se ha leído** al
redactar esta lista: la primera tarea de la pista J es leerlo.

## Pista A — A2A

Orden sugerido: A1, A2, A3 (cierran lo ya empezado), luego A4 a A6 (tipos), y solo
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
  - `auth-required`: sin productor hasta que haya algo que autenticar. No asumir que
    exige transporte remoto: la spec no lo dice.
  Requiere variantes nuevas de `SubagentStatus` y su cadena en SQLite (la columna es
  `TEXT NOT NULL`, sin cambio de esquema), y actualizar la columna «productor» del
  showcase.
- [ ] **A3. Leer lo que falta de la especificación.** No se leyó la sección 4.1.3 de la
  página principal, ni las 3.x y 6.x íntegras, ni el `a2a.proto` completo, ni los
  bindings (§9–§13), ni el esquema JSON. Resumir con citas literales y anotar en la
  spec A2A qué consta y qué no (quién transiciona, transiciones permitidas, uso de
  `UNSPECIFIED`).
- [ ] **A4. Adaptador de nombres del cable.** v0.3 en `kebab-case`, v1.0 en
  `TASK_STATE_*`. Vive en el borde (crate del adaptador), nunca en el núcleo.
- [ ] **A5. Tipos del protocolo en el núcleo.** `Task`, `Message`, `Part`, `Artifact`,
  `TaskStatus`, con los identificadores `taskId` y `contextId`. Alinear con la Fase B
  de la spec de autonomía (artefactos) en lugar de inventar un tipo propio.
- [ ] **A6. Tarjeta del agente (`AgentCard`)** y capacidades declaradas.
- [ ] **A7. Transporte `InProcess`** detrás de `AgentTransport`: mensajes en memoria,
  sin socket, para los tests.
- [ ] **A8. Transporte `A2aRemote`** (HTTP JSON-RPC y SSE; gRPC opcional) con roles
  `serve | consume | both`, por defecto desactivados. Aquí cobra sentido
  `auth-required`.
- [ ] **A9. Flujo de eventos** `TaskStatusUpdateEvent` y de artefactos sobre el bus de
  eventos existente, sin canal de interfaz paralelo.
- [ ] **A10. Cancelar y reanudar.** `CancelTask` solicitado por el cliente; reanudar
  tras `input-required` con un nuevo mensaje del mismo `taskId` (esto último consta
  solo en un resumen de la spec: comprobarlo en A3).
- [ ] **A11. Tareas enlazadas** (`follow_up_of`, Fase C de la autonomía) sobre
  `taskId`/`contextId`.

## Pista J — Decisiones locales con Jev, Kev y Laya

Guardas de la spec que gobiernan toda la pista: clasificar solo decisiones frecuentes,
baratas, recuperables y sin regla determinista; **regla antes que clasificador**; no
autoaprobar por confianza alta; presupuestar la latencia por decisión.

- [ ] **J1. Leer `jevalan`** (`E:\workspace\personal\jevalan`, solo lectura) y los
  informes de `gauntlet-decision-lab`: contrato real de `/v1/systemone`, capacidades
  disponibles, latencias vigentes. Anotar discrepancias con la spec del hub, que data
  del 2026-10-14.
- [ ] **J2. Promover la spec del hub** de «idea, no implementar» a diseño, con
  decisiones marcadas: qué capacidades entran primero, qué ocurre sin hub, qué se
  persiste.
- [ ] **J3. Rasgo `DecisionProvider` en el núcleo** (puro, simulable, sin red en
  tests) con tipos de decisión tipados y probabilidades.
- [ ] **J4. `tool.risk` en `PermissionService`:** decisión `allow | ask | deny` antes
  de ejecutar una herramienta, con reglas fijas primero. Sin hub o con tiempo agotado
  se vuelve a `ask`; jamás a `allow`.
- [ ] **J5. Cliente HTTP del hub** en crate opcional, desactivado por defecto, con
  tiempo máximo, reintento acotado y degradación elegante.
- [ ] **J6. `agent.wakeup`:** que el despertar del padre (Fase A de la autonomía) pueda
  consultar al hub antes de gastar un turno. Aprovecha la causa ya registrada en
  `wake_causes`; añadir la fuente de la decisión a la línea del transcript.
- [ ] **J7. `model.route`:** `{ model, effort }` por turno. Se apoya en el campo
  `effort` ya existente y en los precios del catálogo; medir el ahorro antes de
  activarlo por defecto.
- [ ] **J8. Observabilidad de las decisiones:** mostrar en el rail o el transcript la
  decisión, su fuente, probabilidad y latencia, y contabilizar las llamadas grandes
  evitadas junto al coste del libro de consumo.
- [ ] **J9. Experimentos y KPIs** para validar que clasificar compensa
  (`docs/specs/2026-10-14-loom-decision-experiments.md`), antes de ampliar a
  `skill.route`, `memory.route` y demás.

## Cruces entre pistas

- J6 (despertar con hub) y A2 (productores) comparten el momento del acuse del hijo.
- J4 y A2 (`rejected`): un permiso denegado por `tool.risk` es candidato a rechazo
  del agente, pero la spec A2A no lo dice; decidirlo explícitamente.
- A5 (artefactos) condiciona la Fase B de la autonomía.

## Pendientes sueltos de las fases anteriores

- Validación visual en terminal real de todo lo reciente: rail con estados,
  coste por hijo, `Nf`, línea de causa, selectores, campo «Effort», aviso de cambios
  sin guardar.
- Contrastar con servicios reales la tabla de presupuestos de Anthropic y la forma
  `reasoning: {effort}` de OpenRouter (hoy son suposiciones).
- Glifo `⚿` de `auth-required`: comprobar que se pinta bien en las fuentes del
  usuario.
- Fila «Flock/subagentes» del showcase: la columna «Integración» puede estar
  desfasada.
- Reconstruir causa del despertar y ficheros por hijo al reabrir una sesión
  (hoy solo en memoria).
- Revisar los mensajes de commit antiguos de core y herdr (M2/M3), escritos sin
  haber leído ese código.
- Decisiones de interfaz pendientes de revisión: foco en paneles laterales, colores
  por rol de bloques, encadenado proveedor→modelo del selector.
