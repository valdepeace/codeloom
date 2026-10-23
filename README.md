# CodeLoom

Agente de código en **Rust** con TUI **ratatui**, cuyo modelo de orquestación
está **basado en [deepseek-harness (dsh)](https://github.com/deepseek-ai/deepseek-harness)**:
agente -> subagentes -> jobs, con su representación en el header.

CodeLoom reutiliza el **diseño** de dsh pero mejora la **ejecución** aprovechando el
paralelismo real de Rust (ver spec, §11). Se integra opcionalmente con
[herdr](https://herdr.dev) como plugin, sin parchearlo, y también corre
standalone.

Cabe señalar, además, su premisa: el modelo de agentes parte del protocolo
**[A2A](https://a2a-protocol.org/latest/definitions/)** y no se inventa nada propio
donde el estándar ya dice algo. Lo que es decisión nuestra se rotula como tal.

## Estado

Hay código y funciona: el espacio de trabajo compila, tiene tests y se usa a diario.
Ciertamente, falta mucho (véase «Lo que aún no está»), pero ya no es solo diseño.

**Lo que ya hace:**

- Bucle de agente con proveedores compatibles con OpenAI (p. ej. OpenRouter), herramientas
  de lectura en paralelo y de escritura en exclusiva, y permisos con diálogo.
- Subagentes lanzados como tareas **A2A**: los ocho estados del protocolo
  (`submitted`, `working`, `input-required`, `auth-required`, `completed`, `failed`,
  `canceled`, `rejected`), cancelación, tareas enlazadas (`reference_task_ids`),
  preguntas del hijo al padre y despertar del padre.
- Registro de agentes con fuentes intercambiables: perfiles `.md` propios
  (global en `~/.agents/agents/`, por proyecto en el repo) y **agentes remotos A2A**
  (tarjeta descargada, aprobación explícita desde `/agents`, delegación por JSON-RPC).
- TUI con conversación en árbol, bloques largos plegables, mensajes con Markdown,
  tarjeta de herramienta, panel de ficheros modificados, medidor de contexto,
  `/compact` y compactación anticipada en segundo plano.
- Control de contexto y coste: toda salida de herramienta, sección del prompt y bucle
  lleva una cota (véase el decálogo, más abajo).

**Lo que aún no está:** servir CodeLoom como agente A2A (A8b), OAuth2/OpenID Connect/mTLS
contra remotos (el resto de A8d), flujo SSE (A8c), MCP, LSP, skills y memoria (m4), y el plugin completo de
herdr (m3; hoy solo se detecta el entorno y se le reporta el estado).

## Puesta en marcha

Requiere Rust (la versión está fijada en `rust-toolchain.toml`).

```bash
cargo build
cargo run -p codeloom-cli --bin loom
```

La configuración vive en `codeloom.toml`; el secreto del proveedor, en `.env`
(nunca en el fichero de configuración). Un ejemplo mínimo:

```toml
model = "anthropic/claude-haiku-5.5"
max_steps = 8

[provider]
kind = "openai_compat"
base_url = "https://openrouter.ai/api/v1"
api_key_env = "OPENROUTER_API_KEY"

# Consumir agentes remotos por A2A (por defecto desactivado).
[a2a]
role = "consume"
```

Comprobaciones exigidas en cada commit:

```bash
cargo build
cargo test
cargo fmt --check
cargo clippy -- -D warnings
```

## Agentes remotos por A2A

1. Crear un perfil `.md` con `transport: a2a-remote` y `a2a.endpoint` en
   `~/.agents/agents/` (o en la carpeta de agentes del proyecto).
2. Al arrancar con `[a2a] role = "consume"`, CodeLoom descarga la tarjeta del agente.
3. En `/agents`, la tecla `a` aprueba y `r` revoca. La aprobación queda atada al
   *endpoint* y al *hash* de la tarjeta: si esta cambia, hay que aprobar de nuevo.
4. El padre ya puede delegar en él; la tarea aparece como subagente con su estado A2A.

La tarjeta de un tercero es **dato, no instrucción**: nunca se registra sin aprobación
y no hay descubrimiento automático.

**Remotos que piden autenticación.** CodeLoom sabe dar clave de API (en cabecera o en
consulta) y HTTP `Bearer`/`Basic`. La credencial nunca se escribe en el perfil: se declara
el *nombre* de la variable de entorno que la guarda, por esquema de la tarjeta:

```yaml
transport: a2a-remote
a2a:
  endpoint: https://agente.example
  credentials:
    token: AGENTE_EJEMPLO_TOKEN   # esquema de la tarjeta -> variable de entorno
```

Si falta la variable, o el remoto solo ofrece OAuth2, OpenID Connect o mTLS (aún sin
soporte), la fila del agente en `/agents` dice por qué no se puede usar y no se le envía
nada.

## Nomenclatura

| Capa | Nombre |
|---|---|
| Producto | **CodeLoom** (el telar: teje turnos, subagentes y jobs) |
| crates.io | `codeloom-core`, `codeloom-cli`, `codeloom-tui` |
| Binarios / comandos | `loom` y `codeloom` |
| Integración con herdr | crate `codeloom-herdr`, plugin `codeloom.herdr` (acciones `codeloom.herdr.*`) |
| Config | `codeloom.toml` |
| Conjunto de subagentes | **"flock"** (lo que CodeLoom muestra en su header) |

> `loom` es el comando corto de la misma marca CodeLoom (como `code` para VS Code).
> El nombre del *crate* debe ser único en crates.io; el del *binario*, no.

## Arquitectura

```
crates/
  codeloom-core/   motor (sin herdr ni ratatui): provider, message, tools, agent,
                   subagent, job, event, session, config, registro de agentes
  codeloom-a2a/    borde de A2A: nombres del cable v1.0, tarjeta, cliente JSON-RPC,
                   configuración [a2a]
  batten/          biblioteca de widgets y estado de la TUI (ratatui), sin lógica de agente
  codeloom-tui/    la interfaz: conversación, header de subagentes, paneles, popups
  codeloom-herdr/  integración opcional (detección + cliente socket)
  codeloom-cli/    binarios `loom` y `codeloom`
```

Regla dura: `codeloom-core` **no** depende de `ratatui` ni de `crossterm`; el motor es
una biblioteca pura y comprobable.

## Documentación

El diseño vive en `docs/`: cada tarea lleva su spec (`docs/specs/`) y su plan
(`docs/plans/`). Por dónde empezar:

- Spec de diseño: [`docs/specs/2026-10-13-loom-design.md`](docs/specs/2026-10-13-loom-design.md)
- Mapa de piezas y arquitectura:
  [`docs/specs/2026-10-14-loom-architecture-overview.md`](docs/specs/2026-10-14-loom-architecture-overview.md)
- Índice de A2A y tareas pendientes:
  [`docs/plans/2026-10-22-codeloom-a2a-roadmap.md`](docs/plans/2026-10-22-codeloom-a2a-roadmap.md)
- Perfil de agente y A2A:
  [`docs/specs/2026-10-15-loom-agent-profile-a2a.md`](docs/specs/2026-10-15-loom-agent-profile-a2a.md)
- Decálogo de contexto y coste:
  [`docs/specs/2026-10-22-codeloom-decalogo-de-contexto-y-coste.md`](docs/specs/2026-10-22-codeloom-decalogo-de-contexto-y-coste.md)
- Autoimplementación (bootstrap y dogfooding):
  [`docs/specs/2026-10-14-loom-self-hosting-bootstrap.md`](docs/specs/2026-10-14-loom-self-hosting-bootstrap.md)
- Análisis de referencia (herdr, crush, dsh):
  [`docs/reference/analisis-herdr-crush-dsh.md`](docs/reference/analisis-herdr-crush-dsh.md)

Ideas aún sin implementar (son diseño, no promesa):

- Carga en caliente de MCP y skills:
  [`docs/specs/2026-10-14-loom-mcp-skills-hot-reload.md`](docs/specs/2026-10-14-loom-mcp-skills-hot-reload.md)
- Entrada por voz al prompt:
  [`docs/specs/2026-10-14-loom-voice-prompt-input.md`](docs/specs/2026-10-14-loom-voice-prompt-input.md)
- Resumen del último turno (1-3 frases, texto + voz):
  [`docs/specs/2026-10-14-loom-turn-summary.md`](docs/specs/2026-10-14-loom-turn-summary.md)
- Hub de decisiones local (`jevalan` + Kev/Laya/Jev):
  [`docs/specs/2026-10-14-loom-decision-hub-jevalan.md`](docs/specs/2026-10-14-loom-decision-hub-jevalan.md)
- Gauntlet Loop (builder + crítico ciego):
  [`docs/specs/2026-10-14-loom-gauntlet-loop.md`](docs/specs/2026-10-14-loom-gauntlet-loop.md)
- Skills (`ponytail`, `superpowers`) y su arbitraje:
  [`docs/specs/2026-10-14-loom-skills-ponytail-superpowers.md`](docs/specs/2026-10-14-loom-skills-ponytail-superpowers.md)
- KPIs y compresión de prompts/contexto (`headroom`, `rtk`, `caveman`):
  [`docs/specs/2026-10-14-loom-kpis-prompt-compression.md`](docs/specs/2026-10-14-loom-kpis-prompt-compression.md)
- Memoria (proyecto, general, episódica, grafo):
  [`docs/specs/2026-10-14-loom-memory.md`](docs/specs/2026-10-14-loom-memory.md)
- Plan de experimentos de decisión (cascades, routing, gate de riesgo):
  [`docs/specs/2026-10-14-loom-decision-experiments.md`](docs/specs/2026-10-14-loom-decision-experiments.md)
- Capa estática de decisión (señales + reglas, sin LLM):
  [`docs/specs/2026-10-14-loom-static-decision-signals.md`](docs/specs/2026-10-14-loom-static-decision-signals.md)
- Una sola puerta de descubrimiento (tools/skills/MCP bajo demanda):
  [`docs/specs/2026-10-14-loom-tool-skill-discovery.md`](docs/specs/2026-10-14-loom-tool-skill-discovery.md)
