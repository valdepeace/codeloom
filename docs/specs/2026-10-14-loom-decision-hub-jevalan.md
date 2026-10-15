# CodeLoom — Idea/spec: hub de decisiones local (jevalan + Kev/Laya/Jev)

> Estado: **idea en exploración**. No implementar.
>
> Fecha: 2026-10-14
>
> Relacionado: `docs/specs/2026-10-13-loom-design.md` (§5.4 permisos, §6
> subagentes/jobs, §5.5 extensiones).
> Proyecto externo de referencia: `jevalan`
> (`E:/workspace/personal/jevalan`).

## 0. La idea

Usar **jevalan** como el hub de decisiones de CodeLoom: un proceso local al que el
agente le pregunta *antes de actuar* y que responde con una **decisión tipada**,
resuelta con reglas fijas + clasificadores pequeños, **sin gastar una llamada al
LLM grande**. Así CodeLoom decide riesgo, enrutado de modelo y despertares sin pagar
tokens ni latencia de nube.

jevalan es exactamente eso: "un hub local de decisiones. Los agentes le
preguntan; jevalan decide —o hace que decida un especialista— y devuelve una
respuesta tipada. **Nunca ejecuta el efecto**: eso queda en quien pregunta"
(`jevalan/README.md`). Es el mismo principio que CodeLoom: el motor decide, las
tools ejecutan.

## 1. Por qué ahorra llamadas al LLM

- **Reglas fijas primero.** Un `rm -rf /` o un `curl … | sh` los caza una regex;
  no hace falta preguntar a ningún modelo.
- **Clasificadores pequeños, no generación.** Kev/Laya/Jev devuelven
  **probabilidades tipadas** de preguntas sí/no ("¿destruye?", "¿exfiltra?"), no
  texto libre. Son órdenes de magnitud más baratos y rápidos que pedirle al LLM
  del chat que razone sobre lo mismo.
- **Varias preguntas en una petición.** El nodo `batch` de Kev calcula el estado
  una vez y lo reutiliza para las cuatro preguntas de riesgo
  (`flows/tool-risk-local.yaml`).
- **Local = sin nube.** Kev (GPU) y Laya (CPU) corriendo en la máquina no
  cuestan tokens ni exponen la entrada.

Latencias medidas por el propio jevalan (2026-10-10):

| Medida | Latencia |
|---|---|
| Kev, una pregunta, en caliente | 95-137 ms |
| Laya, una pregunta (round trip) | 657-881 ms |
| `tool.risk` con 4 preguntas en un nodo `batch` | 254-316 ms |

Frente a eso, una llamada al LLM del chat es segundos y tokens. La decisión de
"¿esta tool es peligrosa?" o "¿qué modelo basta para esto?" **no** debe costar
una llamada grande.

## 2. Los modelos clasificadores (`docs/local-models.md`)

| Modelo | Repo | Puerto | Dispositivo | Rol |
|---|---|---|---|---|
| **Jev** (System One) | nube (OpenRouter/TypeSafe) | — | — | Modelo de decisión de referencia; 7 preguntas por llamada |
| **Kev-4B** | `G:\dev\jev\kev` | 8009 | GPU (CUDA, bf16) | El local más potente; **varias preguntas en una petición** |
| **Kev-0.8B** | ídem | 8009 | GPU | Plan B si el 4B no cabe o va lento |
| **Laya** | `G:\dev\jev\laya` | 8021 | CPU (421M) | Encoder + cabeza de decisión; entrada máx. 8192 tokens |

Los tres sirven `POST /v1/systemone`, el mismo contrato que Jev, así que jevalan
los trata como cualquier backend y CodeLoom no necesita saber cuál es cuál.

## 3. Dónde encaja cada capacidad en CodeLoom

| Capacidad | Seam de CodeLoom | Efecto |
|---|---|---|
| `tool.risk` | `PermissionService` (§5.4) | Decidir `allow/ask/deny` **antes** de ejecutar una tool, sin llamada al LLM |
| `model.route` (nuevo) | selección de modelo/step (§4.2-4.3) | Enrutar tarea trivial -> modelo barato; difícil -> caro. Menos llamadas caras |
| `agent.wakeup` | subagentes/jobs dormidos (§6, m2) | Despertar (y gastar un paso de LLM) solo cuando el evento lo justifica |

`tool.risk` es el encaje inmediato: CodeLoom ya diseña un gate de permisos (§5.4) y
jevalan ya trae la capacidad hecha (`capabilities/tool.risk.yaml`,
`flows/tool-risk-local.yaml`). Su entrada es justo lo que CodeLoom tiene a mano
(`tool_name`, `tool_input`, `cwd`, `session_id`, `transcript_path`) y su salida
es `{ verdict: allow|ask|deny, reason, source, probabilities }`.

`model.route` es idea nueva: el flujo `flows/model-route.yaml` ya clasifica
dificultad (`score`, criterios trivial..muy difícil) y riesgo, y devuelve
`{ model, effort }`. CodeLoom puede usar el resultado para elegir provider/modelo por
turno (o por step), que es donde de verdad se ahorran llamadas.

`agent.wakeup` (`flows/agent-wakeup.yaml`) resuelve "¿merece la pena despertar a
un agente dormido?". Encaja con el modelo de subagentes de CodeLoom (§6): un
subagente que espera puede quedarse dormido y solo despertar (gastando un step)
cuando el evento lo justifica.

### 3.1 Una capa transversal: sí, pero no "para todo"

La intuición es buena: casi toda decisión de CodeLoom es un *clasificar y actuar*, y
jevalan es exactamente el seam de clasificación. Los puntos candidatos, todos
"¿qué hago? -> decide una capacidad":

| Punto de decisión | Capacidad | Spec |
|---|---|---|
| ¿Esta tool es peligrosa? | `tool.risk` | este |
| ¿Qué modelo/effort? | `model.route` | este |
| ¿Qué skills activo? | `skill.route` | skills |
| ¿Qué memoria cargo? | `memory.route` | memoria |
| ¿Merece Gauntlet? | `gauntlet.route` | gauntlet |
| ¿Despierto al subagente? | `agent.wakeup` | este |
| ¿Comprimo este contenido? | `compress.route` | KPIs/compresión |

Pero **"jevalan para todo" es una trampa**, y la evidencia del propio lab lo
avisa:

- **Errores correlacionados (~96%)**: jevalan/jev y el LLM fallan en los **mismos
  sitios**; un cascade solo añade **coste y latencia**, no precisión, salvo que
  los errores sean **complementarios**. Más clasificación **no** es más calidad.
- **Solo texto y no genera**: no juzga imágenes, **no ejecuta** código, **no
  explica**. Falla en derivación (código −12.9, mates −14.3, lógica −27.6).
- **Confianza alta puede ocultar fallos**: en seguridad, Jev da P<0.1 a inputs
  inseguros; auto-aprobar por confianza es peligroso.
- **Latencia/coste**: sí, ~100 ms, pero **por decisión**. En el camino caliente
  (por step) suma; hay que **presupuestar** y no clasificar lo trivial.
- **Regla > clasificador** cuando hay determinismo: exit code, esquema JSON,
  tests, patrones fijos. No preguntes a un modelo lo que decide una regex.

**Guarda**: clasificar con jevalan solo cuando la decisión sea **(i) frecuente,
(ii) barata de clasificar, (iii) recuperable o con gate, y (iv) sin una regla
que ya la resuelva**. Empezar en `shadow`, congelar umbrales y validarlos en
held-out. Ante `abstain`/`error`, fallback conservador. Así es una **capa
transversal útil**, no un peaje en cada paso.

## 4. Arquitectura: `codeloom-decide`

**Primero, encuadrar jevalan**: no es un servicio pesado, es **un router
configurable**. Su núcleo es pequeño y declarativo: `capacidad -> flujo`
(`rule`/`noul`/`choice`/`score`/`batch`) `-> backend` (Kev/Laya/Jev/Ollama). Es
"como semantic-router pero más simple": **sin embeddings ni store, sin
infraestructura**, se configura entero en YAML. El valor no está en el motor (es
sencillo), sino en **qué se configura** y en **medirlo** (el lab).

Consecuencia: CodeLoom puede (a) **consumir el daemon de jevalan**, o (b) **llevar su
propio router en proceso** con el mismo modelo de datos (capacidad + flujo +
backend). Es una decisión de **despliegue, no de diseño**: el seam de decisión
(`capacidad por nombre -> decisión tipada`) es el mismo.

Crate opcional, igual que `codeloom-herdr` y `codeloom-voice`: **el core no conoce ni
jevalan ni el router** (regla de arquitectura §2 del spec principal).

```
 CodeLoom (codeloom-core)                      jevalan (daemon local)
 ┌──────────────┐   POST /v1/hub       ┌────────────────────────────┐
 │ Permission   │   { capability,      │ capacidades -> flujos       │
 │ Service      │     input }          │  tool.risk -> reglas + Kev  │
 │ AgentLoop    │─────────────────────▶│  model.route -> Ollama      │
 │ SubagentRt   │◀─────────────────────│  agent.wakeup -> Jev        │
 └──────────────┘   { decision,        └────────────┬───────────────┘
        ▲            provider, trace }              │
        │  WS /events (telemetría, opcional)         ▼
        └──────────────────────────────────  Kev :8009 · Laya :8021
```

- **Cliente HTTP** de `/v1/hub` (capacidad por nombre) y, opcional, **WebSocket
  `/events`** para telemetría (chip en el header: qué decide y cuánto tarda).
- **Modos** copiados de jevalan: `shadow` (solo registra el veredicto, no
  bloquea — útil para calibrar con datos reales) y `enforce` (aplica).
- **Degradación**: si el daemon no responde, `tool.risk` cae a `ask` (nunca
  `allow` a ciegas), y `model.route` cae al modelo por defecto de CodeLoom. Mismo
  criterio que el hook de jevalan (`error/abstain/plazo -> ASK`).
- **Nunca ejecuta**: jevalan devuelve la decisión; CodeLoom aplica el efecto.

## 5. Contrato (el de jevalan, sin inventar)

- Petición: `POST /v1/hub` con `{ capability: "tool.risk", input: {...} }` (la
  envoltura `jev-hub/0.1`, con esquema por capacidad).
- Respuesta: `{ status, decision, provider, trace }`. `status` distingue ok de
  `abstain`/`error`; **`abstain` y `error` nunca llevan `decision`**.
- `tool.risk`: entrada `{ tool_name, tool_input, cwd?, session_id?,
  tool_use_id?, transcript_path? }`; decisión `{ verdict, reason, source,
  probabilities? }`.
- Así mismo para `model.route` (`{ model, effort }`) y `agent.wakeup`
  (`{ wake, reason, probability? }`).
- Hay **hook de Claude Code y de Crush** ya hechos
  (`jevalan hook claude-code`, `docs/crush-hook-install.md`): CodeLoom puede
  consumirlos o hablar `/v1/hub` directamente. Hablar el hub directamente es
  preferible (una capa menos).

## 6. Config (`codeloom.toml`)

```toml
[decide]
enabled = true
hub_url = "http://127.0.0.1:8080"
token_env = "JEVALAN_TOKEN"
mode = "shadow"                 # shadow | enforce
timeout_ms = 5000

[decide.capabilities]
tool_risk = "tool.risk"
model_route = "model.route"     # opcional
agent_wakeup = "agent.wakeup"   # m2
```

El daemon de jevalan se declara aparte (sus backends `kev`/`laya`/`ollama`), como
en `jevalan/config.example.yaml`; CodeLoom solo apunta a su URL.

## 7. Qué llamadas al LLM se ahorran, en concreto

- **Gate de cada tool**: hoy -> razonar con el LLM o preguntar al usuario;
  con jevalan -> una clasificación local de ~100-300 ms, sin tokens.
- **Elección de modelo**: hoy -> usar siempre el modelo grande; con
  `model.route` -> modelos baratos para tareas triviales.
- **Despertar subagentes**: hoy -> despertar y gastar un step; con
  `agent.wakeup` -> dormir hasta que haya señal.

## 8. Relación con otras piezas

- **Hot-reload** (`2026-10-14-loom-mcp-skills-hot-reload.md`): jevalan ya
  **recarga los flujos en caliente** al cambiar el fichero; CodeLoom podría refrescar
  el set de capacidades que consulta sin reiniciar.
- **Permisos** (§5.4): jevalan es un *proveedor* del gate, no un reemplazo;
  sigue habiendo allowlist, prompt de UI y modo `--yolo`.
- **Voz / resumen**: son capas distintas; comparten el patrón "crate opcional
  fuera del core".

## 9. Gotchas

- **Nunca `allow` a ciegas** si jevalan falla o se pasa de plazo: cae a `ask`
  (regla ya fijada por jevalan y por sentido común).
- **`abstain`/`error` no llevan decisión**: tratarlos como "sin opinión", no como
  permiso.
- **Kev y Laya no caben juntos en una GPU de 8 GB** (`docs/local-models.md`):
  Laya va en CPU. CodeLoom no arranca estos servidores; solo consume jevalan.
- **No meter esto en `codeloom-core`**: igual que herdr y voz, es integración
  opcional por URL.
- **La entrada puede llevar secretos** (comandos con tokens): jevalan registra
  los eventos **sin la entrada ni la decisión**, pero conviene respetar ese
  límite también en los logs de CodeLoom.

## 10. Preguntas abiertas / a verificar

- ¿Consultar `tool.risk` por tool-call o cachear el veredicto por `tool_use_id`?
- ¿`model.route` decide por turno o por step? Impacto en cache de prompt.
- ¿CodeLoom habla `/v1/hub` directo (recomendado) o reutiliza el hook de jevalan?
- ¿El set de capacidades se descubre de jevalan o se declara en `codeloom.toml`?
- ¿Se soporta Jev en la nube (privacidad) además de Kev/Laya en local?

## 11. Trabajo previo (prior art)

Sí, la idea no es nueva y hay literatura y proyectos. Lo sólido, verificado:

| Trabajo | URL | Qué aporta a CodeLoom |
|---|---|---|
| **FrugalGPT** (Chen, Zaharia, Zou; 2023) | arXiv:2305.05176 | El patrón **cascade accept/escalate** seminal: probar barato y escalar al caro |
| **RouteLLM** (LMSYS/Anyscale) | arXiv:2406.18665, `lm-sys/RouteLLM` | Router entrenado por preferencias para elegir modelo; ~85 % de ahorro |
| **Language Model Cascades** (Google) | arXiv:2207.10342, `google-research/cascades` | Cascadas como programas probabilísticos; marco general |
| **semantic-router** (aurelio-labs) | `aurelio-labs/semantic-router` | Routing por **embeddings**, sin llamada a LLM (decisión barata) |
| **Speculative decoding** | arXiv:2211.17192 | Borrador barato + verificación cara: "lo difícil contiene subtareas fáciles" |
| **Correlated Errors in LLMs** (ICML 2025) | arXiv:2506.07962, `nikhgarg/llm_correlated_errors_public` | **Valida el límite**: modelos que fallan juntos (≈60 % mismo fallo, más en los mejores) → un cascade añade **coste, no precisión**. Es el respaldo externo a "wrong in the same places" del lab |
| **JEV-as-a-Judge** | arXiv:2609.26550 (citado en el lab) | Cascade con **umbral congelado**; acepta si confía, escala si no; muy barato frente a GPT-6 |

Recientes (según búsqueda web; **verificar** antes de citar): AgentRouter /
Agent-as-a-Router (routing a nivel de **step**), MasRouter (routing por capas:
modo colaboración -> rol -> modelo), HARISSA / UCCI (confianza calibrada para
escalar), CASTER (router que aprende de fallos). Y `pymc-labs/decision-hub` usa
"decision hub" para un **registro de skills**, no para un daemon de decisión (no
confundir).

Foros: hay hilos de HN/Reddit sobre RouteLLM y los cascades, pero **no** hay un
hilo canónico que describa "un hub local que decida todo". Es una síntesis
nuestra, no un consenso con nombre propio.

**Conclusión**: el patrón existe y está estudiado; lo que CodeLoom añade es
**juntarlo en un solo seam local** (con Kev/Laya/Jev) y **presupuestarlo** con la
evidencia de errores correlacionados en contra de usarlo "para todo".

## 12. Reparto: qué se aprovecha en jevalan y qué en CodeLoom

La capa de decisión se parte en **tres escalones**, según coste y estado:

| Escalón | Dónde | Qué | Por qué ahí |
|---|---|---|---|
| 1. **Señales heurísticas** | **CodeLoom, en proceso** | forma de la petición: `keyword`, `language`, `structure`, `context` (ventana), `conversation` (tool-heavy), `metadata`, `authz`, `event` | son deterministas y CodeLoom **ya tiene** el request; no merece un viaje al daemon |
| 2. **Señales aprendidas + decisión** | **jevalan** | `classifier`, `domain`, `complexity`, `pii`, `jailbreak`, `safety`, `preference`, `reask`, `kb`; `projections` (bandas); `rules` AND/OR + `priority` -> **decisión tipada + clase/conjunto de candidatos** | texto -> probabilidad; encaja con `noul/choice/score`; stateless |
| 3. **Selección y efecto** | **CodeLoom (selección/serving)** | ranking entre candidatos (`hybrid`/score ponderado), **afinidad de cache**, coste/KPIs, **routing al backend concreto** | necesita **estado** (sesión, `prompt_cache_key`, modelo previo) y pertenece al **efecto** |

Mapeo directo de las capas de `semantic-router`:

| Capa SR | Se aprovecha en |
|---|---|
| `signals` heurísticas | CodeLoom en proceso |
| `signals` aprendidas | jevalan |
| `projections` | jevalan (bandas/umbrales) |
| `decisions` (rules + priority) | jevalan |
| `algorithm` (`hybrid`) | CodeLoom (selección/serving) |
| `plugins` | CodeLoom (tools/hooks) |

Dos reglas para no equivocarse:

- **No mandes a jevalan lo que decide una regex o un dato del request**: eso es
  escalón 1, en proceso. Ahorra el round-trip.
- **No metas estado ni efecto en jevalan**: selección stateful, afinidad de cache
  y routing son escalón 3, de CodeLoom. Si no, jevalan deja de ser un hub de
  decisiones y pasa a ser un serving router.

**En una frase**: jevalan responde **"qué toca"** (señales aprendidas + decisión);
CodeLoom responde **"cuál concreto y cómo"** (selección, cache, efecto), y lo trivial
se decide **en proceso** sin molestar a nadie.
