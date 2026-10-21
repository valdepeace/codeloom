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

## Estado (2026-10-21)

Implementado, cada tarea en su commit: catálogo, configuración y petición
(`99b7951`), traducción por proveedor (`e842f24`) y campo «Effort» con nivel en
el rail (commit de la tarea 3).

Cómo se usa: en `/settings`, tras elegir un modelo que razona, aparece la fila
`Effort` justo antes de «Save and apply». Con el foco en ella, `←`/`→` (o `Enter`)
recorren «default» y los niveles del modelo. Guardar lo escribe en
`codeloom.toml` (`effort`) y el rail lo muestra como `effort high` bajo el modelo.

Decisiones tomadas al implementar:

- El campo ocupa el índice 4 solo si es visible; sin él, «Save and apply» sigue
  en el 4 y los demás índices no cambian.
- El nivel se reajusta a «default» (con aviso) al cambiar modelo, proveedor o URL
  si el nuevo destino no lo admite.
- En el proveedor OpenAI-compatible el nivel se envía tal cual; solo Anthropic
  valida contra su tabla de presupuestos.
- El rail lee el nivel de lo guardado, igual que el modelo.

Limitaciones conocidas:

- Los presupuestos de Anthropic (`minimal` 1024, `low` 2048, `medium` 8192, `high`
  16384, `max` 32000) y la forma `reasoning: {effort}` de OpenRouter son
  suposiciones **sin contrastar con los servicios reales**.
- Los modelos de Anthropic más recientes podrían preferir un esfuerzo nativo
  frente a `budget_tokens`; no se ha comprobado.
- `toggle` y el presupuesto numérico libre no están soportados.
- Los subagentes heredan el esfuerzo de la configuración del padre.
- Validación visual en terminal real: pendiente.
