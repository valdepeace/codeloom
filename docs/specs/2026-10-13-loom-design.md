# CodeLoom — Spec de diseño

> Estado: **borrador para revisión**. No implementar hasta aprobar el spec y
> escribir el plan.
>
> Fecha: 2026-10-13
>
> Documento base de conocimiento: `docs/reference/analisis-herdr-crush-dsh.md`

## 0. Origen y atribución

CodeLoom es un agente de código en Rust cuyo modelo de orquestación **está basado
en deepseek-harness (dsh)**, el harness de agentes de DeepSeek AI:
**agente -> subagentes -> jobs**, con su representación en el header
(subagentes y jobs trabajando/terminados) y su ciclo turn/step.

- **Modelo a replicar**: deepseek-harness (`packages/subagent`, `packages/jobs`,
  `packages/core/agent-loop`; `docs/agent-lifecycle.md`).
- **Referencia de stack**: herdr (Rust, ratatui, daemon, socket API).
- **Referencia funcional**: crush (tools, permisos, mensajes, persistencia,
  config).
- **Motivación**: dsh tiene el diseño correcto de orquestación, pero su
  ejecución es lenta por el runtime de eventos de JavaScript. CodeLoom conserva el
  diseño y **mejora la ejecución** aprovechando el paralelismo real de Rust
  (ver §11).

CodeLoom **no es un fork** de ninguno de los tres. Reimplementa el modelo de dsh en
Rust y se integra con herdr sin parchearlo.

## 1. Decisiones cerradas

| # | Decisión | Fecha |
|---|---|---|
| 1 | **Crate/binario propio**, integrado con herdr como plugin + socket API, sin parchear herdr | 2026-10-13 |
| 2 | **Un binario, dos modos**: standalone o plugin de herdr (detección en runtime) | 2026-10-13 |
| 3 | **MVP**: motor mínimo (chat + loop + provider + 5 tools), sin subagentes/jobs | 2026-10-13 |
| 4 | **Provider**: `trait` genérico + adapters OpenAI-compatible y Anthropic; OpenRouter como preset | 2026-10-13 |

### Nomenclatura

- **CodeLoom** — el producto (el telar: teje turnos, subagentes y jobs).
- **crates.io** — `codeloom-core`, `codeloom-cli`, `codeloom-tui` (el nombre del
  crate debe ser único en crates.io).
- **Binarios** — `loom` (comando corto) y `codeloom`, ambos construidos por
  `codeloom-cli` (el nombre del binario no necesita ser único).
- **`codeloom-herdr`** — crate de la integración opcional con herdr.
- **`codeloom.herdr`** — id del plugin de herdr (acciones cualificadas
  `codeloom.herdr.*`).
- **
- **"flock"** — nombre del conjunto de subagentes que CodeLoom muestra en su header.

## 2. Arquitectura de crates

```
codeloom/                  (workspace)
├── crates/
│   ├── codeloom-core/         motor: sin dependencia de herdr ni de ratatui
│   │   ├── provider/      trait Provider + adapters (openai_compat, anthropic)
│   │   ├── message/       modelo de mensajes + content parts + streaming
│   │   ├── tools/         trait Tool + registro + tools del MVP
│   │   ├── agent/         step-loop (turn/step, cancelación, colas)
│   │   ├── subagent/      (m2) runtime de subagentes + catálogo
│   │   ├── job/           (m2) JobRegistry + handle + outcome + ring
│   │   ├── event/         bus: durables vs en vivo
│   │   ├── session/       persistencia + historial
│   │   └── config/        config + contexto (AGENTS.md)
│   ├── codeloom-tui/          ratatui: chat + header de subagentes/jobs
│   ├── codeloom-herdr/        integración opcional: detección + cliente socket
│   └── codeloom-cli/          binarios `loom` y `codeloom`: une todo, autodetecta el modo
```

Principios:

- `codeloom-core` **no conoce herdr ni ratatui**: motor testeable y reutilizable.
- `codeloom-herdr` es **opcional** y activable por detección en runtime.
- Un solo binario (`codeloom-cli`), dos modos.

## 3. Concurrencia y paralelismo

Separar cargas por tipo (no paralelizar todo):

| Carga | Naturaleza | Herramienta Rust |
|---|---|---|
| Streaming LLM (SSE), HTTP, socket herdr | I/O-bound | tokio multi-thread (work-stealing) |
| Dispatch de tools | I/O + mix | `FuturesUnordered` / `JoinSet` + barreras |
| Subagentes y jobs | I/O-bound, larga vida | una tarea tokio por hijo, resultado por canal |
| grep/glob de archivos | CPU-bound | rayon / `spawn_blocking` |
| Parseo SSE/JSON, diff, tokens | CPU-bound | `spawn_blocking` |
| Render ratatui + input crossterm | propio | hilo dedicado |

Modelo:

1. **Reactor async (tokio multi-thread)**: step-loop y providers. Tools del
   mismo step en paralelo con `JoinSet` + pool acotado, con barreras para tools
   exclusivas.
2. **Compute pool**: búsqueda, glob, diff y parseo pesado con
   `spawn_blocking`/rayon, para no bloquear el reactor.
3. **Subagentes/jobs = tareas propias**: el padre **nunca espera**; avisos por
   canal; cancelación con `CancellationToken`.
4. **Hilo de render**: sin locks en el camino caliente; snapshots por `watch` +
   deltas.
5. **Estado compartido**: preferir message-passing/actor; `arc_swap`/`watch`
   para lectura frecuente.
6. **Backpressure**: canales acotados en el bus y en los rings de jobs.

## 4. Modelo de datos y step-loop

### 4.1 Mensajes

```rust
enum Role { System, User, Assistant, Tool }

struct Message {
    id: MessageId,
    role: Role,
    parts: Vec<ContentPart>,
    provider_meta: Option<ProviderMeta>,
    usage: Option<Usage>,
    created_at: Timestamp,
}

enum ContentPart {
    Text { text: String },
    Reasoning { text: String, signature: Option<String> },
    ImageUrl { url: String },
    ToolCall { id: CallId, name: String, input: JsonValue },
    ToolResult { call_id: CallId, content: String, is_error: bool },
    Finish { reason: FinishReason }, // stop | length | tool_calls | error
}
```

El mensaje es el **formato propio**; cada adapter traduce a/desde su protocolo.

### 4.2 Seam del provider

```rust
#[async_trait]
trait Provider: Send + Sync {
    fn capabilities(&self) -> Capabilities; // tool_use, reasoning, streaming
    async fn stream(
        &self,
        req: ProviderRequest,
        cancel: CancellationToken,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>>;
}

enum StreamChunk {
    Started,
    ReasoningDelta(String),
    TextDelta(String),
    ToolInputDelta { id: CallId, json: String },
    ToolCallComplete { id: CallId, name: String, input: JsonValue },
    Usage(Usage),
    Done(FinishReason),
}
```

Adapters: `openai_compat` (Chat Completions + `tools`) y `anthropic` (Messages
+ bloques `tool_use`/`tool_result`). OpenRouter/DeepSeek = config over
`openai_compat`.

### 4.3 Turn y step

```
turn_start
  loop step:
    build request (system prompt + historial + tool schemas)
    provider.stream ──chunks──> acumula assistant message
                              └─ deltas en vivo ──> TUI
    persist assistant/message (una vez, al settle)
    if tool_calls:
       classify (parallel | exclusive)
       schedule ──> tool results (durable)
       continue loop
    else: stop
  turn_end
```

- **Durabilidad por mensaje, no por chunk**: el stream se acumula y se persiste
  una vez al cerrar el step. Nada de escrituras por token.
- **Deltas en vivo** por `watch`; el stream durable a SQLite al settle.
- **StopWhen**: nº máx. de steps, detección de bucle (tool-calls idénticas),
  presupuesto de tokens, cancelación.
- **Cancelación** con `CancellationToken` (entre steps y a mitad de stream).

### 4.4 Persistencia y bus

- `SessionStore` (trait) con impl SQLite (`sqlx`): `sessions`, `messages`
  (parts como JSON), `runs`.
- Bus de dos carriles: `durable` (persistido, replay) y `live` (`watch`, UI).
- Backpressure con canales acotados; historial por chunks.

### 4.5 Errores

- Provider -> retry con backoff exponencial + jitter, reintentando el mismo step.
- Tool -> `ToolResult { is_error: true }`; permiso denegado -> opción `StopTurn`.
- Cancelación -> cierre limpio; el mensaje en curso se marca como intento fallido.

## 5. Tools y permisos

### 5.1 Trait

```rust
#[async_trait]
trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;              // .md embebido
    fn schema(&self) -> JsonSchema;
    fn execution_mode(&self, input: &JsonValue) -> ExecutionMode;
    async fn run(&self, ctx: ToolCtx, input: JsonValue, cancel: CancellationToken)
        -> Result<ToolOutput>;
    fn present(&self, out: &ToolOutput) -> ToolView;
}

enum ExecutionMode { Parallel, Exclusive }
struct ToolOutput { content: String, is_error: bool, meta: Option<JsonValue> }
```

- `Exclusive` (edit/write) forma barrera; `Parallel` (view/glob/grep) corre
  concurrente.
- `present` separa cómputo de representación.
- Descripciones en `.md` con `include_str!`.

### 5.2 Registro

`ToolRegistry`: mapa `nombre -> Box<dyn Tool>`; filtrado por config; genera JSON
schema para el provider.

### 5.3 Tools del MVP

| Tool | Qué hace | Modo |
|---|---|---|
| `view` | Leer archivo con offset/limit + nº de línea | Parallel |
| `edit` | Find/replace exacto | Exclusive |
| `bash` | Ejecutar comando (con lista de comandos prohibidos) | Exclusive |
| `glob` | Buscar por patrón | Parallel |
| `grep` | Buscar contenido por regex | Parallel |

### 5.4 Permisos

- Clave por `tool + acción + path`.
- Flujo: hook pre-aprobación (posterior) -> allowlist -> prompt UI ->
  `Grant | Persistent | Deny`; modo `--yolo`.
- Denegado -> `ToolResult { is_error }` + opción `StopTurn`.
- El motor no se bloquea: espera por `oneshot`.

### 5.5 Extensiones (diseñadas ahora, implementadas después)

- Hooks pre/post-tool-use como middleware.
- MCP y LSP entran como tools adicionales en el registro.
- **MCP y skills con carga en caliente** (sin reiniciar): registro versionado +
  supervisor MCP + watcher. Spec propio:
  `docs/specs/2026-10-14-loom-mcp-skills-hot-reload.md`.
- **Entrada por voz al prompt** (dictado): CodeLoom como cliente de un servicio de
  voz (`tipsy-assistant`), texto inyectado en el composer. Idea:
  `docs/specs/2026-10-14-loom-voice-prompt-input.md`.
- **Resumen del último turno** (1-3 frases): tl;dr en el transcript y texto que
  habla la capa de voz. Idea:
  `docs/specs/2026-10-14-loom-turn-summary.md`.
- **Hub de decisiones local** (`jevalan` + Kev/Laya/Jev): riesgo de tools,
  enrutado de modelo y despertares sin llamada al LLM. Idea:
  `docs/specs/2026-10-14-loom-decision-hub-jevalan.md`.
- **Gauntlet Loop** (builder + crítico ciego con contexto limpio), activable por
  caso; Jev solo como filtro. Idea:
  `docs/specs/2026-10-14-loom-gauntlet-loop.md`.
- **Consumo y activación de skills** (`ponytail`, `superpowers`): modos
  off/on/explicit/auto (por jevalan) y regla de arbitraje cuando se contradicen.
  Idea: `docs/specs/2026-10-14-loom-skills-ponytail-superpowers.md`.
- **KPIs y compresión de prompts/contexto** (`headroom`, `rtk`, `caveman`): medir
  provider/modelo/effort y reducir tokens sin perder calidad. Idea:
  `docs/specs/2026-10-14-loom-kpis-prompt-compression.md`.
- **Memoria** (proyecto, general, episódica, grafo): cargada por demanda y
  enrutada por jevalan, con salud de memoria y recall. Idea:
  `docs/specs/2026-10-14-loom-memory.md`.
- **Plan de experimentos** (qué probar de los papers: cascades, routing, gate de
  riesgo) con el lab de jevalan como harness. Plan:
  `docs/specs/2026-10-14-loom-decision-experiments.md`.
- **Capa estática de decisión** (señales + reglas + prioridad, sin LLM, al estilo
  de `semantic-router`): una forma declarativa para todas las decisiones. Idea:
  `docs/specs/2026-10-14-loom-static-decision-signals.md`.
- **Una sola puerta de descubrimiento** (tools/skills/MCP bajo demanda): evita
  cargar todas las esquinas en cada turno. Idea:
  `docs/specs/2026-10-14-loom-tool-skill-discovery.md`.

## 6. Subagentes y jobs (basado en dsh)

### 6.1 Subagentes

```rust
#[async_trait]
trait SubagentProvider: Send + Sync {
    fn name(&self) -> &str;
    fn capabilities(&self) -> SubagentCapabilities;
    async fn start(&self, req: SubagentStartRequest, cancel: CancellationToken)
        -> Result<SubagentActivation>;
}

struct SubagentStartRequest {
    provider: String, label: String, prompt: String,
    parent: SessionId, cwd: Option<PathBuf>,
    agent_options: Option<AgentOptions>, output_schema: Option<JsonSchema>,
    max_depth: usize, tool_filter: Option<ToolFilter>, persona: Option<String>,
}

struct SubagentActivation {
    child_id: SessionId,
    result: oneshot::Receiver<SubagentResult>,
    dispose: CancellationToken,
}
```

- `SubagentRuntime`: registro de providers + `max_depth` (def. 1) y
  `max_active_subagents` (def. 8, con `Semaphore`).
- Backend mínimo (m2): `in-process` (child = tarea tokio con su `AgentLoop` y
  `Session`). Después: fork, acp, claude-code, codex, dsh-sdk.
- Entrega: `Parent` -> aviso de cierre (rol usuario) con texto final +
  `Structured result: <JSON>`; `Caller` -> por `oneshot`.
- Catálogo durable: `subagent/descriptor` + `subagent/catalog`.
- Control: `send_message`, `interrupt`; guardia de profundidad.
- Tools: `subagent` (arranca y devuelve `{ child_id }`) y `subagent_control`.

### 6.2 Jobs

```rust
trait JobRegistry: Send + Sync {
    fn start(&self, spec: JobSpec) -> Result<JobHandle>;
    fn read(&self, id: &JobId, from: usize) -> Result<JobRead>;
    fn kill(&self, id: &JobId);
    async fn wait(&self, id: JobId) -> JobOutcome;
}

struct JobSpec {
    kind: String, label: String, owner: SessionId,
    output_limit: usize, run: JobBody,
}
struct JobHandle { /* append(text), update_progress(frac, label) */ }
enum JobStatus { Running, Stopping, Completed, Killed, Failed }
struct JobOutcome { status: JobStatus, detail: Option<String>, result: Option<JobResult> }
```

- `LocalJobRegistry`: id `<kind>-N`, owner-fenced por sesión, ring acotado,
  máquina de estados.
- Push por `watch`/`Notify`, no poll.
- Tools: `job_output`, `job_list`, `job_kill`; `settled` -> aviso de
  completitud (mensaje de usuario).

### 6.3 Relación y ciclo

- Seams independientes; un job puede llevar el informe de un subagente como
  resultado.
- Ambos cierran inyectando un aviso de usuario -> el loop lo procesa en el
  siguiente step.
- Cancelación padre->hijo con `CancellationToken`; join/drain no bloqueante.

### 6.4 Datos para el header

`watch` con dos listas (subagentes activos/terminados, jobs) derivadas del
catálogo durable + estado en vivo, con `StateDot = running|done|error|idle`.

## 7. UI ratatui

```
┌─ header ─────────────────────────────────────────────────────────┐
│ CodeLoom · workspace  · ⠋ 2 subagentes  ✓ 1 terminado  · 1 job 68%  │
├─ transcript ─────────────────────────────────────────────────────┤
│ you  ...                                                         │
│ ⚙ bash  cargo test            (collapsible)                      │
│ ✎ edit  src/main.rs  +12 -3   (diff)                             │
│ ◆ reasoning ...                                                  │
│ assistant ...                                                    │
├─ composer ───────────────────────────────────────────────────────┤
│ > _                                                              │
└──────────────────────────────────────────────────────────────────┘
        ↑ overlays: permiso, modelo, detalle de subagente/job
```

- Hilo de render dedicado (crossterm + ratatui 0.30).
- Sin locks en el camino caliente; espejo local de `AppState` + deltas.
- Input -> motor por `mpsc`; motor -> UI por `watch`.
- Deltas de stream **coalescidos** (~16-33 ms); repintado por tick/dirty,
  nunca por token.
- Transcript con scrollback acotado y viewport virtualizado; markdown cacheado.

Widgets (`codeloom-tui`): `HeaderBar`, `Transcript`, `SubagentCatalog`, `JobList`,
`Composer`, `PermissionDialog`. Tema/keymap en TOML.

## 8. Integración con herdr

Un binario, dos modos, sin parchear herdr. Solo superficies estables:
manifiesto de plugin + CLI + socket API.

### 8.1 Detección

```rust
enum Host { Standalone, Herdr(HerdrContext) }
```

`HERDR_SOCKET_PATH`/`HERDR_BIN_PATH` presentes -> modo herdr.

### 8.2 Manifiesto

```toml
id = "codeloom.herdr"
name = "CodeLoom"
version = "0.1.0"
min_herdr_version = "0.9.0"
platforms = ["linux", "macos", "windows"]

[[build]]
command = ["cargo", "build", "--release"]

[[panes]]
id = "loom"
title = "CodeLoom"
placement = "tab"
command = ["loom"]
```

Acciones cualificadas: `codeloom.herdr.open`, `codeloom.herdr.focus`, etc.

### 8.3 Cliente (`codeloom-herdr`)

- Cliente del JSON API (enum `Method`, `herdr/src/api/schema.rs`) + suscripción
  de eventos.
- Acciones por la CLI (`HERDR_BIN_PATH`); eventos por el socket.
- Guard de versión (`min_herdr_version` + `herdr status`/`api schema`).

### 8.4 Qué aporta cada lado

- herdr -> CodeLoom: pane/terminal, detach/reattach, SSH, `pane.report_agent`
  (estado en el sidebar), eventos de workspace.
- CodeLoom -> herdr: su header de subagentes/jobs; (m3 opcional) registrar
  subagentes como panes/agentes de herdr.

### 8.5 Compatibilidad y degradación

- Todo el acoplamiento en `codeloom-herdr`; el core no cambia.
- Actualizar herdr = actualizar un binario; sin merges.
- Socket caído -> cae a standalone (no crashea).
- `codeloom.toml` + `AGENTS.md` idénticos en ambos modos.

## 9. Testing, milestones y criterios

### 9.1 Testing

| Nivel | Qué cubre | Herramientas |
|---|---|---|
| Unit | adapters (fixtures SSE), conversión de mensajes, tools, estados de jobs, subagent runtime | `cargo test`, `tokio::test` |
| Integración | loop con provider mock; scheduler parallel/exclusive; avisos de cierre; ring/backpressure | `tokio::test`, mock |
| Property/fuzz | fragmentación de SSE, streaming de tool input | `proptest` |
| Snapshot UI | header con N subagentes/jobs, tool items, permiso | `TestBackend` + `insta` |
| E2E | standalone vs servidor OpenAI-compat falso; herdr vs socket falso | mock server/socket |
| Benchmarks | tok/s, time-to-first-token, coste por chunk, N tools, fan-out | `criterion` |

Nada de red real en tests.

### 9.2 Milestones

- **m0** Scaffolding (workspace, core types, config, CI local).
- **m1 MVP**: provider (openai_compat + anthropic) + mensajes + step-loop +
  5 tools + permisos + TUI chat. Meta: tarea multi-step con tools en standalone.
- **m2 Orquestación**: subagentes `in-process` + `LocalJobRegistry` + header +
  avisos de cierre.
- **m3 Integración herdr**: manifiesto + detección + socket + `pane.report_agent`.
- **m4 Paridad/alcance**: multi-provider, MCP (con carga en caliente, ver
  `docs/specs/2026-10-14-loom-mcp-skills-hot-reload.md`), LSP, hooks, skills,
  persistencia.

(m3 puede adelantarse a m2 si se quiere ver CodeLoom dentro de herdr antes.)

### 9.3 Criterios de éxito (medibles)

- TTFT y throughput comparables o mejores que crush y sin el coste por-chunk
  de dsh.
- Loop multi-step fiable con tool-use, cancelación y gate de permisos.
- N subagentes/jobs concurrentes usan N núcleos; el stream nunca se bloquea.
- Header con estados en tiempo real por push, no por poll.

### 9.4 Riesgos

- Sutilezas del loop de `fantasy` (cancelación, colas).
- Deriva de protocolos de provider (mitigado por adapters + fixtures).
- Cambios de la API de herdr (mitigado por `min_herdr_version` + capa aislada).

## 10. Resumen de arquitectura

- **Modelo**: dsh (agente -> subagentes -> jobs, header, turn/step).
- **Motor**: Rust, `codeloom-core`, sin herdr ni ratatui; tokio + compute pool.
- **UI**: ratatui, header de subagentes/jobs, deltas coalescidos.
- **Host**: herdr opcional vía plugin + socket API; standalone por defecto.
- **Ejecución**: paralelismo real de Rust (§11).

## 11. Mejoras de ejecución respecto a dsh

CodeLoom **reutiliza el diseño de dsh pero ejecuta distinto**. Estas son las
mejoras concretas, con su causa en dsh y cómo se resuelve en CodeLoom:

| # | En dsh (causa) | En CodeLoom (mejora) |
|---|---|---|
| 1 | Event loop **de un solo hilo** (JS/GIL): todo compite por un hilo | tokio multi-thread + **compute pool**; los subagentes/jobs usan **N núcleos reales** |
| 2 | Por cada chunk: dispatch de Cordis con **2 pases + `bind()` por listener + un `Promise` por listener** (`vendor/cordis/src/events.ts:165-175`, `agent/src/dispatch.ts:120-137`) | Canales (`watch`/`broadcast`) y deltas agrupados; **cero allocs por token** |
| 3 | Por cada chunk: `snapshotJsonValue` + `deepFreeze` (**2 recorridos profundos**, `llm/src/assistant-stream.ts:108-111`) | Frames con `Arc<str>`/`Bytes`, sin deep-walk; **sin copia por token** |
| 4 | Pool de tools **ordenado**: `prepare` de una tool serializa el arranque de la siguiente (`tool-calls.ts:206-210`) | `JoinSet` + reclassify; **paralelismo real** (solo las `Exclusive` forman barrera) |
| 5 | Jobs con **poll de 150 ms** (`jobs-local/src/index.ts:41,158`) | Push por `watch`/`Notify`; **latencia ~0** en el reflejo de estados |
| 6 | Persistencia write-behind de **200 ms** (`session-persistence-jsonl/src/storage.ts:36`) | Append por mensaje al cerrar el step + **ring con backpressure** |
| 7 | Cierre con `waitForChildren()`/`drainDescendants()` **bloqueante** (`subagent/manager.ts:252-266`) | Join/drain **no bloqueante**; cancelación por `CancellationToken` |
| 8 | Dispatch síncrono de `session/event` por evento aceptado (`session/src/index.ts:876-890`) | Carril durable separado + bus async; el render no depende de la persistencia |

Efecto buscado: el loop de tokens **nunca se bloquea** por una búsqueda ni por
un subagente, y el coste de UI/procesamiento **no escala con el nº de tokens**,
que era el techo de dsh. La equivalencia funcional (qué hace) con dsh se
mantiene; cambia el **cómo** (ejecución).

## 12. Referencias

- Base de conocimiento: `docs/reference/analisis-herdr-crush-dsh.md`.
- deepseek-harness: `packages/{core,llm,subagent,jobs,api,client}`,
  `docs/{architecture,agent-lifecycle}.md`,
  `docs/subsystems/{subagent,jobs,tools,llm-streaming}.md`.
- herdr: `src/{ui,server,api,pane,pty}`,
  `docs/versions/0.9.3/website/src/content/docs/plugins.mdx`.
- crush: `internal/{agent,config,message,permission,ui,db,lsp,hooks,skills}`.
