# CodeLoom — Idea/spec: profile de agentes y comunicación A2A

> Estado: **idea en exploración**. No implementar.
>
> Fecha: 2026-10-15
>
> Alcance deliberadamente **estrecho**: (1) el **profile** de un agente (su
> configuración) y (2) la **comunicación entre agentes** (A2A como sustrato). El
> resto (delegación en la UI, mecánica relay/handoff, fases por milestone) se
> documenta en otros specs.
>
> Relacionado: `2026-10-13-loom-design.md` (§6 subagentes/jobs, §5.5 extensiones),
> `2026-10-14-loom-architecture-overview.md` (mapa, §9 control de activaciones),
> `2026-10-14-loom-tool-skill-discovery.md` (puerta única tools/skills/MCP),
> `2026-10-14-loom-skills-ponytail-superpowers.md` (skills),
> `2026-10-14-loom-mcp-skills-hot-reload.md` (MCP).

## 0. La idea

Cada agente (frontend, devops, ...) tiene una **identidad y un scope propios**: su
*profile* declara qué MCP, skills y tools puede usar, y su persona (cómo trabaja).
Dos agentes se hablan por **A2A**, el protocolo estándar de comunicación
agente-a-agente, igual que las tools se hablan por MCP.

La intuición de partida: **no inventar**. A2A estandariza la comunicación (como MCP
la de tools); el *profile* es justo el hueco que ningún protocolo cubre, así que se
copia el **formato** de lo ya adoptado (frontmatter YAML de agentes) y se reserva
la gobernanza rica como extensión opcional.

Reparto de responsabilidades:

- **Comunicación** -> A2A v1.0 (Linux Foundation). **No** definimos un protocolo
  propio.
- **Profile** -> un documento local (`AgentProfile`) con frontmatter YAML + body,
  del que se **deriva** la Agent Card pública y del que se **ingieren** las
  remotas.
- **Scope** (qué MCP/skills/tools ve) -> **local**, sin equivalente en A2A. Es la
  aportación de CodeLoom.

## 1. Estado del arte (evidencia, no opinión)

### 1.1 A2A (Agent2Agent)

| Dato | Valor |
|---|---|
| Mantenedor | **Linux Foundation** (donado por Google); TSC con AWS, Cisco, IBM, Microsoft, Salesforce, SAP, ServiceNow |
| Licencia | Apache 2.0 |
| Repo | `github.com/a2aproject/A2A` |
| Versión | **v1.0** (9 abr 2025); versionado `Major.Minor` |
| Agent Card | JSON en `/.well-known/agent-card.json` (`Content-Type: application/a2a+json`) |
| Campos de la card | `name`, `description`, `supportedInterfaces`, `version`, `capabilities`, `defaultInputModes`, `defaultOutputModes`, `skills` (con `tags`), y opcionales `provider`, `securitySchemes`, `security`, `signatures`, `iconUrl` |
| Transportes | HTTP JSON-RPC 2.0 · SSE (streaming) · **gRPC** · HTTP+JSON REST |
| Tareas | 9 estados: `submitted`, `working`, `input-required`, `auth-required`, `completed`, `failed`, `canceled`, `rejected`, `unspecified` |
| Seguridad | `apiKey` / `httpAuth` / `oauth2` / `openIdConnect` / `mTLS`; firmas JWS |
| SDK Rust | **`a2a-lf`** (`cargo add a2a-lf`) |
| Versión en peticiones | header `A2A-Version: 1.0` (vacío = `0.3`) |

Conclusión: A2A es el análogo de MCP para la comunicación entre agentes, y hay SDK
en Rust, así que **no partimos de cero**.

### 1.2 Papers

- **arXiv 2505.02279** — *A Survey of Agent Interoperability Protocols: Model
  Context Protocol (MCP), Agent Communication Protocol (ACP), Agent-to-Agent
  Protocol (A2A), and Agent Network Protocol (ANP)* (Ehtesham, Singh, Gupta, Kumar;
  2025). El survey de referencia: compara modos de interacción, descubrimiento,
  patrones de comunicación y seguridad, y propone un roadmap de adopción por fases.
- **arXiv 2504.16736** — *A Survey of AI Agent Protocols* (Yang et al., 2025).
  Clasificación en dos ejes (contexto vs inter-agente; general vs dominio).
- **arXiv 2506.19676** — *A Survey of LLM-Driven AI Agent Communication: Protocols,
  Security Risks, and Defense Countermeasures* (Kong et al., 2025).
- **arXiv 2504.16902** — *Building a Secure Agentic AI Application Leveraging A2A
  Protocol* (Habler, Huang, Narajala, Kulkarni; 2025). A2A con el framework
  MAESTRO: gestión de Agent Cards, integridad de tareas, auth.

### 1.3 Formatos de *profile* (el hueco real)

Ningún protocolo de comunicación define **identidad ni scope** de un agente. De
dónde copiar el formato, por credibilidad:

| Fuente | Qué aporta | Veredicto |
|---|---|---|
| **Claude Code subagents** `.claude/agents/*.md` | frontmatter YAML: `name`, `description`, `tools`, `model`, `effort`, `maxTurns` | **de-facto**, copiar |
| **Cursor** `.cursor/agents/*.md` | `name`, `description`, `model`, `readonly` | adoptado, copiar |
| **GitHub Copilot** `.github/agents/*.agent.md` | `name`, `description` (portable) | adoptado, copiar |
| **CrewAI** `Agent` | `role`, `goal`, `backstory`, `tools`, `llm`, `memory`, `mcps`, `skills`, `allow_delegation` | mapea casi 1:1 |
| **A2A Agent Card** | representación **externa** (capacidades/seguridad) | estándar, pero **sin scope** |
| **OSSA** (`openstandardagents.org`) | gobernanza: `tool_permissions`, `risk_level`, allow-list de red | **solo minar ideas** (5 stars, ~42 descargas/sem, un vendor). **No depender.** |

Lectura clave: el **scope** (qué MCP/skills/tools puede tocar un agente) **no** cabe
en la Agent Card; es lo que A2A deja fuera y lo que CodeLoom aporta. Por eso el
*profile* es **interno** y la card es su **proyección pública**.

## 2. AgentProfile (formato)

Un agente es **un fichero**: frontmatter YAML (config) + body markdown
(instrucciones/persona). Mismo patrón que las skills, y compatible con el de
Claude Code / Cursor / Copilot.

Ubicación propuesta: `~/.agents/agents/<name>.md` (fuera del repo, junto a las
skills; ver `2026-10-14-loom-skills-ponytail-superpowers.md`).

```markdown
---
name: frontend
description: Especialista en UI, componentes y accesibilidad
version: 1
persona:
  role: Ingeniero de frontend
  goal: Implementar y revisar UI del proyecto
  backstory: Prefiere componentes pequeños, testeables y accesibles
scope:                     # allowlist: lo que ESTE agente puede cargar
  mcp: [serena, figma]
  skills: [ponytail, accessibility]
  tools: [view, edit, glob, grep, bash]
model: claude-sonnet
effort: high
max_turns: 20
max_depth: 1
delegation:
  allow: true
  policy: both            # relay | handoff | both
transport: a2a-remote     # in-process | a2a-remote
a2a:
  endpoint: https://devops.internal/.well-known/agent-card.json
  card: devops            # id del par remoto
---

Eres el agente de frontend. Solo atiendes tareas de UI, componentes y accesibilidad.
Si la tarea es de otra especialidad, dilo y delega con `delegate`.
```

Campos, agrupados:

| Grupo | Campos | Notas |
|---|---|---|
| Identidad | `name`, `description`, `version` | lo mínimo que también pide la card |
| Persona | `persona.{role,goal,backstory}` + **body** | el body es el system prompt |
| Scope | `scope.{mcp,skills,tools}` | allowlist; **sin equivalente A2A** |
| Ejecución | `model`, `effort`, `max_turns`, `max_depth` | reusa `AgentOptions` de §5 |
| Delegación | `delegation.{allow,policy}` | política por agente (default `relay`) |
| Transporte | `transport`, `a2a.{endpoint,card}` | local o remoto |

Reglas:

- **`scope` es obligatorio** si `delegation.allow = true`; sin scope, el agente no
  sabe qué es "fuera de su dominio".
- **Los campos son aditivos y tolerantes**: un `AgentProfile` incompleto usa
  defaults (transporte `in-process`, `delegation.allow=false`). Nada de fallar por
  un campo nuevo.
- **Gobernanza OSSA (opcional, futuro)**: un bloque `governance:` (a la OSSA:
  `tool_permissions`, `risk_level`, allow-list de red) puede añadirse *encima*, sin
  que el resto dependa de él. Se documenta como extensión, **no** como requisito.

## 3. Scope = allowlist (la aportación de CodeLoom)

- **Lo fuera de scope no se carga**: los MCP/skills fuera de la allowlist no entran
  en el prefijo (conecta con la puerta única y con hot-reload).
- **Se hace frontera dura** en el gate: si el modelo pide algo fuera de scope, se
  responde con un `ToolResult` de error inmediato (`"fuera de scope: esto es de
  <dominio>, delega con `delegate`"`). No gasta un step de LLM propio; es una tool
  call fallida, como un `tool.risk` denegado.
- **Tier 0 siempre**: núcleo (`view/edit/bash/glob/grep`) + la puerta
  (`discover`/`delegate`) nunca se filtran; si no, el agente no puede salir de su
  jaula.
- **Cargar un agente no concede permisos**: sigue aplicando el gate `tool.risk`.
- **Pertenencia de una tarea a un dominio = señal**, no adivinanza: estático
  (keyword/estructura) primero, jevalan después. El agente *sabe* que no es suyo
  por la misma capa que decide todo lo demás (§4 del mapa).

## 4. Agent Card: proyección y mapeo

La card es la **vista externa**; el `AgentProfile` es la **vista interna**.

- **Síntesis (local)**: un agente local **construye su card en memoria** desde el
  profile. **Tener card no requiere servidor**: una card es un *documento*.
- **Ingesta (remoto)**: de un par remoto se obtiene la card (`GET
  /.well-known/agent-card.json`) y se normaliza a un `AgentDescriptor` interno.
- **Mapeo**:

| AgentProfile (interno) | Agent Card (externo, A2A v1.0) |
|---|---|
| `name` | `name` |
| `description` | `description` |
| `version` | `version` |
| `persona.*` + body | (no viaja; solo `description`) |
| `scope.mcp/skills/tools` | **no tiene equivalente** (queda local) |
| `transport`, `a2a.endpoint` | `supportedInterfaces[]` |
| `delegation` | **no tiene equivalente** (local) |
| — | `skills[]` (con `tags`), `capabilities`, `securitySchemes` |

- **Servir la card** es un **rol**, no un requisito: solo se expone por HTTP si esta
  instancia debe **aceptar** A2A entrante. Por defecto **apagado**.

## 5. A2A como transporte

- **Dónde vive**: crate **opcional** `codeloom-a2a` (como `codeloom-herdr`,
  `codeloom-decide`). **El core no conoce A2A.**
- **SDK**: `a2a-lf` como base (Agent Card, operaciones, tipos).
- **Dos transportes, mismo contrato** detrás de `AgentTransport`:
  - `InProcess`: mensajes en memoria, **sin socket**. Es lo que usan los tests.
  - `A2aRemote`: HTTP JSON-RPC + SSE (y gRPC como opción).
- **Roles**: `serve | consume | both`, en `codeloom.toml`, **default off**. El caso
  común (standalone, un proceso) nunca levanta un server.
- **Mapeo de ciclo de vida**: `Task` A2A <-> subagente/job del núcleo:
  `working` = step en curso, `input-required` = aviso dormido (`agent.wakeup`),
  `completed` = `SubagentResult`, `canceled` = `CancellationToken`.
- **Streaming**: `SendStreamingMessage`/SSE entra por el mismo camino que
  `provider.stream`; no hay canal de UI paralelo.
- **Versionado**: se pinea `A2A-Version` (`1.0`) y se envuelve en tipos propios
  (`AgentCard`, `Task`); un cambio del spec se absorbe en el crate.

### Estado de tarea (2026-10-21)

Ciertamente, ya existe `codeloom_core::task::TaskState`, enumerado puro con los
estados de tarea de A2A, que se obtiene por proyección desde los estados
internos. Cabe señalar que **no lleva serialización**: los nombres del cable
dependen de la versión (v1.0: `TASK_STATE_INPUT_REQUIRED`; v0.3:
`input-required`) y los fija el adaptador, no el core.

| Estado interno                                       | `TaskState`  |
|------------------------------------------------------|--------------|
| `JobStatus::Running`, `JobStatus::Stopping`          | `Working`    |
| `JobStatus::Completed`, `SubagentStatus::Completed`  | `Completed`  |
| `JobStatus::Failed`, `SubagentStatus::Failed`, `SubagentStatus::Interrupted` | `Failed` |
| `JobStatus::Killed`, `SubagentStatus::Killed`        | `Canceled`   |

Cabe señalar que `Interrupted -> Failed` es decisión nuestra: A2A carece de un
equivalente a «interrumpida», de modo que la tarea se proyecta a fallo por haber
quedado sin terminar. El modelo interno (`JobStatus`, `SubagentStatus` y el
esquema SQLite) no cambia; la traducción vive solo en el borde.

### Hallazgos de la lectura de la especificación oficial (2026-10-21)

Fuente: `a2a-protocol.org/latest/specification/` (v1.0.0) y las páginas de
conceptos, ciclo de vida de una tarea y streaming. No se leyó el `a2a.proto`
ni las secciones de bindings (§9–§13), de las que solo se dispone de resumen.

- **Nombres del cable en v1.0**: estados `TASK_STATE_*`, roles `ROLE_USER` /
  `ROLE_AGENT`, `Part` única con una de las claves `text`, `raw`, `url`,
  `data`; los eventos de stream se envuelven en `statusUpdate` /
  `artifactUpdate`; operaciones `SendMessage`, `SendStreamingMessage`,
  `GetTask`, `ListTasks`, `CancelTask`, `SubscribeToTask`.
- **`contextId` y `taskId`**: el primero agrupa varias tareas de una misma
  conversación; el segundo identifica una tarea y lo genera el servidor. Una
  tarea en estado terminal es **inmutable**: todo refinamiento es una tarea
  nueva en el mismo contexto, con `referenceTaskIds`.
- **Reanudar `input-required`**: nuevo mensaje con el mismo `taskId` (y
  `contextId`).
- **Resultados**: la especificación indica que los `Message` no deben
  entregar resultados; estos van en `Artifact`.
- **Tareas largas**: `returnImmediately` más `SubscribeToTask` o *push*; al
  recibir la notificación, el cliente llama a `GetTask`. De ahí se sigue que
  el cliente debe **reaccionar** a `input-required` y a los estados
  terminales, y no esperar a que el humano vuelva a escribir.
- **Mensaje o tarea**: un agente puede responder con un `Message` directo
  para lo trivial y crear tarea solo al comprometerse (agente híbrido).
- **Cadena de peticiones**: solo `auth-required` tiene regla explícita (el
  cliente que es agente puede pasar su propia tarea a ese estado). Para
  `input-required` no existe regla equivalente: el flujo de
  `2026-10-21-codeloom-subagent-input-required-design.md` es **inferencia
  nuestra**, no mandato del protocolo.
- **Discrepancias de la propia documentación**: si el stream se cierra en
  `input-required` (lo afirma una página, no la especificación); un campo
  `index` en `artifactUpdate` (lo muestra una página, no la tabla
  normativa); y «devolver de inmediato» frente al bloqueo por defecto de
  `SendMessage` (prevalece este último).
- **Versionado**: cabecera `A2A-Version`; vacía equivale a `0.3`.

## 6. Frontera con el core

- El core define **traits puros**: `AgentProfile` (dato), `AgentRegistry`
  (catálogo: locales + remotos uniformados), `AgentTransport`.
- `codeloom-core` **no** depende de A2A, ni de HTTP, ni de red. Cumple AGENTS.md:
  motor honesto, integraciones en crates opcionales.
- **No red en tests, por construcción**: los tests usan `InProcess`; los casos
  remotos usan fixtures/cassettes de Agent Card, como hoy se mockea el provider.
- **Standalone funciona sin servidor**: sin endpoints, todo es `in-process`.

## 7. Relación con lo ya documentado

- **Subagentes (§6.1 del spec base)**: un agente `in-process` **es** un subagente;
  reusa `SubagentRuntime`, `persona`, `tool_filter` (derivado del scope) y
  `max_active_subagents`.
- **Puerta única (`tool-skill-discovery.md`)**: los agentes se resuelven con la
  **misma puerta** (`discover` capacidades / `delegate` agentes). Este spec **no**
  redefine la puerta, solo le da el catálogo y el transporte.
- **Contrato de control (§9 del mapa)**: la comunicación entre agentes es una
  **activación de primera clase** (`id` + `generation`, `who`, bus durable,
  replay), igual que skills/memoria/MCP.
- **Skills**: `AgentProfile` vive junto a las skills (`~/.agents`) y usa su mismo
  patrón de fichero.

## 8. Gotchas

- **No confundir card con service**: "tener card" = dato (siempre); "servir card" =
  rol (opcional). El error clásico es creer que un agente local necesita un servidor.
- **El scope no viaja en A2A**: si delegas a un remoto, **tú** no controlas su
  scope; confías en su card y en su gate. La frontera dura aplica **dentro** de
  cada agente, no a través del protocolo.
- **No dejar que el protocolo dicte el modelo interno**: `turn/step`, mensajes y
  `ToolResult` siguen siendo de CodeLoom; A2A es un **seam de traducción** en el
  borde.
- **Autenticación de remotos**: `securitySchemes` (OAuth2/mTLS) obligatorios para
  remotos reales; un remoto sin auth es un agente sin puerta.
- **Superficie de confianza**: una Agent Card es contenido de un tercero; validar y
  no ejecutar nada de ella sin pasar por el gate.
- **Sin servidor en tests**: prohibido bindear puertos; `InProcess` o cassettes.

## 9. Preguntas abiertas

- **Profile**: ¿frontmatter YAML (de-facto) o un `codeloom.toml` propio? ¿Se
  reutiliza el parser de skills?
- **Transporte por defecto**: ¿JSON-RPC sobre HTTP, o gRPC?
- **Card local**: ¿se sirve por defecto cuando el rol es `serve`, o solo bajo
  `both`?
- **gobernanza OSSA**: ¿se adopta el bloque `governance:` como extensión, o se
  modela a mano (más simple) sin depender de un standard inmaduro?
- **Descubrimiento remoto**: ¿se consultan Agent Cards por endpoint, o se usan
  **registries** (A2A los contempla)? ¿Cache de cards?
- **Compatibilidad de versiones**: ¿coexistir con `0.3` (default de A2A sin header)
  o exigir `1.0`?

## Referencias

- A2A: `github.com/a2aproject/A2A`; `a2a-protocol.org/latest/specification/`; SDK
  Rust `a2a-lf`.
- arXiv 2505.02279 (survey MCP/ACP/A2A/ANP), 2504.16736 (survey de protocolos),
  2506.19676 (seguridad de comunicación), 2504.16902 (A2A seguro con MAESTRO).
- Formatos de profile: Claude Code subagents (`.claude/agents/*.md`), Cursor
  (`.cursor/agents/*.md`), Copilot (`.github/agents/*.agent.md`), CrewAI `Agent`.
- OSSA (`openstandardagents.org`): gobernanza, referencia únicamente.
