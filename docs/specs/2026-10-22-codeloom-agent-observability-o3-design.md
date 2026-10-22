# Observabilidad de agentes — Fase O3: causalidad y ficheros tocados

Estado: propuesta del 2026-10-22 (petición del usuario: seguir con la Fase O3). Plan:
`docs/plans/2026-10-22-codeloom-agent-observability-o3.md`.
Relacionado: `2026-10-21-codeloom-agent-observability-design.md` (Fase O3 allí
prevista), `2026-10-21-codeloom-agent-observability-o2-design.md`,
`2026-10-21-codeloom-agent-autonomy-design.md` (Fase A: el despertar del padre).

## Intención

CodeLoom se define por usar agentes y subagentes con eficiencia y poder verlos.
Tras las fases O1 y O2 se ve qué hace cada hijo, cuánto consume y se navega entre
padre e hijo. Faltan dos respuestas que el usuario se hace al mirar el rail:
*¿por qué ha hablado el padre si yo no he escrito nada?* y *¿qué ficheros ha
tocado cada hijo?*

## Objetivos

1. **Causalidad.** Cuando el padre arranca un turno despertado, el transcript lo
   dice: `⟳ auto-turn · despertado por «review» (completed)`, con la etiqueta del
   hijo y el motivo (`completed`, `failed`, `asks input`).
2. **Ficheros por hijo.** El rail muestra, en la línea de métricas de cada hijo,
   cuántos ficheros ha editado (`3f`), y el hijo abierto lista sus ficheros con
   `+añadidas/−quitadas` en su panel de cambios.

## No objetivos

- Persistir causalidad ni ficheros entre ejecuciones (ver «Limitaciones»).
- Grafo o árbol dibujado de quién despertó a quién; solo la causa inmediata.
- Ficheros creados por `bash` o por herramientas que no emiten `FileDiff`.
- Cambiar el esquema SQLite.

## Anti-objetivos

- Que el aviso de causa engorde el contexto del modelo: es una línea de la
  interfaz, no un mensaje del historial.
- Trabajo en `render` más allá de contar un conjunto y formatear una línea.
- Que un hijo con muchos ficheros ensanche el rail: la línea se recorta como el
  resto.

## Decisiones de diseño (mías, marcadas para revisión)

- **Causa del despertar.** La TUI ya sabe qué evento activa `wake_pending`
  (`SubagentSettled` con `Completed`/`Failed`, o `SubagentInputRequested`). Se
  guarda la última causa `(etiqueta del hijo, motivo)` en `App.wake_cause`; al
  empezar el turno despertado (`TurnStarted` con `wake_in_flight`) se emite una
  línea informativa en el transcript con esa causa y se vacía. La etiqueta sale de
  `App.subagents`; si no se conoce, `agent #<id>`.
- **Varias causas a la vez.** Si antes de despertar llegan varias, gana la más
  reciente y se añade `(+N más)` con el número de causas descartadas.
- **Ficheros por hijo.** `LiveEvent::FileDiff` pasa a ser relevante para hijos
  (`is_child_relevant` → true), así que llega envuelto en `ChildEvent`. La TUI
  guarda `files_by_session: HashMap<i64, BTreeMap<String, (usize, usize)>>` (sesión
  → ruta → añadidas/quitadas, la última gana, igual que `changes.upsert`), con
  **todas** las sesiones, padre incluido, para poder restaurar el panel al navegar.
  Se vacía al cargar una sesión que no sea navegación padre↔hijo
  (`nav_keep_stack`), igual que ya ocurre con el resto del estado de la jerarquía.
- **Rail.** Grupo `3f` en la línea de métricas, entre el coste y el tiempo; solo si
  el hijo tiene ficheros. Se calcula como `files_by_session[child].len()`.
- **Panel de cambios.** Al cargar una sesión (abrir un hijo o volver al padre), el
  panel de cambios (`App.changes`, hoy vacío tras cada carga) se rellena con
  `files_by_session` de la sesión cargada, de modo que el estado del padre no se
  pierde al ir y volver.
- **Fuente de verdad.** Memoria de la TUI. El esquema no cambia.

## Qué se deja al implementador

Texto exacto de la línea de causa, la posición del grupo de ficheros dentro de la
línea de métricas y nombres internos no fijados en el plan.

## Verificación

Pruebas puras de: el filtro de eventos, la acumulación por hijo, el reemplazo de la
misma ruta, la causa (única, múltiple, hijo desconocido), el grupo del rail y el
cambio de panel al abrir y volver. Sin red. Validación visual en terminal real:
pendiente.

## Limitaciones previstas

- Solo `edit` emite `FileDiff` hoy; lo que cree `bash` u otras herramientas no
  cuenta.
- Al cerrar y reabrir la sesión, la causa y los ficheros de los hijos se pierden.
  Reconstruir los ficheros desde las llamadas `edit` del historial de cada hijo
  evitaría tocar el esquema; queda como siguiente paso.
- La causa solo cubre al padre de la sesión abierta.

## Estado (2026-10-22)

Fase O3 implementada en dos tareas: ficheros por hijo (commit `3cde1dd`) y causa
del despertar en el transcript (pendiente de commit en el momento de escribir
esto). Verificado: `cargo test` en `codeloom-core` (147), `codeloom-tui` (364) y
`codeloom-cli` (3 + 3 + 1), `cargo clippy -p codeloom-core -p codeloom-tui --
-D warnings` y `rustfmt --check` limpios en `app.rs`, sin red.

Decisiones tomadas al implementar:

- El texto de la causa va en inglés, como los demás estados de la interfaz
  (`auto-turn`, `working`): `⟳ auto-turn · woken by «review» (completed)`. Con
  varias causas, `(+N more)` con las descartadas. La spec proponía castellano;
  el plan fijó el inglés y se ha seguido el plan.
- La causa se pinta como `LineKind::System` en el transcript, no como elemento
  de actividad. No entra en el historial del modelo ni se persiste.
- `wake_causes` se vacía al empezar el turno despertado, al pulsar Esc
  (`request_cancel`), al cambiar de sesión y cuando el presupuesto de turnos
  agotado descarta el despertar. Este último caso no estaba en la spec: sin él,
  las causas de un despertar descartado se contarían en el siguiente.
- Un turno del usuario no emite línea y no toca `wake_causes`.

Limitaciones reales:

- Causa y ficheros viven solo en memoria de la TUI: se pierden al cerrar y
  reabrir la sesión.
- `wake_causes` no tiene tope; en la práctica crece con el número de hijos que
  terminan o preguntan antes de un turno despertado.
- Solo `edit` emite `FileDiff`; lo que escriba `bash` no aparece.
- Validación visual en terminal real: pendiente.
