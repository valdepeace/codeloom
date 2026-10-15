# CodeLoom — Idea/spec: KPIs y compresión de prompts/contexto

> Estado: **idea en exploración**. No implementar.
>
> Fecha: 2026-10-14
>
> Relacionado: `docs/specs/2026-10-13-loom-design.md` (§4.4 bus/sesión, §4.3
> step-loop, §5.5 extensiones), `2026-10-14-loom-decision-hub-jevalan.md` (el lab
> y sus métricas), `2026-10-14-loom-skills-ponytail-superpowers.md` (skills como
> caveman).
>
> Fuentes de las herramientas de compresión (verificadas por web, **revisar el
> repo directo antes de integrar**): `headroomlabs-ai/headroom` y `rtk-ai/rtk`
> (ambos en Rust).

## 0. La idea

Dos piezas que van juntas:

1. **KPIs**: medir de verdad **provider, modelo, effort** y el resto de ejes
   (skills, gauntlet, decisiones) para no optimizar a ciegas ni elegir a ojo.
2. **Compresión**: **Headroom** (lo que el agente *lee*: salidas de tools, logs,
   RAG, ficheros, historial), **RTK** (la salida de *comandos*), y **Caveman**
   (la *prosa*). Reducir tokens sin romper la calidad.

Ambas encajan con lo ya documentado: jevalan ya mide en su laboratorio y CodeLoom
tiene bus durable + sesión para agregar.

## 1. KPIs: qué medir

Ejes (dimensiones):

- **Provider / modelo** (adapter, id, versión).
- **Effort / reasoning budget** (nivel de razonamiento pedido).
- **Capas activas**: skills (`ponytail`/`superpowers`), gauntlet on/off, decide
  on/off, compresión on/off.

Métricas por turno y por step (agregables):

| Métrica | Para qué |
|---|---|
| TTFT y latencia total | comparar providers/modelos |
| Tokens in / out / **cached** | coste real; el cacheado es la palanca |
| Coste estimado | presupuesto por tarea |
| Nº de steps, tool-calls, reintentos | fricción del loop |
| **Ratio de compresión** (antes/después) | medir Headroom/RTK |
| Decisiones jevalan (allow/ask/deny, escalados) | calibrar umbrales |
| Calidad: tests pasan, verificación, veredicto del crítico | que la calidad no cayó |

Dónde vive: `SessionStore` (SQLite, ya previsto §4.4) + un agregado consultable;
eventos en el bus; export (JSON/CSV) para análisis. El header puede mostrar lo
vivo (tokens, coste, compress ratio).

Regla del laboratorio de jevalan, aplicable: **distinguir el acierto de una
decisión de la calidad final**; una métrica cómoda puede ocultar el fallo (el
informe del Gauntlet lo muestra: 94 fps con el juego injugable).

## 2. Compresión de contexto (entrada) — Headroom

`headroomlabs-ai/headroom` (~75k estrellas, **Rust** + SDKs Python/TS,
Apache-2.0). Comprime **lo que el agente lee** antes de que llegue al LLM:
salidas de tools, logs, chunks de RAG, ficheros e historial. 20-95 % de reducción
según el contenido, **en local** (nada sale del equipo).

Compresores que cita: `SmartCrusher` (JSON), `CodeCompressor` (AST-aware:
Python, JS/TS, Go, **Rust**, Java, C/C++, C#, PHP), `Kompress-v2-base` (texto, ML),
**`CacheAligner`** (mantiene estable el prefijo del KV-cache del provider) y
**`CCR`** (compresión **reversible** con caché local para recuperar el original
bajo demanda). Se integra como librería, proxy FastAPI, **servidor MCP** o
envoltura del agente.

Encaje en CodeLoom:

- Es una capa **entre la tool y el provider**, opcional (crate/servicio), fuera
  de `codeloom-core`.
- **`CacheAligner` es la pieza que conecta con lo ya documentado**: evita que
  comprimir cambie el prefijo de la petición y rompa el prompt cache (misma
  preocupación que en hot-reload y skills).
- **`CCR` resuelve el riesgo central**: si comprime de más, el original se
  **recupera** por demanda; no se pierde información, se mueve fuera del prompt.
- Se puede consumir como **servidor MCP** → encaja con la idea de hot-reload de
  MCP (CodeLoom ya lo trata como tools).

## 3. Compresión de la salida de comandos — RTK (Rust Token Killer)

`rtk-ai/rtk` (~83k estrellas, **Rust**, binario único, Apache-2.0 +
BUSL-1.1). Es un **proxy CLI** que intercepta la salida de comandos y la comprime
antes de que llegue al contexto: 60-90 % en comandos de desarrollo habituales,
con **cuatro estrategias** (filtrado inteligente, agrupación, truncado,
deduplicación) sobre 100+ comandos (git, cargo, npm, pytest, Docker, k8s, AWS,
Terraform…). `git diff` 10k -> 2.5k; `pytest` 8k -> 800. Da soporte a 18 agentes.

Encaje en CodeLoom: en la tool **`bash`** (§5.3). CodeLoom ya ejecuta y devuelve
`ToolResult`; se añade un **filtro de salida por tipo de comando**. Crítico: el
`exit code` y las líneas de error **deben sobrevivir** (misma regla que caveman:
"payload verbatim, errores exactos"); y el original debe quedar recuperable.

## 4. Compresión de prosa — Caveman

Caveman es una **voz tersa** (`~/.agents/skills/caveman*`), no una capa del
motor: ya vive en el spec de skills como skill de gobernanza de estilo. Su
ecosistema añade `caveman-compress` (comprimir ficheros de memoria), `learn`
(reducir coste de tokens) y `stats`, bajo "Caveman Cloud".

**Gotcha de nombres**: "headroom" en caveman es la **métrica de presupuesto
inferido** (`caveman-evidence-review`: *inferred daily headroom*), NO el compresor
**Headroom** de §2. No confundir la métrica con la herramienta.

## 5. Coherencia con lo ya documentado

- **Prompt cache**: Headroom `CacheAligner` + la regla de aplicar cambios en
  **frontera de step** (hot-reload, skills). No comprimir ni cambiar el prefijo a
  mitad de stream.
- **jevalan**: los KPIs alimentan la calibración de umbrales y el enrutado de
  modelo; jevalan aporta el marco de medir con presupuesto y separar acierto de
  calidad final.
- **Medir antes de optimizar**: no activar compresión por defecto sin comprobar
  que la calidad no cae (tests/verificación como contrapeso).

## 6. Riesgos y gotchas

- **Context rot**: comprimir la salida de una tool puede borrar el detalle que el
  modelo necesitaba. Mitigación: **CCR/reversible** (recuperar el original).
- **Doble compresión**: Headroom + RTK + caveman acumulan pérdida; medir el
  ratio y la calidad combinados, no por separado.
- **RTK vs el éxito del comando**: no dejar que el filtrado oculte un `exit != 0`
  ni un error; errores y números exactos.
- **"headroom" ambiguo**: compresor vs métrica de presupuesto (§4).
- **Licencias**: Headroom Apache-2.0; **RTK Apache-2.0 + BUSL-1.1** (reventa
  comercial requiere contacto) → revisar antes de **embeber**, no solo consumir.
- **Verificar las cifras**: las estrellas/porcentajes vienen de la web; confirmar
  contra el repo y una medición real en CodeLoom con los KPIs de §1.

## 7. Config (borrador)

```toml
[kpi]
enabled = true
export = "jsonl"            # volcado para análisis

[compress.headroom]         # entrada: lo que el agente lee
enabled = false
mode = "mcp"                # mcp | lib | proxy
reversible = true           # CCR: recuperar el original bajo demanda
keep_cache_prefix = true    # CacheAligner

[compress.rtk]              # salida de comandos (tool bash)
enabled = false
preserve_errors = true
preserve_exit_code = true

[compress.caveman]          # prosa (vía la skill)
enabled = false
intensity = "full"          # lite | full | ultra
```

## 8. Preguntas abiertas

- ¿Headroom entra como **MCP** (reutiliza la idea de hot-reload de MCP) o como
  crate/proxy?
- ¿Los KPIs viven en `codeloom-core` (siempre) o en una capa opcional?
- ¿Se comprime **antes** de persistir el `ToolResult` o solo en la petición?
  (afecta a replay y a la verdad de la sesión).
- ¿Quién decide comprimir: config fija, el modelo (por tipo de contenido) o
  jevalan?
- ¿Se publican los KPIs comparables a los del lab (coste, latencia, calidad) para
  poder enfrentar brazos con y sin compresión?
