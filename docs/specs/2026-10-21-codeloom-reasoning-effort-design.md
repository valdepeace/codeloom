# Esfuerzo de razonamiento por modelo (effort)

Estado: propuesta del 2026-10-21 (petición del usuario: «seguimos poniendo también
el effort de un modelo, ya que tenemos acceso a ese dato por models.dev»).
Plan: `docs/plans/2026-10-21-codeloom-reasoning-effort.md`.
Relacionado: `2026-10-21-codeloom-models-catalog-design.md`.

## Intención

Los modelos que razonan admiten graduar cuánto piensan. models.dev lo describe
por modelo en `reasoning_options` (verificado el 2026-10-21 sobre 6908 modelos
con `reasoning: true`): una lista de opciones de tipo `effort` (con `values`, p. ej.
`low`/`medium`/`high`/`max`), `budget_tokens` (con `min`/`max`) o `toggle`. Se pretende
elegir el esfuerzo en `/settings` con las opciones reales del modelo y que llegue
a la petición del proveedor.

## Objetivos

1. El catálogo expone por modelo qué admite: valores de `effort`, rango de
   `budget_tokens` y si hay `toggle`.
2. `/settings` muestra un campo «Effort» (solo si el modelo admite `effort`), que
   cicla con `←`/`→` entre «default» y los valores del modelo.
3. El valor se guarda en `codeloom.toml` (`effort`) y se envía en cada petición.
4. El nivel activo se ve en el rail, junto al modelo.

## No objetivos

- Mostrar el razonamiento de otra forma (ya existe `ReasoningDelta`).
- `toggle` puro (activar/desactivar sin gradación): se trata como «default».
- Elegir presupuesto numérico libre: solo los niveles con nombre.

## Anti-objetivos

- Enviar parámetros de razonamiento a modelos que no los admiten: con
  «default» o modelo no razonador la petición queda **idéntica** a la de hoy.
- Romper proveedores que rechazan campos desconocidos.

## Decisiones de diseño (mías, marcadas para revisión)

- `CodeLoomConfig.effort: Option<String>` y `ProviderRequest.effort: Option<String>`.
- **OpenAI-compatible:** `reasoning_effort: "<nivel>"`; si la URL base contiene
  `openrouter.ai`, en su lugar `reasoning: {"effort": "<nivel>"}` (forma nativa de
  OpenRouter).
- **Anthropic:** se traduce el nivel a presupuesto con la tabla fija
  `minimal`=1024, `low`=2048, `medium`=8192, `high`=16384, `max`=32000; se envía
  `thinking: {"type":"enabled","budget_tokens":N}`, se sube `max_tokens` a
  `N + max_tokens` y se omite `temperature` (la API la restringe con `thinking`).
  Nivel desconocido: no se envía nada.
- Si el modelo cambia y el nivel guardado ya no figura entre sus valores, se
  descarta (vuelve a «default») y se avisa en `settings_feedback`.

## Qué se deja al implementador

Posición del campo nuevo en el diálogo, formato de la etiqueta en el rail y
nombres internos no fijados en el plan.

## Verificación

Pruebas puras de: parseo de `reasoning_options`, cuerpo de petición de cada
proveedor con y sin esfuerzo (la sin esfuerzo idéntica a la actual), configuración
(guardar y leer), campo del diálogo (visibilidad, ciclo, descarte al cambiar de
modelo) y rail. Sin red. Validación visual y contra proveedores reales: pendiente
(la tabla de presupuestos y la forma de OpenRouter son suposiciones a comprobar).
