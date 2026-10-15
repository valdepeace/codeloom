# CodeLoom — Documento de referencia: análisis de herdr, crush y deepseek-harness

> Estado: notas de investigación. No es el spec de diseño; es la base de
> conocimiento de la que saldrá el diseño.
>
> Fecha: 2026-10-13

## 1. Propósito

CodeLoom quiere construir un agente de código en **Rust + ratatui** cuyo valor
diferencial es el modelo de orquestación de **deepseek-harness (dsh)**:
agente -> subagentes -> jobs, con una UI que muestre en el header qué
subagentes están trabajando y cuáles terminaron, más los jobs. La referencia
funcional es **crush** (comportamiento del asistente) y la referencia de stack
es **herdr** (Rust, ratatui, daemon/socket).

Este documento consolida el análisis de los tres proyectos para tenerlo como
referencia durante el diseño y la implementación.

## 2. Resumen ejecutivo

- **herdr** es un multiplexer/runtime de terminales para agentes CLI, en Rust,
  con daemon, socket API JSON, panes (PTY), detach/reattach, SSH remoto y
  plugins por manifiesto. **No tiene cliente LLM**: es el host, no el agente.
- **crush** es un asistente de código en Go (Bubble Tea). Es la referencia
  funcional completa, pero el bucle agente vive en la librería externa
  `charm.land/fantasy`, no en crush.
- **dsh** es un harness TS "todo es plugin" (Cordis). Aporta el modelo de
  subagentes y jobs, el ciclo turn/step y su representación en el front. El
  diseño es bueno; el rendimiento está limitado por el runtime de eventos de JS.
- **Conclusión**: no existe una pieza reutilizable que ya haga lo que CodeLoom
  quiere. El plan es reimplementar el modelo de dsh (subagentes + jobs + bus de
  eventos) en Rust, usando a herdr como referencia de stack/UI y a crush como
  referencia funcional.

## 3. Los tres proyectos

### 3.1 herdr (Rust)

Runtime/workspace manager para agentes de código. Un solo binario, sin
Electron. Ownsea los PTY de agentes externos (claude, codex, cursor, opencode,
grok, crush, …); no los envuelve ni los reemplaza.

- **Stack**: ratatui 0.30 + crossterm 0.29, tokio (multi-thread), serde,
  bincode, clap, interprocess, portable-pty. Sin cliente HTTP
  (`Cargo.toml:27-54`).
- **Workspace**: `.` + `crates/ghostty-vt` (emulador de terminal VT).
- **Cliente/servidor**: daemon headless (`src/server/headless.rs`), dos sockets
  locales: protocolo de cliente (render TUI) y socket API JSON
  (`src/api/socket_path()`); env override `HERDR_SOCKET_PATH`.
- **API JSON-RPC**: `Request { id, method }` con enum `Method` etiquetado
  (`src/api/schema.rs:35-120`). Métodos: `agent.start`, `agent.prompt`,
  `agent.wait`, `pane.report_agent`, `plugin.pane.open`, etc. Suscripción por
  `EventHub` (ring de 512 eventos, `src/api/event_hub.rs`).
- **CLI**: espejo de la API (`agent`, `pane`, `workspace`, `tab`, `worktree`,
  `machine`, `session`, `plugin`, `integration`, `status`, `api`).
- **Panes**: abstracción de terminal + actor PTY (`src/pane.rs`, `src/pty/`).
- **Detección de agentes**: por regex sobre el texto del pane contra manifiestos
  TOML (`src/detect/manifests/*.toml`, 25 agentes, incl. `Crush`), estados
  working/blocked/idle.
- **Integraciones**: instala hooks/settings en CLIs externos para que reporten
  estado por el socket (`src/integration/`).
- **Plugins**: por manifiesto (id/version/actions/events/panes/link_handlers),
  sin link dinámico; se lanzan comandos externos con env `HERDR_*`
  (`src/app/api/plugins/`).
- **LLM/agente propio**: **ninguno**. `grep openai` solo aparece en rutas de
  instalación de codex. No hay `reqwest`/`ureq`.

**Puntos útiles para CodeLoom**: ratatui/crossterm, tokio, patrón daemon+socket,
EventHub, capa de panes/PTY (útil para la tool `bash` y para hostear agentes),
manifiestos declarativos y persistencia/restore.

### 3.2 crush (Go)

Asistente de código, referencia funcional completa.

- **Entrypoint**: `main.go:15` -> `cmd.Execute()`. Comandos: `run`, `dirs`,
  `projects`, `logs`, `models`, `stats`, `session`, `login`, `server`,
  `schema`, `update-providers`.
- **Wiring**: `internal/app/app.go` — `App` (`app.go:55-91`) con sesiones,
  mensajes, permisos, LSP, skills, brokers pubsub. Inicializa el agente coder
  en `InitCoderAgent` (`app.go:170`).
- **Agente**: `internal/agent/agent.go` — `SessionAgent` (`:175-191`), impl
  `sessionAgent` (`:209-267`), entrada `Run` (`:612`). El bucle de pasos
  (LLM -> tools -> resultado -> LLM) **no está en crush**: delega en
  `fantasy.Agent.Stream` (`agent.go:858`) con callbacks `PrepareStep`
  (`:870-942`), callbacks de streaming, `StopWhen` (`:1110-1135`).
- **Coordinador**: `internal/agent/coordinator.go` — `Coordinator` (`:121-149`),
  `run` (`:462-742`), `buildTools` (`:1411-1535`), `buildProvider` (`:1889`).
  Agentes nombrados: `coder` y `plan` (`internal/config/config.go:61-63`).
- **Providers**: implementados por `charm.land/fantasy`; crush configura
  anthropic/openai/openrouter/vercel/openai-compat/azure/bedrock/google/vertex
  (`coordinator.go:1649-1863`) + `hyper` propio. Catálogo en
  `charm.land/catwalk`.
- **Mensajes propios**: `internal/message/` — roles assistant|user|system|tool,
  content parts `TextContent`, `ReasoningContent`, `ImageURLContent`,
  `BinaryContent`, `ToolCall`, `ToolResult`, `Finish`, `ShellCommand`
  (`content.go:51-148`). Convierte a/desde fantasy (`agent.go:1631`, `:1758`).
- **Tools**: `internal/agent/tools/*` (cada una un `.go` + `.md`). Registro y
  filtrado en `buildTools` (`coordinator.go:1411-1535`). Incluye core
  (view/edit/multiedit/write/bash/glob/grep/ls/download/fetch/todos/sourcegraph),
  LSP (diagnostics/references/symbols/definition/call_hierarchy/rename/
  replace_symbol), MCP, y sub-agentes (`agent`, `agentic_fetch`).
- **Permisos**: `internal/permission/permission.go` — `Service` (`:65-85`),
  comprobaciones y prompt a la UI; `StopTurn` si se deniega
  (`tools/tools.go:74-78`).
- **UI**: Bubble Tea v2 + lipgloss + glamour + ultraviolet. Modelo único en
  `internal/ui/model/ui.go` (`:196+`), render en `internal/ui/chat/`,
  overlays en `internal/ui/dialog/`.
- **LSP**: `internal/lsp/` (manager lazy, wrapper sobre `powernap`).
- **MCP**: `internal/agent/tools/mcp/` — stdio/sse/http, OAuth, canales.
- **Hooks**: `internal/hooks/` — eventos tipo `PreToolUse`, decisión
  deny>allow>none, ejecución en paralelo con timeouts, compatibles Claude Code.
- **Skills**: `internal/skills/` — `SKILL.md` con frontmatter YAML.
- **Persistencia**: SQLite + sqlc (`sqlc.yaml`), tablas sessions/files/messages
  y migraciones.
- **Config**: `crushrc` (script Bash con builtins) + `crush.json` (deprecado);
  descubrimiento hacia arriba, contexto `AGENTS.md`/`CRUSH.md`/`CLAUDE.md`.
- **Cliente/servidor**: `internal/server` (HTTP sobre unix socket/named
  pipe/TCP), `internal/client`, `internal/proto`, `internal/workspace`
  (AppWorkspace vs ClientWorkspace). `RunComplete` es la señal autoritativa de
  fin de run.

**Puntos útiles para CodeLoom**: modelo de tools y permisos, modelo de mensajes,
catálogo de providers, SQLite/sesiones, config con archivo de contexto, hooks y
skills. Todo esto es la "paridad funcional" a la que aspirar *después* del
motor.

### 3.3 deepseek-harness / dsh (TypeScript)

Harness de agentes "todo es plugin" sobre Cordis. Es el modelo a replicar.

- **Composición**: un `dsh` en ejecución = capas ordenadas de **profiles**
  (web/headless/sdk/sdk-minimal/acp) y **bundles** (`dsh-base`, `dsh-web-app`,
  `dsh-headless`, `dsh-sdk-app`, `dsh-acp-app`, `dsh-sdk-minimal`)
  (`docs/architecture.md:13-39`).
- **Apps**: `apps/cli` (runner de profiles), `apps/web`, `apps/desktop`,
  `apps/desktop-host`.
- **Core**: `packages/core/*` — `session` (`ctx.sessions`), `system-prompt`
  (`ctx.systemPrompt`), `tools` (`ctx.tools`), `agent` (`ctx.agents`),
  `agent-loop` (`ctx.agentLoop`). LLM en `packages/llm/llm` (`ctx.llm`).
- **Plugin system (Cordis)**: cada plugin es un `Service` con `inject`/`apply`;
  reclama una clave `ctx.<key>`; registros reversibles vía `ctx.effect()`.
  Modos de dispatch: `emit`, `waterfall` (middleware con `next()`), `parallel`,
  `serial`, `bail` (`docs/cordis-primer.md:9-45`).
- **Claves de contexto**: `ctx.llm`, `ctx.tools`, `ctx.systemPrompt`,
  `ctx.sessions`, `ctx.agents`, `ctx.agentLoop`, `ctx.subagents`, `ctx.jobs`,
  `ctx.shell`, `ctx.fs`, `ctx.sandbox`, `ctx.commands`, `ctx.agentTeams`,
  `ctx.goals` (`docs/architecture.md:130-163`).

## 4. Comparativa de agentes: crush vs dsh

Diferencia de fondo: en crush el subagente es una **llamada anidada
bloqueante**; en dsh subagentes y jobs son **seams de primera clase** con
runtime, catálogo y observabilidad.

| Eje | crush | dsh |
|---|---|---|
| Definición de agentes | `coder`, `plan` fijos (`config.go:61-63`) + agente `task` | `ctx.agents` + `agent-loop`, vienen de bundles/profiles |
| Quién tiene el loop | `fantasy.Agent.Stream` (externo) | `ReactLoopAgent` (`packages/core/agent-loop/src/agent.ts`) |
| Subagente | Tool `agent` -> `runSubAgent` (`coordinator.go:2246`) llama `Agent.Run` **bloqueante** (`:2307-2323`) | Runtime async `ctx.subagents`; `startActivation` **fire-and-forget** |
| Resultado del hijo | Texto en la tool response (`coordinator.go:2347-2351`) | Aviso de cierre inyectado como mensaje de usuario (`delivery: parent`) o `activation.result` (`delivery: caller`) |
| Catálogo de hijos | No (solo sub-sesión) | `subagent/catalog` + `subagent/descriptor` durables |
| Profundidad / paralelismo | No | `maxDepth=1`, hasta 8 activos |
| Continuación / steering | No | `send_message`, `interrupt`, `waitForChildren`, `drainDescendants` |
| Backends de hijo | Solo el mismo agente | in-process spawn/fork, acp, codex, claude-code, dsh-sdk |
| Jobs | Solo shell background (`job_output`/`job_kill`) | Seam genérico `ctx.jobs` (kind, ring, ciclo de vida) |
| Coste | Se suma al padre (`updateParentSessionCost:2362`) | Resultado/estado por eventos |
| UI | Item de tool, sin estado | Catálogo + jobs con estados en el header |

## 5. El modelo dsh a replicar

### 5.1 Subagentes

- **Seam**: `ctx.subagents` = `SubagentRuntime`
  (`packages/subagent/subagent/src/index.ts`).
  - `registerProvider` (`:543`), `startActivation` (`:253/260/267`),
    `sendMessage` (`:308`), `interrupt` (`:357`), `waitForChildren` (`:290`),
    `drainDescendants` (`:371`).
  - Gestor `SubagentManager` (`manager.ts:130`): `startLocal` (`:379`),
    `startExternal` (`:485`), `sendMessage` (`:545`); añade
    `subagent/descriptor` (`:886`).
  - Config: `maxDepth` (def. 1), `maxActiveSubagents` (def. 8).
- **Contratos** (`subagent/src/types.ts`): capacidades (agentOptions,
  outputSchema, depthLimit, toolFilter, persona); `SubagentActivationSpec`
  (provider, label, childId?, request, signal, delivery); `SubagentActivation`
  (childId, messageId?, result: Promise, dispose()); `SubagentStartRequest`.
- **Eventos**: en vivo `subagent/start`, `subagent/end`,
  `subagent/provider-added|removed`; durables `subagent/descriptor`,
  `subagent/catalog`.
- **Entrega**: `delivery: parent` inyecta aviso de cierre con el texto final +
  `Structured result: <JSON>`; `delivery: caller` no toca al padre.
- **Backends**: `subagent-spawn-in-process` y `subagent-fork-in-process`
  (locales, durables, continuables); `subagent-acp`, `subagent-codex`,
  `subagent-claude-code`, `subagent-dsh-sdk` (externos, one-shot).
- **Tool**: `tool-subagent` devuelve al instante `started subagent <childId>`;
  `tool-subagent-control` expone control/seguimiento y `send_message`.

### 5.2 Jobs

- **Seam**: `ctx.jobs` = `JobRegistry` abstracto
  (`packages/jobs/jobs/src/index.ts`).
- **Tipos** (`jobs/src/types.ts`): `JobSpec` (kind/label/owner/outputLimit/run),
  `JobHandle` (`append`, `updateProgress`), `JobHooks` (`cancel`, `done`),
  `JobOutcome` (`status: completed|killed|failed`, `detail`, `result`).
- **Concreto**: `LocalJobRegistry` (`jobs-local/src/index.ts`): `start` (`:206`),
  `read` (`:316`), `kill` (`:328`), `wait` (`:332`); ring/pump/events.
- **Modelo**: unidad larga con id `<kind>-N`, owner-fenced por sesión, un ring
  de salida acotado, ciclo `running|stopping|completed|killed|failed`.
- **Consumo**: tools `job_output`, `job_list`, `job_kill`; los eventos
  `settled` se convierten en avisos de completitud en la sesión.
- **Cliente/remoto**: `job.list`, `job.follow`, `job.kill`.
- **Relación con subagentes**: son seams separados; `JobOutcome.result` puede
  llevar el informe de un subagente o el valor de un workflow.

### 5.3 Ciclo turn/step

- **Durable** (`session/event`): `turn/start`, `step/start`, `system/message`,
  `user/message`, `assistant/message`, `assistant/attempt`, `tool/call`,
  `tool/result`, `request/header`, `request/context`, `turn/end`.
- **En vivo** (`agent/*`): `agent/inbox/*`, `agent/status`, `agent/pre-step`
  (waterfall), `agent/request` (waterfall), `agent/request-error`,
  `agent/assistant-stream` (start/chunk/end), `agent/turn-stopping`.
- **Definiciones**: un **step** = una petición al modelo + sus tools; un
  **turn** = cero o más steps (`docs/architecture.md:81`).

### 5.4 Representación en el front

El header es un **árbol de slots**, no un componente monolítico:

- `ConversationHeader.tsx:12-26` (outer) y `ConversationSession.tsx:58-160`
  (por sesión) componen *lineage* + *actions* + *utilities* + *corner*.
- **Subagentes**: `ui-subagent/src/client/index.ts:74-94` engancha lineage y
  actions (`id: 'subagent-catalog'`). Componente
  `SubagentHeaderLineage.tsx` (`:235-407`) con dropdown; cada fila muestra
  label, actividad, tokens/duración y un `StateDot`. El click abre el
  transcript del hijo (`openChild` -> `ctx.uiWorkspace.openSession`).
- **Jobs**: `ui-jobs/src/client/index.ts:35-52` engancha actions
  (`id: 'job-list'`). Componente `JobListAction.tsx` (`:188-322`);
  expandir inicia `job.follow` y pinta un `TerminalBlock` con la salida
  acumulada.
- **Estados**: primitiva `StateDot` con estados
  `done | warning | ongoing | error | idle`
  (`ui-primitives/src/StateDot.tsx:9,44-56`).
- **Estado en el cliente**: jobs vía `ClientJobsModel` + `job.list`/`job.follow`
  (`api/job-controller/src/client/`); subagentes derivados de proyecciones de
  sesión (`subagent/catalog`, `subagent/descriptor`) + `api-session/status`.
- **Render**: slot `inject` -> `hooks` -> prop `useXxx` vía
  `useSyncExternalStore` (`renderer.ts:42-45`, `scoped-slots.tsx:183-200`).

## 6. Por qué dsh es lento (hallazgos de rendimiento)

No es el concepto del loop; son ineficiencias de implementación. Son la lista
de cosas a evitar en CodeLoom.

| Causa | Dónde | Efecto |
|---|---|---|
| Dispatch de Cordis: 2 pases + `bind()` por listener + un `Promise` por listener | `vendor/cordis/src/events.ts:165-175`, `agent/src/dispatch.ts:120-137` | Coste por cada evento |
| `snapshotJsonValue` + `deepFreeze` (2 recorridos profundos) **por chunk** | `llm/src/assistant-stream.ts:108-111`, `util/values` | Coste por token |
| Evento `agent/assistant-stream` por chunk + `session/event` síncrono | `agent.ts:449`, `session/src/index.ts:876-890` | Coste por token |
| Pool de tools ordenado: `prepare` de una tool bloquea el arranque de la siguiente | `tool-calls.ts:206-210` | Serialización parcial del paralelismo |
| Persistencia write-behind de 200 ms | `session-persistence-jsonl/src/storage.ts:36` | Retraso de durabilidad |
| Jobs: poll de salida cada 150 ms | `jobs-local/src/index.ts:41,158` | Latencia de reflejo |
| Cierre: `waitForChildren()`/`drainDescendants()` se esperan | `subagent/manager.ts:252-266` | Cierre bloqueante del turno |
| Nota histórica de regresión de apertura de sesión | `.agents/notes/.../2026-09-04-session-open-performance-gate.md:9` | Fold de stream compacto |

`BENCHMARK.md` no tiene cifras de latencia.

## 7. Ventajas de Rust y qué conlleva el port

### 7.1 Ventajas esperadas

- Sin runtime de eventos de JS: dispatch de tokens con canales
  (`tokio::sync::broadcast`/`watch`) o lectura directa de estado, no por-token
  con allocs de `bind`/`Promise`.
- Frames de stream como `&str`/`Bytes` sin clonar; sin `deepFreeze` por chunk.
- Paralelismo real de tools con `FuturesUnordered` (sin serializar `prepare`).
- Jobs con notificación push (`watch`/`Notify`) en vez de polling de 150 ms.
- Emulador VT propio para la tool de shell y para hostear agentes externos.

### 7.2 Mapa de componentes

| Componente | Referencia | En CodeLoom (Rust) |
|---|---|---|
| Provider LLM | crush `buildProvider` (`coordinator.go:1889`) | `reqwest` + SSE; `trait Provider` |
| Step-loop | fantasy `Agent.Stream` + `PrepareStep`/`StopWhen` | `tokio`; loop propio |
| Mensajes + streaming | crush `internal/message` | enums `ContentPart` + frames de stream |
| Tools | crush `internal/agent/tools/*`; dsh `ctx.tools` | `trait Tool` + registro |
| Permisos | crush `internal/permission` | gate por tool+acción+path |
| Subagentes | dsh `ctx.subagents` | `trait SubagentProvider` + runtime + catálogo |
| Jobs | dsh `ctx.jobs` | `JobRegistry` + `JobHandle` + `JobOutcome` + ring |
| Bus de eventos | dsh `session/event` vs `agent/*` | canales durables vs en vivo |
| Persistencia | crush SQLite+sqlc | `sqlx`/`rusqlite` |
| UI | herdr `src/ui/` + dsh header | widgets ratatui + store compartido |
| Daemon + socket API | herdr `src/server`, `src/api` | reutilizable como referencia |
| LSP/MCP/hooks/skills | crush internals | post-MVP |

### 7.3 Estimación gruesa

- **Motor mínimo** (1 provider + tools + loop + UI chat): semanas.
- **Subagentes + jobs + header** (el diferencial): semanas adicionales.
- **Paridad con crush** (multi-provider, LSP, MCP, permisos, skills, hooks,
  persistencia): meses.

## 8. Decisiones abiertas

1. ~~**Objetivo de build**~~ — **RESUELTO (2026-10-13)**: CodeLoom es un
   crate/binario propio. Se integra con herdr **sin parchearlo**, como plugin
   (pane de terminal que ejecuta `loom`) + socket API/CLI. Así herdr queda
   vanilla y se puede actualizar sin merges.
2. ~~**Modo de ejecución**~~ — **RESUELTO (2026-10-13)**: el mismo binario corre
   **standalone** (TUI propia) o **como plugin de herdr** (autodetecta
   `HERDR_BIN_PATH`/`HERDR_SOCKET_PATH`/contexto de plugin y usa herdr como
   host). El core es una librería sin dependencia de herdr; la integración es
   una capa opcional por detección en runtime.
3. ~~**Alcance del MVP**~~ — **RESUELTO (2026-10-13)**: motor mínimo primero.
   Chat + loop + `trait Provider` + ~5 tools (view/edit/bash/glob/grep), sin
   subagentes ni jobs. La orquestación (subagentes/jobs/header) es milestone 2.
4. ~~**Provider**~~ — **RESUELTO (2026-10-13)**: `trait Provider` genérico con
   dos adapters de protocolo: **OpenAI-compatible** (cubre OpenAI, DeepSeek,
   OpenRouter, Ollama, vLLM...) y **Anthropic**. OpenRouter es un preset de
   config sobre el adapter OpenAI-compatible, no un adapter aparte.

## 9. Rutas de referencia rápida

- herdr: `herdr/Cargo.toml`, `herdr/src/{ui,server,api,pane,pty,detect,integration,app}`
- crush: `crush/internal/{agent,config,message,permission,ui,db,lsp,hooks,skills}`
- dsh: `deepseek-harness/packages/{core,llm,subagent,jobs,api,client}`,
  `deepseek-harness/docs/{architecture,agent-lifecycle}.md`,
  `deepseek-harness/docs/subsystems/{subagent,jobs,tools,llm-streaming}.md`
