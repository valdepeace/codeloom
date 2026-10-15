# CodeLoom

Agente de código en **Rust** con TUI **ratatui**, cuyo modelo de orquestación
está **basado en [deepseek-harness (dsh)](https://github.com/deepseek-ai/deepseek-harness)**:
agente -> subagentes -> jobs, con su representación en el header.

CodeLoom reutiliza el **diseño** de dsh pero mejora la **ejecución** aprovechando el
paralelismo real de Rust (ver spec, §11). Se integra opcionalmente con
[herdr](https://herdr.dev) como plugin, sin parchearlo, y también corre
standalone.

## Estado

Diseño. Sin implementación todavía.

- Spec de diseño: [`docs/specs/2026-10-13-loom-design.md`](docs/specs/2026-10-13-loom-design.md)
- Mapa de piezas y arquitectura propuesta:
  [`docs/specs/2026-10-14-loom-architecture-overview.md`](docs/specs/2026-10-14-loom-architecture-overview.md)
- Spec de carga en caliente de MCP y skills:
  [`docs/specs/2026-10-14-loom-mcp-skills-hot-reload.md`](docs/specs/2026-10-14-loom-mcp-skills-hot-reload.md)
- Idea de entrada por voz al prompt (dictado):
  [`docs/specs/2026-10-14-loom-voice-prompt-input.md`](docs/specs/2026-10-14-loom-voice-prompt-input.md)
- Idea de resumen del último turno (1-3 frases, texto + voz):
  [`docs/specs/2026-10-14-loom-turn-summary.md`](docs/specs/2026-10-14-loom-turn-summary.md)
- Idea de hub de decisiones local (`jevalan` + Kev/Laya/Jev) para ahorrar
  llamadas al LLM:
  [`docs/specs/2026-10-14-loom-decision-hub-jevalan.md`](docs/specs/2026-10-14-loom-decision-hub-jevalan.md)
- Idea de Gauntlet Loop (builder + crítico ciego), activable por caso:
  [`docs/specs/2026-10-14-loom-gauntlet-loop.md`](docs/specs/2026-10-14-loom-gauntlet-loop.md)
- Idea de consumo/activación de skills (`ponytail`, `superpowers`) y su
  arbitraje:
  [`docs/specs/2026-10-14-loom-skills-ponytail-superpowers.md`](docs/specs/2026-10-14-loom-skills-ponytail-superpowers.md)
- Idea de KPIs y compresión de prompts/contexto (`headroom`, `rtk`, `caveman`):
  [`docs/specs/2026-10-14-loom-kpis-prompt-compression.md`](docs/specs/2026-10-14-loom-kpis-prompt-compression.md)
- Idea de memoria (proyecto, general, episódica, grafo), enrutada por jevalan:
  [`docs/specs/2026-10-14-loom-memory.md`](docs/specs/2026-10-14-loom-memory.md)
- Plan de experimentos (qué probar de los papers: cascades, routing, gate de
  riesgo):
  [`docs/specs/2026-10-14-loom-decision-experiments.md`](docs/specs/2026-10-14-loom-decision-experiments.md)
- Autoimplementación (bootstrap y dogfooding):
  [`docs/specs/2026-10-14-loom-self-hosting-bootstrap.md`](docs/specs/2026-10-14-loom-self-hosting-bootstrap.md)
- Idea de capa estática de decisión (señales + reglas + prioridad, sin LLM):
  [`docs/specs/2026-10-14-loom-static-decision-signals.md`](docs/specs/2026-10-14-loom-static-decision-signals.md)
- Idea de una sola puerta de descubrimiento (tools/skills/MCP bajo demanda):
  [`docs/specs/2026-10-14-loom-tool-skill-discovery.md`](docs/specs/2026-10-14-loom-tool-skill-discovery.md)
- Análisis de referencia (herdr, crush, dsh):
  [`docs/reference/analisis-herdr-crush-dsh.md`](docs/reference/analisis-herdr-crush-dsh.md)

## Idea en una línea

Mismo modelo que dsh (subagentes + jobs + header), stack Rust/ratatui,
multi-provider (OpenAI-compatible, Anthropic, OpenRouter), y opcionalmente
hospedado por herdr.

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

## Arquitectura prevista

```
crates/
  codeloom-core/   motor (sin herdr ni ratatui): provider, message, tools, agent,
                   subagent, job, event, session, config
  codeloom-tui/    ratatui: chat + header de subagentes/jobs
  codeloom-herdr/  integración opcional (detección + cliente socket)
  codeloom-cli/    binarios `loom` y `codeloom`
```
