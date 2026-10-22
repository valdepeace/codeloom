# Decisiones locales con Jev, Kev y Laya (jevalan): tareas pendientes

Estado: documento de referencia del 2026-10-22. No es un plan de ejecución para
agentes: cada tarea que se emprenda necesita antes su spec y su plan.

Complementario, pero deliberadamente separado, de
`2026-10-22-codeloom-a2a-roadmap.md`. Por ahora las dos líneas avanzan por separado.

## Contexto

`jevalan` es un hub local de decisiones: el agente le pregunta antes de actuar y
recibe una decisión tipada, sin gastar una llamada al LLM grande. **Jev** (nube),
**Kev-4B/0.8B** (GPU, puerto 8009) y **Laya** (CPU, puerto 8021) sirven el mismo
contrato `POST /v1/systemone` y devuelven probabilidades tipadas, no texto.

Fuente en este repositorio: `docs/specs/2026-10-14-loom-decision-hub-jevalan.md`
(**idea en exploración, «no implementar»**), con apoyo en
`2026-10-14-loom-static-decision-signals.md`, `2026-10-14-loom-decision-experiments.md`
y `2026-10-14-loom-gauntlet-loop.md`. El proyecto `jevalan` en sí
(`E:\workspace\personal\jevalan`) **no se ha leído** al redactar esta lista: la primera
tarea es leerlo. Las cifras de latencia de la spec son del 2026-10-10 y proceden del
propio jevalan, sin verificar desde aquí.

## Guardas (de la spec del hub)

Clasificar solo decisiones frecuentes, baratas de clasificar, recuperables o con gate y
sin regla determinista; **regla antes que clasificador**; no autoaprobar por confianza
alta; presupuestar la latencia por decisión; los errores de jevalan y del LLM están
correlacionados (~96 % según la spec), así que apilarlos no añade precisión.

## Tareas

- [ ] **J1. Leer `jevalan`** (solo lectura) y los informes de `gauntlet-decision-lab`:
  contrato real de `/v1/systemone`, capacidades disponibles, latencias vigentes.
  Anotar discrepancias con la spec del hub.
- [ ] **J2. Promover la spec del hub** de «idea, no implementar» a diseño, con
  decisiones marcadas: qué capacidades entran primero, qué ocurre sin hub, qué se
  persiste.
- [ ] **J3. Rasgo `DecisionProvider` en el núcleo** (puro, simulable, sin red en
  tests) con tipos de decisión tipados y probabilidades.
- [ ] **J4. `tool.risk` en `PermissionService`:** decisión `allow | ask | deny` antes
  de ejecutar una herramienta, con reglas fijas primero. Sin hub o con tiempo agotado
  se vuelve a `ask`; jamás a `allow`.
- [ ] **J5. Cliente HTTP del hub** en crate opcional, desactivado por defecto, con
  tiempo máximo, reintento acotado y degradación elegante.
- [ ] **J6. `agent.wakeup`:** que el despertar del padre (Fase A de la autonomía) pueda
  consultar al hub antes de gastar un turno. Aprovecha la causa ya registrada en
  `wake_causes`; añadir la fuente de la decisión a la línea del transcript.
- [ ] **J7. `model.route`:** `{ model, effort }` por turno. Se apoya en el campo
  `effort` ya existente y en los precios del catálogo; medir el ahorro antes de
  activarlo por defecto.
- [ ] **J8. Observabilidad de las decisiones:** mostrar en el rail o el transcript la
  decisión, su fuente, probabilidad y latencia, y contabilizar las llamadas grandes
  evitadas junto al coste del libro de consumo.
- [ ] **J9. Experimentos y KPIs** para validar que clasificar compensa, antes de
  ampliar a `skill.route`, `memory.route` y demás.

## Cruces con la línea de A2A

- J6 (despertar con hub) y A2 (productores de estados) comparten el momento del acuse
  del hijo.
- J4 y `rejected`: un permiso denegado por `tool.risk` es candidato a rechazo del
  agente, pero la especificación de A2A no lo dice; decidirlo explícitamente.
