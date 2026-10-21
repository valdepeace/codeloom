# Observabilidad de agentes y subagentes

Estado: aprobada en conversación el 2026-10-21 (el orden: observabilidad antes
que las fases B y C de autonomía). Plan:
`docs/plans/2026-10-21-codeloom-agent-observability.md`.
Relacionado: `2026-10-21-codeloom-agent-autonomy-design.md`,
`2026-10-15-loom-agent-profile-a2a.md`, `2026-10-14-loom-architecture-overview.md`.

## Intención

CodeLoom nace para usar agentes y subagentes con eficiencia **y poder verlos**,
algo que otros editores no ofrecen. Hoy la interfaz solo sabe de un subagente
su etiqueta, si está en marcha, si espera respuesta y su resultado final: es un
indicador de estado, no observabilidad. La causa es concreta: cada hijo se crea
con un bus de eventos propio y aislado (`subagent.rs`), de modo que la
interfaz no recibe nada de su interior (pasos, herramientas, tokens), y
`LiveEvent::Usage` carece de sesión, por lo que el gasto no se atribuye.

## Objetivos

1. Que la interfaz reciba, etiquetados por agente, los eventos relevantes del
   interior de cada hijo (pasos, herramientas, uso de tokens, errores).
2. Métricas vivas por agente: paso actual de N, herramienta en curso, tokens de
   entrada y salida, tiempo transcurrido y estado de tarea (A2A).
3. Totales de la jerarquía (tokens y número de agentes activos).
4. Que todo ello se vea en el rail sin costar fluidez al composer.

## No objetivos (fases posteriores de esta línea)

- Vista de árbol completa y navegación a la sesión de un hijo (Fase O2).
- Causalidad: quién despertó a quién, artefactos producidos (Fase O3).
- Persistir métricas en SQLite o histórico entre ejecuciones.
- Reenviar el texto en streaming (`Delta`, `ReasoningDelta`) de los hijos.

## Anti-objetivos

- Que la observabilidad ralentice el dibujado o el teclado: sin temporizadores
  nuevos, sin trabajo en `render` más allá de formatear unas cifras.
- Que un hijo muy verboso sature el bus del padre.
- Que los eventos de un hijo se mezclen con el transcript o los totales del
  padre (hoy los manejadores asumen que todo evento es de la sesión abierta).

## Decisiones de diseño

- **Un evento envoltorio**, `LiveEvent::ChildEvent { parent, child, event }`
  con `event: Box<LiveEvent>`, en lugar de duplicar cada variante con sesión.
  Los manejadores existentes no lo conocen y por tanto no pueden confundirlo
  con un evento propio; la interfaz lo despacha aparte.
- **Reenvío filtrado en el lanzador**: una tarea por hijo suscrita a su bus
  reenvía solo `TurnStarted`, `StepStarted`, `ToolStarted`, `ToolFinished`,
  `Usage`, `TurnEnded` y `Error`. Quedan fuera los deltas (volumen) y el resto.
- **Estadísticas en la interfaz** (`AgentStats` por id de hijo): se alimentan
  solo de `ChildEvent`; no se persisten. Se limpian al cambiar de sesión.
- **Sin temporizador propio**: el tiempo transcurrido se calcula al dibujar a
  partir de un `Instant`; se refresca con cualquier otro redibujado y no fuerza
  fotogramas.
- **Rail**: cada subagente ocupa dos líneas (etiqueta con estado y una línea
  tenue de métricas) y el título lleva el total de la jerarquía. Se mantiene el
  límite de cinco subagentes visibles.

## Qué se deja al implementador

Formato exacto de las cifras (`1.2k`), recorte por anchura (el rail mide 32
columnas) y nombres internos no fijados en el plan.

## Verificación

Pruebas del reenvío (etiquetado, filtro, cierre al terminar el hijo), de
`AgentStats` (pasos, herramienta en curso, tokens acumulados, limpieza) y del
rail (líneas de métricas y recorte). Validación visual en terminal real:
pendiente.
