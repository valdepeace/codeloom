# CodeLoom — Idea/spec: memoria (proyecto, general y otras)

> Estado: **idea en exploración**. No implementar.
>
> Fecha: 2026-10-14
>
> Relacionado: `docs/specs/2026-10-13-loom-design.md` (§2 config/contexto, §4.4
> sesión, §5.5 extensiones), `2026-10-14-loom-mcp-skills-hot-reload.md`,
> `2026-10-14-loom-decision-hub-jevalan.md` y
> `2026-10-14-loom-kpis-prompt-compression.md`.
> Fuentes: `codebase-memory` (grafo de código, MCP) y `caveman` (memoria
> `cavemem` + salud de memoria), en `~/.claude/skills` y `~/.agents/skills`.

## 0. La idea

Que CodeLoom tenga **memoria**: de **proyecto** (convenciones, decisiones,
trampas), **general** (preferencias y procedimientos del usuario, entre
proyectos) y, si hace falta, otros tipos (episódica/sesión, semántica/grafo de
código). Y que un **sistema de decisión** (jevalan) resuelva **cuándo cargar cada
memoria**, porque cargarla siempre cuesta tokens.

**¿Tiene sentido? Sí.** El problema real no es "recordar", es **cuándo pagar
tokens por recordar**. La decisión resuelve justo eso; sin ella, o cargas todo
(caro) o no cargas nada (y el agente es más tonto). El foco es el ahorro **sin
perder calidad**.

## 1. El principio rector: recordar por demanda, no por defecto

Una memoria que se inyecta en **cada turno** paga tokens **siempre** y solo
rinde en las sesiones que la usan (ese es el *dead_load* que el ecosistema
caveman penaliza). Por tanto:

- **Nada de cargar toda la memoria cada turno.** Se **rutea**: qué capa entra,
  según la tarea.
- Lo pesado se **aparca** en memoria compacta con un **puntero** y un camino de
  **recall**; el bloque original se recupera **byte a byte** cuando hace falta
  (patrón `caveman mem`: `recall` devuelve la forma compacta, `recover` el
  original exacto).
- **Gate net-token-negative**: un cambio de memoria solo se guarda si
  **tokens/turno después < antes**. Si no baja, se revierte.
- **No se borra memoria sin un camino de recall que funcione.** Es la única
  forma de ahorrar sin volver tonto al agente.

## 2. Tipos de memoria

| Tipo | Qué guarda | Fuente de referencia | Alcance |
|---|---|---|---|
| **Proyecto** | convenciones, arquitectura, trampas, decisiones | `AGENTS.md`/`MEMORY.md` + ficheros de tema | un repo |
| **General / usuario** | preferencias, estilo, procedimientos repetidos | memoria global (entre proyectos) | todas |
| **Episódica / sesión** | el historial del turno | `SessionStore` (§4.4, ya diseñado) | una sesión |
| **Semántica / grafo** | estructura del código (quién llama a quién) | `codebase-memory` (MCP) | un repo |

- **Proyecto**: CodeLoom ya menciona el contexto `AGENTS.md` en `config/contexto`
  (§2). Aquí se formaliza: índice de una línea por entrada + ficheros de tema
  enlazados (como `MEMORY.md`), con **límite de truncado** (lo que pasa del
  límite no se carga).
- **General**: lo que se aprende entre repos; se inyecta solo si la tarea lo
  pide, nunca por defecto.
- **Episódica**: ya prevista en el `SessionStore`; puede **destilarse** a
  proyecto/general (una decisión repetida -> una regla del proyecto).
- **Semántica/grafo** (`codebase-memory`): consultas estructurales ~500 tokens
  frente a ~80K de grep ("quién llama a X", impacto, código muerto). Es memoria
  **bajo demanda**: se consulta, no se inyecta entera. Es el servidor MCP
  `DeusData/codebase-memory-mcp` (**C**, binario estático sin dependencias,
  SQLite con FTS5/BM25, consultas tipo Cypher, MIT).

### 2.1 El modelo de Serena (y qué copiar)

Serena es **Python** (se instala con `uv`, es un **servidor MCP** con backend
**LSP**). Guarda la memoria como **ficheros markdown**: por proyecto en
`.serena/memories/` (versionados con el código) y **globales** entre proyectos
en `~/.serena/memories/global/`. Herramientas MCP: `write_memory`, `read_memory`,
`list_memories`, `delete_memory`, `rename_memory`.

El agente recibe **la lista de nombres** de memoria al arrancar y **lee bajo
demanda** lo que necesita (referencias por el prefijo `mem:`). En su
`.serena/project.yml` (visto en `jevalan`/`tipsy-assistant`) hay dos cosas que
CodeLoom debe copiar:

- Las memorias se cargan **bajo demanda**: el propio fichero dice *"contrary to
  the memories, which are loaded on demand"* (el `initial_prompt` sí es siempre
  dado al activar). Es exactamente el principio de §1.
- `read_only_memory_patterns` (marcar entradas como **solo lectura**) e
  `ignored_memory_patterns` (p. ej. `_archive/.*`, `_episodes/.*`: no aparecen en
  `list_memories` ni en el arranque del proyecto).

**¿Ahorra y da contexto? Sí, con matices:**

- **Da contexto**: son notas curadas y versionadas, evitan re-descubrir el código
  por exploración (que es lo caro).
- **Ahorra** solo si se leen **bajo demanda** y son **concisas**. Leerlas todas
  al activar ya cuesta; copiar "todas al inicio" lo haría mal, copiar "on-demand"
  lo hace bien.
- **Riesgo**: memoria **obsoleta** es peor que ninguna (desinforma). Necesita la
  salud de memoria de §5.

**Decisión**: para la **memoria de proyecto**, seguir el modelo de Serena
(markdown en el repo + tools MCP `read/write/list/delete`, bajo demanda, con
patrones de solo-lectura/ignorado). Serena decide **cómo se almacena**; el router
(jevalan) decide **qué se lee**. Para general/grafo/episódica, el diseño de
§2-§4.

### 2.2 codebase-memory vs Graphiti (grafos distintos)

Sí, se pueden comparar, pero **no son el mismo tipo de grafo** y se
complementan: uno modela la **estructura del código**, el otro los **hechos y su
evolución en el tiempo**.

| Eje | `codebase-memory` | `graphiti` (Zep) |
|---|---|---|
| Qué modela | estructura del código (CALLS, IMPORTS, ficheros) | hechos y su cambio temporal (episodios) |
| Fuente | análisis estático del repo | episodios: conversaciones, docs, decisiones |
| Lenguaje | **C** (binario estático) | **Python** |
| Backend | **SQLite** (FTS5/BM25 + Cypher-like) | **Neo4j/FalkorDB/Neptune** (Kuzu deprecado) |
| Coste | barato, **sin LLM** | LLM de extracción + embeddings + BD de grafo |
| Temporalidad | no (foto del código) | **sí** (provenencia, hechos que caducan) |
| MCP | sí | sí (carpeta `mcp_server`) |
| Licencia | MIT | Apache-2.0 |

Encaje en CodeLoom:

- **codebase-memory** = la memoria **semántica/estructural** (§2). Determinista,
  barata, encaja con "consultar, no inyectar".
- **graphiti** = candidato para la memoria **episódica/general**: un grafo
  temporal de lo aprendido (decisiones, preferencias, hechos que cambian), en
  lugar de (o además de) el `SessionStore` plano. Es **más caro** (LLM +
  embeddings + BD de grafo), así que va **detrás del router** y solo donde la
  temporalidad importa; no por defecto.
- Ambos son **MCP**: CodeLoom los consume por protocolo, el lenguaje de cada uno da
  igual.

### 2.3 codebase-memory vs LSP vs AST (¿chocan?)

No chocan; son **capas distintas**. Y `codebase-memory` **no usa un servidor
LSP**: indexa con **tree-sitter (AST de 162 lenguajes)** y resuelve tipos con un
resolutor **embebido** ("Hybrid LSP", una implementación propia en C, no un
language server). Es **estático**, sin runtime, y da a cada arista una
**confianza** (cascade de 6 estrategias: import/módulo/nombre único/sufijo/fuzzy).

| | LSP (servidor real) | `codebase-memory` (grafo) | AST / tree-sitter crudo |
|---|---|---|---|
| Qué da | tipos, referencias, diagnósticos, jerarquía de llamadas | estructura a escala, impacto, código muerto, arquitectura | sintaxis (nodos), sin tipos |
| Frescura | viva, siempre actual | snapshot, **re-indexar** | viva al parsear |
| Exactitud | **autoritativa** | por **confianza**, aproximada | aproximada |
| Coste | proceso por lenguaje, pesado | binario único + SQLite, barato | barato, sin resolver |
| Huecos | necesita language server por lenguaje | macros, dispatch dinámico y reflexión **no** representados; no guarda el código fuente | nada de semántica |

**Reparto en CodeLoom (precedencia, no competencia):**

- **LSP = autoritativo** cuando hay language server: definiciones, referencias y
  diagnósticos exactos, "qué se hace" con tipos. Es la fuente de verdad (m4 ya
  prevé LSP).
- **codebase-memory = primera pasada barata / consulta a escala**: impacto,
  código muerto, arquitectura, cuando importa el coste o no hay LSP. Si su
  respuesta y la del LSP discrepan, gana el LSP.
- **AST crudo = último recurso sintáctico** (lenguaje sin LSP o sin necesidad de
  tipos).

**Conflicto a gestionar**, no de herramienta sino de **coherencia**:

- **Estancamiento**: el grafo se queda atrás si no se re-indexa; LSP no.
- **No guarda líneas fuente**: el grafo da relaciones, no el texto; para
  contenido exacto se lee el fichero (`view`).
- **Confianza**: aceptar una arista de baja confianza (fuzzy 0.30) como si fuera
  un hecho es el error típico.
- **El nombre "Hybrid LSP" engaña**: no es un servidor LSP; no confundirlo al
  decidir recursos.

## 3. Router de memoria (jevalan)

La decisión de "¿qué memoria entra?" es una **clasificación barata**, no una
llamada cara:

- **Capacidad `memory.route`** (nueva, flujo jevalan): preguntas `noul`/`score`
  del tipo *"¿esta tarea necesita memoria de proyecto?"*, *"¿preferencias
  generales?"*, *"¿consultar el grafo de código?"*.
- Salida: el **conjunto de capas** a activar y su presupuesto de tokens.
- Reutiliza el mismo marco del hub de decisiones: la misma llamada de ~100 ms que
  enruta modelo y skills, enruta memoria.
- **Si jevalan no responde**: por defecto, **memoria de proyecto sí** (barata y
  casi siempre útil) y el resto no; nunca cargar todo a ciegas.

## 4. Mecánica en CodeLoom

- **`MemoryIndex` versionado** con watcher: editar `AGENTS.md`/`MEMORY.md` se
  reindexa **en caliente** (ver `2026-10-14-loom-mcp-skills-hot-reload.md`),
  aplicado en **frontera de step** (prompt cache).
- **Dos formas de guardar**: el **puntero compacto** (lo que entra al prompt) y
  el **original recuperable** (`recall`/`recover`), nunca solo el primero.
- **Capas por entrada**: `always` (raro), `on-demand` (lo normal), `never`.
- **Alcance**: proyecto, global y por subagente. Un crítico del Gauntlet no debe
  heredar la memoria (sesgo) que sí usa el builder.
- **Destilación**: `procedure_repeat` (un procedimiento repetido en muchas
  sesiones) es candidato a skill o a regla del proyecto, pero **el pago no está
  probado** -> se gradúa con un experimento, no se activa "porque sí".

## 5. Salud de la memoria (memory health)

La memoria se **enferma** y hay que curarla, o el ahorro se vuelve coste:

- **Reglas duplicadas** (la misma regla en dos ficheros) -> quedarse con la más
  específica.
- **Huérfanos** (ficheros que el índice no enlaza) y **enlaces muertos**.
- **Truncación** (`MEMORY.md` pasa del límite y sus últimas entradas no se ven)
  -> condensar el índice, mover detalle a ficheros de tema.
- **Imports rotos** (`@import` a nada) y **referencias obsoletas** (rutas que ya
  no existen).

## 6. KPIs y garantía de calidad

- Medir **tokens/turno por capa** y el **coste del recall**; un offload solo vale
  si el puntero + recall < el bloque que sustituye.
- **No reclamar un ahorro no medido**; distinguir el re-contado del fichero de un
  before/after sin control (son evidencias distintas, no se suman).
- **Contrapeso de calidad**: si una sesión más barata **falla más** (más errores
  de tool), no es un ahorro. Se mide junto con los KPIs de
  `2026-10-14-loom-kpis-prompt-compression.md`.

## 7. Config (borrador)

```toml
[memory]
hot_reload = true

[memory.project]
path = ["AGENTS.md", "MEMORY.md"]   # índice + ficheros de tema
default = true                      # barata; entra salvo que la decisión diga no

[memory.general]
enabled = true
mode = "on-demand"                  # always | on-demand | never

[memory.graph]                      # codebase-memory (MCP)
enabled = false
mode = "on-demand"

[memory.route]
decide = true                       # jevalan: memory.route
budget_tokens = 2000
```

## 8. Gotchas

- **Cargar todo siempre = dead_load**: paga tokens cada turno y casi nunca los
  usa. Rutear.
- **No borrar memoria sin recall**: sin `recover` que devuelva el original, un
  "ahorro" es una pérdida de conocimiento.
- **El grafo de código es consulta, no inyección**: pedir "quién llama a X"
  cuesta ~500 tokens; volcar el grafo, mucho más.
- **Prompt cache**: alta/baja de memoria cambia el prefijo; aplicar en frontera
  de step (igual que hot-reload y skills).
- **Herencia a subagentes**: la memoria es sesgo; el crítico no hereda la del
  builder.
- **La decisión de memoria no decide su contenido** (eso es de la capa de
  confianza): el router elige **qué capas** y con qué presupuesto.

## 9. Preguntas abiertas

- ¿La memoria general vive en el repo, en `~/.config/codeloom`, o en un servicio?
- ¿Cuándo se **destila** episódica a proyecto/general: al cerrar el turno, al
  cerrar la sesión, o por un comando?
- ¿El grafo (`codebase-memory`) entra como MCP (reutiliza hot-reload de MCP) o
  como crate?
- ¿Quién firma "esto es una regla del proyecto": el usuario, el modelo, o el
  router?
- ¿Se soporta el formato ecosistema (`MEMORY.md`, `@import`) para no reinventar,
  o uno propio?
