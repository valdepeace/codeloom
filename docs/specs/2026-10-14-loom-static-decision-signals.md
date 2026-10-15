# CodeLoom — Idea/spec: la capa estática de decisión (señales + reglas)

> Estado: **idea en exploración**. No implementar.
>
> Fecha: 2026-10-14
>
> Relacionado: `2026-10-14-loom-decision-hub-jevalan.md` (§4 y §11) y
> `2026-10-14-loom-decision-experiments.md`.
> Fuente: `semantic-router` (`vllm-project/semantic-router`), `config.yaml`
> local en `E:/workspace/personal/semantic-router`.

## 0. Qué es la "parte estática"

`semantic-router` es enorme (embeddings, store, LoRA, fusión, "micro-agent"…),
pero su **decisión principal** se describe **declarativamente** en `config.yaml`
y **no usa el LLM**: son **señales** evaluadas y **decisiones** con reglas y
prioridad que eligen modelo. Esa es la parte estática: **determinista + un
clasificador pequeño**, cero llamadas generativas.

## 1. El modelo de `semantic-router` (config.yaml)

```
routing:
  strategy: priority
  modelCards: [...]        # qué sabe hacer cada modelo
  signals:
    keywords: [{name, operator: OR, case_sensitive, keywords: [...]}, ...]
    domains:  [{name, description, mmlu_categories: [...]}, ...]   # clasificador
  decisions:
    - {name, priority, rules: {operator: AND/OR, conditions: [{type: domain|keyword, name}]}, modelRefs: [...]}
```

Tres piezas:

- **Señales** (`signals`):
  - **keyword**: coincidencia de palabras, `operator: OR` (o AND), `case_sensitive`.
    Coste ~0, sin modelo.
  - **domain**: clasificación por **cabeza clasificadora** pretrained (MMLU
    categories: `computer science`, `math`, `other`). Un modelo pequeño, no
    generativo.
  - (El `model_catalog` admite además un **clasificador externo**. Ojo: el
    `jev-openrouter-classifier` que aparece en este `config.yaml` **lo añadió el
    usuario** como prueba propia; **no** es parte de semantic-router.)
- **Decisiones** (`decisions`): cada una tiene **`priority`**, `rules` (un
  `operator` AND/OR sobre `conditions` que referencian señales) y **`modelRefs`**
  (lista ordenada de modelos destino).
- **Estrategia**: `priority` (gana la de mayor prioridad); hay una **regla de
  fallback** con `conditions: []` que captura todo si nada casa.

Ejemplo real del config: `route-cs-architecture` (priority 260) casa si
`domain == computer science` **AND** keyword `cs_architecture` -> manda a
`deepseek-v3.2`/`glm-5-3`.

### 1.1 El catálogo de señales (lo que más interesa)

La capa de señales es un **catálogo de detectores con nombre**, separado de las
reglas. Dos familias:

- **Heurísticas** (sin modelo general): `authz` (identidad/rol/tenant),
  `conversation` (multi-turno, tool-heavy, agéntico), `context` (ventana
  necesaria), `event`, `input-modality`, `keyword` (palabras/BM25/n-gramas),
  `language`, `metadata` (pistas del llamante), `structure` (densidad,
  contadores).
- **Aprendidas** (embeddings/clasificador): `classifier` (etiquetas de un modelo
  local o LLM externo), `complexity` (easy/medium/hard), `domain` (tema),
  `embedding` (intención por ejemplos), `modality`, `fact-check`, `hallucination`,
  `jailbreak`, `safety`, `pii`, `preference`, `reask`, `kb`, `user-feedback`.

**Projections** combinan señales antes de decidir: `partitions` (una gana,
`semantics: exclusive`), `scores` (`weighted_sum`) y `mappings`
(`threshold_bands` -> bandas de routing). Se separan así a propósito:

- señales = **solo detección**; el resultado va en `decision/`.
- combinar/derivar bandas -> `projections/`.
- elegir modelo -> `algorithm/`.
- comportamiento de ruta -> `plugin/`.

Es la separación que hace auditable el routing: *qué detecto* != *qué hago*.

## 2. Equivalencia con jevalan

Son la **misma idea**, con distinta forma:

| semantic-router (estático) | jevalan (`flows/*.yaml`) |
|---|---|
| `signals.keywords` (OR/AND) | nodo `rule` (expresión CEL, p. ej. `input...matches(r"...")`) |
| `signals.domains` (clasificador) | nodo `noul` / `choice` / `score` (pregunta al modelo de decisión) |
| `decisions[].rules` (AND/OR de conditions) | composición de nodos `rule`/`route` |
| `decisions[].priority` + `strategy: priority` | orden de nodos + reglas `route` (primera que casa) |
| `modelRefs` | `outputs` del flujo (p. ej. `{ model, effort }`) |
| fallback `conditions: []` | `else` / `error` del flujo |

Diferencia: jevalan es **más programable** (umbrales, `batch` de preguntas en una
llamada, CEL), semantic-router es **más declarativo** (señales + reglas +
prioridad). Los dos evitan el LLM en la decisión.

## 3. Qué adopta CodeLoom

Una sola forma declarativa para **toda** decisión:

```
señales  ->  reglas (AND/OR + prioridad)  ->  objetivo
```

- **Señales**: deterministas primero (`keyword`/regex) y clasificador pequeño
  después (`domain`/`score` con Kev/Laya/Jev). Nunca el LLM grande para decidir.
- **Reglas**: AND/OR de condiciones sobre señales, con **prioridad** y **fallback**.
- **Objetivo genérico**: no solo `modelRefs`. El mismo motor decide
  **modelo**, **skills**, **memoria** o **gate de tool** (`allow/ask/deny`) — es
  la "capa transversal" del hub (§3.1), pero **estática y configurable**.
- Reutiliza el modelo de jevalan (flujos) o el de semantic-router (signals +
  decisions); CodeLoom formaliza **uno** y lo aplica a todas las capacidades.

Ventajas: **cero LLM** en el camino de decisión, declarativo, testeable, y el
mismo motor para todo. Límites: las señales clasificadoras son **aproximadas**
(errores correlacionados, ver el hub §3.1) y hay que **medirlas** (el lab).

## 4. Ejemplo en CodeLoom (borrador)

```toml
[decide.strategy]
order = "priority"          # primera que casa, por prioridad

[decide.signals.cs_arch]    # señal keyword (coste ~0)
kind = "keyword"
operator = "OR"
keywords = ["architecture", "microservices", "DDD"]

[decide.signals.domain]     # señal clasificador (Kev/Laya/Jev)
kind = "classify"
question = "¿de qué dominio es esta tarea?"
labels = ["cs", "math", "other"]

[decide.decisions.route-arch]
priority = 260
when = "domain == cs && cs_arch"
target = { model = "reasoning", effort = "high" }   # o { skill = ... } o { gate = ... }

[decide.decisions.default]
priority = 0
when = "true"
target = { model = "default" }
```

## 5. Selección de modelo: `hybrid` y afinidad de cache

La decisión elige un destino; **qué modelo concreto** dentro de ese destino lo
decide el `algorithm` (`strategy`), y aquí está la joya que interesa.

- **`hybrid`** (paper *Hybrid LLM*, arXiv:2404.14618) combina **varios selectores
  en un score único y auditable**:
  - `EloSelector` — rating relativo de modelos (experiencia).
  - `RouterDCSelector` — similitud semántica query<->descripción del modelo.
  - `AutoMixSelector` — estimación de valor coste-calidad.
  - **coste** como **ajuste multiplicativo de segunda etapa** (no un término más).
  - pesos configurables (`experience_weight`, `router_dc_weight`,
    `automix_weight`, `cost_weight`) y normalización 0-1.

- **Afinidad de cache = el último ajuste, acotado.** Para una petición que
  **continúa una sesión**, favorece al modelo que sirvió el turno anterior para
  que el **prompt cache del backend siga siendo útil**, y **se desvanece a cero
  según crece la diferencia entre los dos mejores scores base**. `prompt_cache_key`
  marca la petición como continuación; sin modelo previo, se favorece el que sirvió
  la misma key en la recipe; cada réplica lo recuerda **una hora**.

Lo que CodeLoom debe copiar:

- La afinidad de cache es **preferencia acotada, no bloqueo**: cambia de modelo
  cuando la mejora base lo justifica, y **decae con el gap**. (En el `config.yaml`
  local el propio comentario lo dice: *"affinity remains a bounded selection
  preference, not a model lock"*, y trae la adaptación/protección **apagadas**.)
- **Consistencia con lo ya documentado**: esto es la otra cara del *prompt cache*
  que en `hot-reload`, `skills` y `memory` obligaba a aplicar cambios en frontera
  de step y a mantener el prefijo. Aquí el router **penaliza romper el cache**,
  pero solo como ajuste de último tramo.
- **Typed candidates**: dos referencias al mismo modelo con distinto `effort`
  (`low`/`high`) son candidatos **separados**; no se colapsan por nombre. Encaja
  con `model.route` devolviendo `{ model, effort }`.

## 6. ¿Va esto en jevalan o se sale de su fin?

Hay que partir la capa en dos, y la frontera es clara:

| Parte | ¿En jevalan? | Por qué |
|---|---|---|
| **Señales** (keyword, domain, complexity, pii, jailbreak…) | **Sí** | es detección: encaja con `rule` / `noul` / `choice` / `score` |
| **Decisiones** (reglas AND/OR + prioridad -> capacidad) | **Sí** | es literalmente su fin: devolver una decisión tipada |
| **`hybrid`** (Elo + RouterDC + AutoMix + coste) | **No (o con matices)** | necesita **estado** y **embeddings/descripciones/precios** |
| **Afinidad de cache** | **No** | es propiedad del **efecto/serving**, no de la decisión |

Razones de fondo:

- **jevalan es stateless y "nunca ejecuta el efecto"**: devuelve `{ decisión }` y
  se olvida. La afinidad de cache necesita recordar qué **backend sirvió el turno
  anterior**, el `prompt_cache_key` y el estado por réplica durante una hora; eso
  no es una decisión pura, es **conocer el serving**.
- **El prompt cache vive en el backend/proveedor**, no en el hub. Mantenerlo
  caliente es del lado del **efecto** (quien enruta de verdad la llamada al
  modelo), no de quien decide "qué toca".
- **`hybrid` es selección continua entre candidatos** (normalizar, ponderar,
  ajustar coste): es el `algorithm` de semantic-router, una capa distinta de
  signals/decisions.

**Reparto propuesto:**

- **jevalan decide el "qué"**: la capacidad y el **conjunto de candidatos o la
  clase** (`{ model_class: reasoning, effort: high, candidates: [...] }`). Sigue
  siendo un hub de decisiones.
- **CodeLoom (su capa de selección/serving) decide el "cuál concreto"**: la selección
  stateful entre candidatos, con **afinidad de cache acotada** y coste. Ahí sí
  hay estado (sesión, cache key, precios) — y CodeLoom lo tiene.

Analogía con semantic-router: jevalan cubre **signals + projections + decisions**;
`algorithm` (`hybrid`) y `plugins` quedan **fuera**. Meter `hybrid` + afinidad en
jevalan lo convertiría en un **serving router** (otro producto, el de vLLM SR), no
en un hub de decisiones. Es una decisión de producto, y conviene no difuminarla.

**El lado de jevalan se especifica en su propio repo**:
`jevalan/docs/superpowers/specs/2026-10-14-capa-de-senales-y-decisiones-design.md`
(señales con nombre, `projections` y decisiones con prioridad). Este documento es
el **hallazgo de CodeLoom**; aquel es el **plan de jevalan**.

## 7. Preguntas abiertas

- ¿Adoptamos la forma de semantic-router (`signals`+`decisions`) o la de jevalan
  (`flows`)? ¿O una que soporte las dos?
- ¿Una sola tabla de señales/decisiones para todo, o una por capacidad
  (`tool.risk`, `model.route`, `skill.route`)?
- ¿Cómo se miden las señales clasificadoras sin caer en los errores
  correlacionados (P0 del plan de experimentos)?
- ¿Se expone la decisión (qué regla casó) para depurar, como la TUI de jevalan?
