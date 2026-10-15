# CodeLoom — Arquitectura propuesta: mapa de piezas

> Estado: **borrador para revisión**.
>
> Fecha: 2026-10-14
>
> Este documento es el **mapa**. Cada pieza tiene su spec; aquí se ve cómo encajan.

## 0. Índice de specs

| Pieza | Spec |
|---|---|
| Diseño base (crates, concurrencia, turn/step, tools, permisos) | `2026-10-13-loom-design.md` |
| Carga en caliente de MCP y skills | `2026-10-14-loom-mcp-skills-hot-reload.md` |
| Entrada por voz al prompt | `2026-10-14-loom-voice-prompt-input.md` |
| Resumen del último turno | `2026-10-14-loom-turn-summary.md` |
| Hub de decisiones (jevalan) | `2026-10-14-loom-decision-hub-jevalan.md` |
| Capa estática de señales/decisiones (hallazgo semantic-router) | `2026-10-14-loom-static-decision-signals.md` |
| Gauntlet Loop | `2026-10-14-loom-gauntlet-loop.md` |
| Skills (ponytail / superpowers) | `2026-10-14-loom-skills-ponytail-superpowers.md` |
| KPIs y compresión | `2026-10-14-loom-kpis-prompt-compression.md` |
| Memoria | `2026-10-14-loom-memory.md` |
| Plan de experimentos | `2026-10-14-loom-decision-experiments.md` |

## 1. Modelo mental: cinco capas

1. **Presentación** — la TUI y sus entradas/salidas (teclado, herdr, voz).
2. **Motor** — `codeloom-core`: el bucle turn/step, providers, tools, subagentes,
   jobs, sesión y bus. No conoce hosts ni integraciones.
3. **Decisión** — un solo seam para «¿qué toca?» y «¿cuál concreto?», en **tres
   escalones** (§4).
4. **Conocimiento y efecto** — tools, MCP, memoria, skills, compresión. Lo que
   aporta contexto y ejecuta.
5. **Transversal** — KPIs, hot-reload, config y el laboratorio de evaluación.

Regla de oro heredada del spec base: **el motor decide, las tools ejecutan**, y
todo lo opcional (host, voz, decisión, compresión) vive **fuera de `codeloom-core`**.

## 2. Diagrama de crates

```
   usuario · voz · texto
        │
        ▼
┌───────────────────────────────────────────────────────────┐
│ codeloom-tui (ratatui): composer · transcript · header (flock) │
│   overlays: permiso · modelo · subagente/job               │
└───────────────┬───────────────────────────▲───────────────┘
     input (mpsc)│                           │watch (deltas)
                ▼                           │
┌───────────────────────────────────────────────────────────┐
│                        codeloom-core                          │
│  agent (turn/step) · provider (openai/anthropic) · tools   │
│  subagent · job · event bus (durable+live) · session       │
│  config/contexto · skills · memoria · permisos             │
└──┬───────────┬───────────┬───────────┬───────────┬────────┘
   │           │           │           │           │
┌──▼───────┐┌──▼───────┐┌──▼───────┐┌──▼──────┐┌───▼────────┐
│loom-     ││loom-     ││loom-     ││loom-    ││ (MCP: tools│
│decide    ││voice     ││compress  ││herdr    ││  y memoria)│
│(router)  ││(dictado) ││(contexto)││(host)   ││            │
└──┬───────┘└──┬───────┘└──┬───────┘└──┬──────┘└───┬────────┘
   │           │           │           │           │
┌──▼────────┐┌─▼─────────┐┌▼─────────┐┌─▼───────┐┌──▼──────────────┐
│ jevalan   ││tipsy-     ││headroom  ││herdr    ││codebase-memory  │
│ señales+  ││assistant  ││rtk       ││(PTY,    ││serena · graphiti │
│ decision  ││(voz)      ││caveman   ││panes)   ││servidores MCP     │
└──┬────────┘└───────────┘└──────────┘└─────────┘└──────────────────┘
   │
┌──▼───────────────┐
│ Kev · Laya · Jev │  modelos de decisión (locales + nube)
└──────────────────┘
```

`codeloom-cli` es el binario: une todo y **autodetecta** el modo (standalone o
plugin de herdr). `codeloom-decide`, `codeloom-voice`, `codeloom-compress` y `codeloom-herdr`
son **opcionales**; el core no los conoce.

## 3. Flujo de un turno (de punta a punta)

```
tecla/voz ─▶ composer ─▶ ┌─ resolver (frontera de step) ───────────────┐
                         │  memory.route / skill.route / model.route   │
                         │  discovery: Tier 0 + subset bajo demanda     │
                         │  (escalón 1 en proceso · escalón 2 jevalan) │
                         └───────────────────┬─────────────────────────┘
                                             ▼
                       agent.step: build request (system + skills + memoria + tools)
                                             │
                       provider.stream (SSE) ─┴─▶ deltas a la UI
                                             │
                       tool_calls ─▶ gate tool.risk (jevalan) ─▶ ejecutar tool
                                             │        (bash filtrado por rtk/headroom)
                                             ▼
                       tool results ─▶ compresión (headroom) ─▶ mensaje durable
                                             │
                       ¿tool_calls? sí ─▶ nuevo step · no ─▶ turn_end
                                             │
                       resumen del turno (1-3 frases) ─▶ transcript + voz
```

Puntos donde ya está documentado un invariante:

- Los cambios (memoria/skills/MCP) se aplican **en frontera de step**, nunca a
  mitad de stream (prompt cache). Ver hot-reload, skills, memoria.
- El **gate de permisos** (jevalan `tool.risk`) nunca da `allow` a ciegas.
- Los `ToolResult` son **durables**; la compresión no debe perder el original
  (recuperable).

## 4. La capa de decisión: tres escalones

```
   petición
      │
 ┌────▼─────────────────────────────┐   escalón 1 · EN PROCESO (CodeLoom)
 │ señales heurísticas del request   │   keyword · language · structure ·
 │ (deterministas, coste ~0)         │   context · conversation · metadata
 └────┬─────────────────────────────┘
      │ (lo trivial ya se decide aquí; no viaja al daemon)
 ┌────▼─────────────────────────────┐   escalón 2 · JEVALAN (stateless, texto)
 │ señales aprendidas + decisión     │   classifier · domain · complexity ·
 │ projections (bandas) + prioridad  │   pii · jailbreak · safety · reask · kb
 │                                   │   -> decisión tipada + clase/candidatos
 └────┬─────────────────────────────┘
      │ "qué toca" (capacidad, clase, effort, set de candidatos)
 ┌────▼─────────────────────────────┐   escalón 3 · LOOM (selección/serving)
 │ selección concreta + afinidad de  │   ranking (hybrid/score) · cache affinity
 │ cache + coste/KPIs + routing      │   acotada · coste · -> backend concreto
 └────┬─────────────────────────────┘
      │ "cuál concreto y cómo"
 ┌────▼─────────────────────────────┐
 │ EFECTO: llamada al proveedor/tool │
 └──────────────────────────────────┘
```

Frontera (de `decision-hub-jevalan.md` §12): **jevalan = «qué toca»; CodeLoom = «cuál
concreto y cómo»; lo trivial se decide en proceso.**

## 5. Conocimiento y gobernanza

| Pieza | Qué hace | Dónde vive | On-demand |
|---|---|---|---|
| **Skills** | cómo se trabaja (ponytail/superpowers) | fuera del repo (~/.agents) | sí, por `skill.route` |
| **Memoria** | qué se sabe (proyecto/general/episódica/grafo) | repo (Serena) + global | sí, por `memory.route` |
| **MCP** | tools adicionales (+ memoria vía MCP) | servidores externos | carga en caliente |
| **Compresión** | reducir tokens sin perder calidad | headroom/rtk/caveman | por configuración |
| **Gauntlet** | calidad contra una barra (builder+crítico) | patrón sobre subagentes | por caso |

Todo comparte el mismo contrato: **cargar por demanda, medir y poder podarlo.**

### 5.1 Una sola puerta de descubrimiento (por qué escala)

Con **muchos MCP y muchas skills**, cargar todo cada turno es carísimo y diluye
la elección. CodeLoom expone **una sola puerta de descubrimiento** y el **revelado
progresivo** hace el resto:

```
  Tier 0 (siempre): view · edit · bash · glob · grep  +  discovery
        │
   discovery("qué necesito")  ── estático (keyword) → resuelve ya
        │                       └─ jevalan → si no, decide
        ▼  carga SOLO el subconjunto
  Tier 1 (bajo demanda): tools MCP · skills · memoria
```

- La puerta se resuelve **en el mismo resolver de frontera de step** que
  skills/memoria/modelo; emite `discovery/resolved { loaded: [...] }`.
- **Estático primero** (coste ~0), **jevalan** después: el mismo reparto de §4.
- Efecto: `N schemas en cada turno` -> `1 puerta + un puñado`. Ahorra tokens
  **indirectamente** y evita la dilución. Detalle:
  `2026-10-14-loom-tool-skill-discovery.md`.

## 6. Transversales

- **KPIs**: provider, modelo, effort, capas activas, compresión, decisiones
  jevalan, calidad. Alimentan decisiones y el laboratorio.
- **Hot-reload**: `codeloom.toml`, MCP, skills y memoria se recargan en caliente,
  aplicados en frontera de step.
- **Laboratorio (`jevalan lab`)**: el harness de evaluación (offline/record/live,
  fixtures, cassettes, Brier) que decide qué capacidad/umbral se adopta.
- **Experimentos**: P0 (errores correlacionados) -> P1 (tool.risk sombra) ->
  P2 (model.route) -> P3 (cascade Gauntlet) -> P4 (routing por step).

## 7. Pieza -> repositorio

| Pieza | Repo | Lenguaje |
|---|---|---|
| CodeLoom (motor, TUI, CLI, crates opcionales) | `codeloom` | Rust |
| jevalan (hub de decisiones / señales) | `jevalan` | Rust |
| Modelos de decisión Kev / Laya | `G:\dev\jev` | Python/servidores |
| Jev (System One) | nube (OpenRouter/TypeSafe) | — |
| tipsy-assistant (voz) | `tipsy-assistant` | Rust |
| headroom (compresión de contexto) | `headroomlabs-ai/headroom` | Rust |
| rtk (compresión de salida de comandos) | `rtk-ai/rtk` | Rust |
| codebase-memory (grafo de código) | `DeusData/codebase-memory-mcp` | C |
| serena (memorias) | `oraios/serena` | Python |
| graphiti (grafo temporal) | `getzep/graphiti` | Python |
| herdr (host) | `herdr` | Rust |

## 8. Qué es «el núcleo» y qué es «el borde»

- **Núcleo (`codeloom-core`)**: bucle turn/step, providers, tools, subagentes, jobs,
  sesión, bus, config. Testeable, sin hosts ni integraciones.
- **Borde (crates opcionales)**: host (herdr), voz, decisión (jevalan), compresión,
  MCP, y todo lo que se consume por URL/MCP.
- **Fuera de CodeLoom del todo**: el serving router de modelos (`hybrid` + afinidad
  de cache es del lado de CodeLoom o de un proxy; jevalan no lo hace), la
  infraestructura de embeddings, y los servidores de los proveedores.

El resultado buscado: un motor honesto y rápido, con **decisiones baratas y
medibles** y **contexto cargado por demanda**, donde cada pieza se puede prender,
apagar o reemplazar sin tocar el núcleo.

## 9. Control de activaciones (transversal)

Toda pieza opcional (gauntlet, skills, decide, voz, compresión, memoria) se
activa y desactiva, y todas comparten el mismo **contrato de control**:

1. **La activación es estado de primera clase**, no una variable oculta. Tiene
   `id` y `generation`, vive en la **sesión** y en el **bus durable**, y registra
   **quién** la activó (`user | lead | router | subagent`).
2. **Se resuelve en frontera de step.** Un step posterior **detecta** qué está
   activo leyendo ese estado (no adivina); el log durable lo **replaya**. Mismo
   patrón que hot-reload, skills y memoria.
3. **Es observable**: header (flock) + eventos en vivo (`gauntlet/started`,
   `skills/resolved`, `mcp/server-state`, `tool/registry-changed`…).
4. **Es controlable**: `off | on | explicit | auto` por pieza en `codeloom.toml`,
   con presupuesto y cancelación (`CancellationToken`).
5. **Anidamiento con guardia**: cada subagente solo usa lo de su `tool_filter`, y
   hay `max_depth` (def. 1) y `max_active_subagents` (def. 8). Un crítico no abre
   otro loop (ver `gauntlet-loop.md` §4.1).
6. **Es medible**: toda activación emite KPIs (coste, latencia, calidad) para
   decidir si mereció la pena.

Con esto, la pregunta "¿se activó? ¿lo veo? ¿puede un subagente entrar?" tiene
siempre la misma respuesta: **está en el estado y en los eventos, con profundidad
acotada**.

## 10. Estado de las ideas

Registro para no perder el hilo. Todo queda **apuntado en specs**; nada
implementado.

| Idea | Estado | Spec | Experimento |
|---|---|---|---|
| Diseño base (crates, turn/step, tools) | base | `2026-10-13-loom-design.md` | — |
| Carga en caliente de MCP/skills | borrador | `...-mcp-skills-hot-reload.md` | — |
| Hub de decisiones (jevalan) | borrador | `...-decision-hub-jevalan.md` | P1/P0 |
| Capa estática de señales/decisiones | borrador | `...-static-decision-signals.md` | P0 |
| Puerta única de descubrimiento (tools/skills/MCP) | idea | `...-tool-skill-discovery.md` | — |
| Gauntlet Loop (+ control §4.1) | borrador | `...-gauntlet-loop.md` | P3 |
| Skills (ponytail/superpowers) | borrador | `...-skills-ponytail-superpowers.md` | — |
| KPIs y compresión | idea | `...-kpis-prompt-compression.md` | P2/P4 |
| Memoria | idea | `...-memory.md` | — |
| Voz al prompt | idea | `...-voice-prompt-input.md` | — |
| Resumen del turno | idea | `...-turn-summary.md` | — |
| Plan de experimentos | plan | `...-decision-experiments.md` | P0-P4 |
| Autoimplementación (bootstrap/dogfooding) | evaluación | `...-self-hosting-bootstrap.md` | — |
| Mapa de arquitectura (este) | borrador | `...-architecture-overview.md` | — |
| Lado de jevalan (señales/projections/decisiones) | propuesta | `jevalan/docs/superpowers/specs/2026-10-14-...` | — |

**Próximo paso recomendado**: **P0** (medir si los errores son complementarios o
correlacionados) -> **P1** (`tool.risk` en modo sombra). Barato, sin riesgo, y
decide el alcance de todo lo demás.

## 11. Orden de implementación (del núcleo hacia fuera)

De dentro hacia fuera: primero lo que sostiene todo, luego lo que se apoya en
ello; lo **más experimental, al final** y detrás de validación.

| # | Pieza / spec | Capa | Depende de | Fase | Riesgo |
|---|---|---|---|---|---|
| 0 | `loom-design` (base) | núcleo | — | m0 | — |
| 1 | `loom-mvp` (plan m0+m1) | núcleo | 0 | m1 | bajo |
| 2 | `self-hosting-bootstrap` (+ `AGENTS.md`) | núcleo | 1 | m1 | bajo |
| 3 | `architecture-overview` (mapa; meta vivo) | meta | — | — | — |
| 4 | `turn-summary` | núcleo ampliado | 1 | m1 | bajo |
| 5 | `kpis` (medición) | transversal | 1 | m1 | bajo |
| 6 | `mcp-skills-hot-reload` | núcleo ampliado | 1 | m4 | medio |
| 7 | `tool-skill-discovery` (puerta única) | núcleo ampliado | 6 | m4 | medio |
| 8 | `static-decision-signals` (estático) | decisión | 1 | m4 | medio |
| 9 | `decision-hub-jevalan` (seam + integración) | decisión | 8 | m4 | medio |
| 10 | `decision-experiments` **P0 y P1** | decisión | 9 | m4 | bajo |
| 11 | `memory` (proyecto, modelo Serena) | conocimiento | 1 | m4 | medio |
| 12 | `compression` (headroom/rtk) | transversal | 5 | m4 | medio |
| 13 | `gauntlet-loop` | orquestación | 1 (subagentes m2) | m2 | **alto** |
| 14 | `voice-prompt-input` | borde | 1 | — | bajo |
| 15 | experimentos **P2-P4** (model.route, cascade Gauntlet, step) | decisión | 10,13 | — | **alto** |
| 16 | partes experimentales: **graphiti** (memoria temporal), **`hybrid` + afinidad de cache**, **embeddings** (semantic-router) | varios | 11,12,8 | — | **alto** |

Lectura rápida:

- **Primero el núcleo** (0-3): sin motor, TUI, tools y un `AGENTS.md`, no hay
  nada.
- **Luego lo barato y de alto valor** (4-5): resumen y **medir** desde el
  principio; sin KPIs no se valida nada.
- **La decisión, de dentro hacia fuera** (6-10): registro -> discovery -> capa
  estática -> hub -> **experimentos P0/P1** (que deciden si el resto se sostiene).
- **Conocimiento y compresión** (11-12): memoria por demanda y reducción de
  tokens, ya sobre hombros firmes.
- **Lo más experimental, al final** (13-16): el **Gauntlet** (método no probado,
  P3), el routing por step (P4), y los grafos/embeddings pesados (`graphiti`,
  `hybrid`+cache, semantic-router). Todo detrás de validación con el lab.

**Regla de no-contradicción con los milestones**: respeta m0 -> m1 -> m2 -> m3 ->
m4 del spec base; aquí solo se dice **en qué orden dentro de cada fase** y qué se
pospone a "experimental".
