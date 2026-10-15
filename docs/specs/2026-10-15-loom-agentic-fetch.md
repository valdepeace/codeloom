# CodeLoom — Idea: tool de red con sub-agente (`fetch` / `agentic_fetch`)

> Estado: **idea en exploración**. No implementar.
>
> Fecha: 2026-10-15
>
> Relacionado: `docs/specs/2026-10-13-loom-design.md` (m2: subagentes/jobs),
> `docs/reference/analisis-herdr-crush-dsh.md:101-103` (Crush: `download`,
> `fetch`, sub-agentes `agent`, `agentic_fetch`). Fuera del MVP.

## 0. La idea

Copiar de Crush el patrón de **fetch agentic**: una tool que **descarga una URL**
y **delega en un sub-agente** (otra llamada al provider) la tarea de extraer o
resumir lo relevante, devolviendo texto compacto al hilo principal. La UI lo
muestra como un tool item con el dominio y el resultado.

## 1. Por qué es un bloque grande

Hoy CodeLoom **no tiene** ni tool de red ni subagentes:

- No existe `fetch`/`download` (`tools/` = bash, edit, glob, grep, view).
- **Subagentes/jobs = m2**, fuera del MVP (`loom-design.md`).
- El MVP evita red real y el análisis de decisión (§7) describe dos tools
  lejanas en alcance: `http_request`/`browser` avanzadas y `fetch`.

Hacer la versión agentic implica **tres piezas nuevas**: (a) tool de red,
(b) concepto de subagente (otra llamada al provider con su propio prompt),
(c) UI del resultado del sub-agente.

## 2. Forma propuesta

- **`fetch`** (simple, sin sub-agente): descarga la URL, extrae texto legible
  (HTML → texto) y lo devuelve acotado por tamaño. Es el paso 1 razonable.
- **`agentic_fetch`** (encima): tras descargar, lanza un sub-agente con
  `provider.stream` y un prompt de extracción; devuelve el resumen.
- **UI**: tool item con el dominio (y opcionalmente "resumiendo…") y, al
  terminar, el resumen. Reutiliza `tool_item`; no requiere widget nuevo.

## 3. Seguridad (no negociable)

- **Egreso de red**: desactivado por defecto; allowlist de dominios/esquemas
  (`https`), y respeto del modo de permisos.
- ***Prompt injection***: el contenido descargado es **dato no confiable**;
  nunca se ejecuta ni se pasa como instrucciones al agente principal.
- Límite de **tamaño** y de tiempo; sin seguir redirecciones a esquemas raros.
- **Tests sin red**: provider mock / `wiremock` (regla del repo).

## 4. Gotchas

- El MVP prohíbe red real en tests; habrá que diseñar el seam (trait `Fetcher`)
  para inyectar un doble.
- Mapear el resultado al transcript sin volcar HTML crudo.
- Coste del sub-agente: una llamada extra por URL.

## 5. Preguntas abiertas

- ¿Empezar por `fetch` simple (sin sub-agente) y dejar `agentic_fetch` para m2?
- ¿Allowlist de dominios configurable, o "preguntar por permiso" cada vez?
- ¿La extracción usa el mismo modelo del chat o uno rápido?
- ¿Soporta POST/formularios, o solo GET de lectura?
