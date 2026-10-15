# CodeLoom — Plan de experimentos: qué probar de los papers

> Estado: **borrador para revisión**. No implementar hasta aprobar.
>
> Fecha: 2026-10-14
>
> Relacionado: `2026-10-14-loom-decision-hub-jevalan.md` (§11 prior art),
> `2026-10-14-loom-gauntlet-loop.md`, `2026-10-14-loom-kpis-prompt-compression.md`.

## 0. Objetivo y principio

De todo el prior art, quedarse con lo **probable y medible**: cada idea de paper
se convierte en un experimento con hipótesis, diseño, métricas y guardas. El
principio es el del lab: **medir antes de adoptar**, con umbrales congelados y
held-out. Nada entra en CodeLoom por "suena bien".

Y una lección transversal de los papers, que ordena todo: los modelos
**fallan juntos** (errores correlacionados), así que un cascade **ahorra coste,
no da precisión**. Eso convierte un experimento en **P0**.

## 1. El harness: reutilizar `jevalan lab`, no reinventarlo

El laboratorio ya existe (`offline` / `record` / `live`, fixtures, cassettes,
presupuesto, `runs/<id>/report.md`, Brier). CodeLoom **no** construye su propio
banco de pruebas: usa el lab para decidir qué capacidad/umbral funciona.

- **`shadow.jsonl`** ya acumula decisiones reales -> de ahí salen los **fixtures**.
- Los **KPIs** de CodeLoom (`loom-kpis-prompt-compression.md`) alimentan el mismo
  informe (coste, latencia, calidad), para poder enfrentar brazos.

**Nota de coste de infraestructura**: el router es **simple y configurable**
(jevalan: `capacidad -> flujo -> backend` en YAML; "como semantic-router pero más
simple", sin embeddings ni store). Por eso empezar es barato: P0 y P1 no piden
infraestructura, solo el router y datos. Lo caro no es el router; es **medir**.


## 2. Experimentos, por prioridad

### P0 — ¿Los errores son complementarios o correlacionados?

- **Papers**: *Correlated Errors in LLMs* (arXiv:2506.07962, ICML 2025);
  *JEV-as-a-Judge* (arXiv:2609.26550).
- **Hipótesis**: decide si **tiene sentido** cualquier cascade en CodeLoom. Si el
  decision model y el LLM fallan en los mismos casos, un cascade no mejora la
  precisión: solo ahorra coste/latencia.
- **Diseño**: mismo conjunto de casos (fixtures del lab + shadow) evaluados por
  el decision model (Jev/Kev) y por el LLM. Medir la **tasa de "ambos mal, misma
  respuesta"** y el AUROC de complementariedad.
- **Métricas**: corr. de errores, AUROC de detección del error del LLM por parte
  del decision model, coste relativo.
- **Salida esperada**: una **regla de diseño** para todo lo demás — cascade solo
  para coste, o cascade también para precisión (poco probable).
- **Coste**: bajo (es medición sobre datos que ya existen).

### P1 — `tool.risk` en modo sombra (la integración más segura)

- **Papers**: `arXiv:2609.33401` (decisiones de seguridad; la confianza alta
  oculta fallos) + *JEV vs LLMs as Rubric Judges* (arXiv:2609.29769).
- **Hipótesis**: un gate local de riesgo clasifica igual que el humano/LLM a una
  fracción del coste, **sin subir falsos-aprobados**.
- **Diseño**: hook `PreToolUse`-equivalente en **shadow** (registra, no bloquea),
  con `flows/tool-risk-local.yaml` contra Kev. Congelar τ (0.55/0.85) antes.
- **Métricas**: falsos-aprobados (lo crítico), desacuerdo con humano, coste,
  latencia; **errores a confianza alta** aparte.
- **Guardas**: `abstain`/`error` -> `ask`; nunca `allow` a ciegas; τ validado en
  held-out.
- **Encaje**: `PermissionService` (§5.4) — read-only, sin efecto. **Primer trial.**

### P2 — Router de modelo (RouteLLM / FrugalGPT)

- **Papers**: RouteLLM (arXiv:2406.18665), FrugalGPT (arXiv:2305.05176).
- **Hipótesis**: elegir modelo por dificultad baja el coste **sin** bajar la
  calidad final.
- **Diseño**: `model.route` (flujo `flows/model-route.yaml` con `score`) más un
  router afinado con las decisiones de shadow (al estilo RouteLLM: aprender del
  histórico). Comparar contra "siempre el modelo grande".
- **Métricas**: coste, latencia, **calidad final** (tests/verificación),
  **arrepentimiento acumulado** (a lo ACRouter), escalados.
- **Guardas**: el router decide por **step** o por **turno** (probarlo; P4);
  `error` -> modelo por defecto.

### P3 — Cascade en el crítico del Gauntlet (filtro antes del LLM)

- **Papers**: FrugalGPT + JEV-as-a-Judge + *JEV vs rubric judges*.
- **Hipótesis**: el decision model **filtra** (F1 binario textual, F2 A/B de
  texto) y el crítico LLM solo entra si confianza < τ -> menos coste, misma
  calidad, sin subir falsos-aprobados.
- **Diseño**: 3 brazos (LLM solo / decision-model solo / filtro->LLM) sobre los
  mismos artefactos y juicios humanos (el diseño ya esbozado en el lab,
  `src/jev-gauntlet-poc/`).
- **Métricas**: las del informe del Gauntlet (falsos-aprobados, desacuerdo,
  cambio al invertir orden A/B, abstenciones, calidad final, coste).
- **Condición de éxito**: superar al crítico LLM solo **o** bajar coste sin subir
  falsos-aprobados.

### P4 — Routing por step (AgentRouter, reciente -> verificar)

- **Paper**: routing a nivel de **step** > por llamada (según búsqueda web;
  verificar antes de apoyarse).
- **Hipótesis**: en tareas multi-step, decidir por paso mejora frente a decidir
  una vez por turno.
- **Diseño**: comparar `model.route` por turno vs por step, mismo presupuesto.
- **Métricas**: coste/latencia/calidad; impacto en **prompt cache** (el prefijo
  cambia con el modelo).
- **Nota**: posterior a P2; el churn de cache puede comerse el ahorro.

## 3. Lo que NO se prueba ahora

- **semantic-router** (`aurelio-labs/semantic-router`): técnica elegante (routing
  por embeddings, sin LLM) pero exige **infraestructura** (embeddings + store);
  demasiado para empezar. Se reevalúa si P2 muestra que el router es el cuello.
- **Speculative decoding**: es a nivel de token, no de decisión de agente; solo
  se toma el **principio** (borrador barato + verificación), no la técnica.

## 4. Reglas comunes a todos los experimentos

- **Congelar preguntas y umbrales antes** de mirar los casos finales.
- **Held-out**, nunca in-sample.
- **Shadow primero** (registrar sin actuar) en todo lo que tenga efecto.
- **`abstain`/`error` -> conservador** (nunca `allow` a ciegas).
- **Separar acierto de decisión de calidad final** del artefacto.
- **No sumar evidencias distintas** (re-contado vs before/after vs holdout); cada
  una se presenta con su fuerza.
- **Contrapeso de calidad**: una sesión más barata que **falla más** no es un
  ahorro.

## 5. Qué se implementa en CodeLoom (mapeo)

| Experimento | Capacidad | Seam de CodeLoom | Efecto |
|---|---|---|---|
| P0 | (medición) | `codeloom-decide` + lab | ninguno (decide diseño) |
| P1 | `tool.risk` | `PermissionService` (§5.4) | gate de tools |
| P2 | `model.route` | selección de modelo (§4.2-4.3) | coste |
| P3 | `gauntlet.route` + filtro | crítico del Gauntlet | coste/calidad |
| P4 | `model.route` por step | step-loop (§4.3) | coste |

Orden recomendado: **P0 -> P1 -> P2 -> P3 -> P4**. P0 es barato y decide el
alcance de todo lo demás; P1 es el primer trial real y sin riesgo.
