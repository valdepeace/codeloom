# Elección automática de agente: regla y clasificador (A12b)

Estado: propuesta del 2026-10-23, pendiente de aprobación (tarea A12b de
`docs/plans/2026-10-22-codeloom-a2a-roadmap.md`). No implementar todavía. Independiente de A7–A9.

## Propósito

Hoy el padre elige el agente leyendo la lista del prompt y, si duda, pregunta con `ask_user`
(`[agents] confirm_delegation`: `doubt`, `always`, `never`). Es una petición al modelo, no una
garantía. Esta tarea añade una **elección automática** previa, determinista primero y con un
clasificador opcional después, de modo que cuando un agente encaje con claridad se elija sin gastar
un turno ni una pregunta, y que **siempre quede visible qué agente se eligió y por qué**
(observabilidad, valor central del proyecto).

## Lo que consta

- A2A **no fija** cómo se elige un agente ni trae campo de modelo o esfuerzo (nota de A12 en el
  roadmap): la elección es decisión nuestra y se rotula como propia.
- Las tarjetas (A6) traen `skills[]` con `tags`, `examples` y modos de entrada/salida; el registro
  (A13) solo ofrece al enrutador los agentes **aprobados**.
- La pista Jevalan tiene `model.route` (J7) con la guarda «regla antes que clasificador»; aquí no
  se usa Jevalan, se aprovecha solo esa guarda.
- El hueco existente es `regla_de_eleccion` en `registry/mod.rs`; hoy solo emite texto para el
  modelo.

## Lo que NO consta (y por tanto decidimos)

- Qué umbral de coincidencia basta para elegir sin preguntar: sin medir, es una apuesta nuestra.
- Si un clasificador compensa su coste frente a preguntar: **no se ha medido**. Por eso va apagado
  por defecto y detrás de la regla.

## Decisiones (nuestras)

- **D1. Un trait `AgentRouter`** en el núcleo: `route(&self, petición, candidatos) ->
  Result<Option<RouteDecision>>`, con `RouteDecision { agent, reason, source }` y `source ∈
  {Rule, Classifier}`. `None` significa «no sé»: se vuelve al comportamiento actual.
- **D2. Un `agent` explícito en la herramienta `subagent` siempre manda.** El enrutador solo actúa
  cuando se omite.
- **D3. Primer nivel, regla determinista (`RuleRouter`).** Puntúa cada candidato aprobado por
  coincidencias entre las palabras de la tarea y sus `tags`, nombre y `examples`. Elige solo si el
  mejor supera un mínimo **y** saca ventaja clara al segundo; si no, `None`. Sin llamadas al modelo.
- **D4. Segundo nivel, clasificador (`ClassifierRouter`), opcional y apagado.** Una llamada barata al
  proveedor con salida JSON estricta; la respuesta se valida contra los nombres candidatos; tiempo
  máximo; cualquier fallo o respuesta inválida es `None`. Solo se consulta si la regla devolvió
  `None`. Cuotas del decálogo: lo que entra al clasificador es nombre, descripción acotada y tags de
  los candidatos aprobados, con tope total.
- **D5. Los remotos entran como datos:** su descripción es texto de un tercero. Al clasificador
  solo llegan campos acotados, marcados como datos; las instrucciones de la tarjeta no se obedecen.
- **D6. Configuración:** `[agents] routing = "off" | "rule" | "classifier"` (por defecto `off`;
  `classifier` implica la regla primero) y `routing_model` para el clasificador.
- **D7. Convivencia con `confirm_delegation`:** `always` sigue preguntando siempre (el enrutador
  solo sugiere el agente en la pregunta); en `doubt` y `never`, una decisión del enrutador sustituye
  a la duda y a la pregunta.
- **D8. Visibilidad:** cada decisión emite un evento (`AgentRouted { agent, reason, source }`) que
  la interfaz pinta en el transcript junto al nodo del subagente, y queda en el resultado de la
  herramienta que ve el modelo. Una abstención también se anota en el registro de depuración.

## Fuera de alcance, declarado

- Elegir modelo o esfuerzo por tarea (J7 y `model.route`): otra tarea y otro seam.
- Aprendizaje a partir de elecciones pasadas.
- Enrutar entre agentes remotos por coste o latencia.
- Medir el ahorro: se mide después, con el interruptor apagado por defecto hasta entonces.

## Preguntas abiertas

- Umbral y ventaja de la regla (D3): se fijan con casos reales de la persona usuaria.
- Si el idioma de la tarea (español, inglés) exige normalizar acentos y plurales en la regla.
- Si el clasificador debe usar el mismo proveedor que el padre o uno más barato configurable.
