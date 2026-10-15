# CodeLoom — Idea/spec: una sola puerta de descubrimiento (tools/skills/MCP)

> Estado: **idea en exploración**. No implementar.
>
> Fecha: 2026-10-14
>
> Relacionado: `2026-10-14-loom-static-decision-signals.md` (los dos modos),
> `2026-10-14-loom-decision-hub-jevalan.md`,
> `2026-10-14-loom-mcp-skills-hot-reload.md`,
> `2026-10-14-loom-skills-ponytail-superpowers.md`,
> `2026-10-14-loom-memory.md`, `2026-10-14-loom-architecture-overview.md`.

## 0. La idea

Con **muchos MCP y muchas skills instaladas**, cargar todas sus tools/descriptions
en cada petición es carísimo en tokens y **diluye** la instrucción. La solución:
exponer **una sola puerta de descubrimiento**. El modelo conoce **un** mecanismo
("busca la tool/skill que necesites") y el **subconjunto relevante** se carga solo
cuando hace falta.

Es la misma decisión de §4 del mapa, en dos modos:

- **Estático** (señales/keyword, coste ~0): casos evidentes y frecuentes.
- **jevalan** (clasificación aprendida): cuando lo estático no lo ve.

El pago: **N esquemas en cada turno -> 1 puerta + un puñado**.

## 1. El problema en números (por qué duele)

Cada tool aporta su **JSON Schema** y cada skill su **description** al prefijo de
la petición. Con `M` servidores MCP y `K` skills eso son **miles de tokens en cada
turno**, y encima:

- **Dilución**: 60 tools compiten por la atención; baja la precisión de elección.
- **Cache**: añadir/quitar tools cambia el prefijo (prompt cache), ver hot-reload.
- **Coste**: se paga aunque la tarea no use ninguna de esas tools.

## 2. Diseño: revelado progresivo con una puerta

```
        turno
          │
   ┌──────▼────────────────────────────────────────────┐
   │ Tier 0 — núcleo SIEMPRE cargado (pocas tools)      │
   │ view · edit · bash · glob · grep  +  discovery     │
   └──────┬─────────────────────────────────────────────┘
          │
   ┌──────▼─────────────────────────────────────────────┐
   │ discovery("qué necesito")                          │
   │  modo estático (keyword) -> resuelve ya             │
   │  modo jevalan (aprendido) -> si no, decide          │
   │  salida: subconjunto de tools/skills/MCP            │
   └──────┬─────────────────────────────────────────────┘
          │  se cargan SOLO esas
   ┌──────▼─────────────────────────────────────────────┐
   │ Tier 1 — long tail bajo demanda                     │
   │ tools MCP · skills · memorias · fragmentos          │
   └────────────────────────────────────────────────────┘
```

- **Tier 0** (siempre): las tools que se usan en casi todo turno (`view/edit/bash/
  glob/grep`) más la **puerta de descubrimiento**. Pocas y estables (bueno para el
  cache).
- **Tier 1** (bajo demanda): todo lo demás (MCP, skills, memoria).
- **La puerta** es una capacidad que, dada la tarea, devuelve el subconjunto
  relevante. Se resuelve **en frontera de step** (frontera de decisión), y se
  emiten eventos (`discovery/resolved { loaded: [...] }`).

## 3. Los dos modos (aquí está la gracia)

| | Estático | jevalan |
|---|---|---|
| Cuándo | caso evidente/frecuente | lo estático no lo ve |
| Coste | ~0, determinista | ~100 ms, clasificador |
| Ejemplo | "git" -> carga `bash`/skill git | "arregla el pipeline de CI" -> decide qué MCP/skill |

Es **el mismo** reparto del mapa (§4): determinista primero, clasificador después,
LLM nunca para decidir. Y como la decisión es de **tools/skills**, también ahorra
tokens **indirectamente** (menos esquemas cargados).

## 4. Qué ahorra, en concreto

- **Prefijo**: N schemas -> 1 puerta + subconjunto. Menos tokens de entrada por
  turno, y menos que se diluya la elección.
- **Skills**: en vez de inyectar todas las `SKILL.md`, solo la relevante
  (conecta con el spec de skills y con la memoria por demanda).
- **MCP**: se listan/activan bajo demanda (conecta con `list_changed` y hot-reload).
- **Memoria**: igual patrón (cargar por demanda).

## 5. Gotchas

- **El modelo debe saber que puede descubrir**: la puerta tiene que estar en el
  prompt y explicada; si no, no la usa. Un "si no encuentras la tool, búscala".
- **No esconder el núcleo**: Tier 0 siempre disponible; ocultar `edit` o `bash`
  rompe cosas.
- **Cache**: si el subconjunto cargado cambia, cambia el prefijo. Aplicar en
  frontera de step y mantener **estable** la puerta (mismo schema siempre).
- **Latencia de descubrimiento**: una vuelta extra; compensa con la decisión
  estática (que resuelve sin round-trip) y cacheando resoluciones.
- **Riesgo de no descubrir**: si el modelo no encuentra una tool que existía,
  necesita un fallback ("mostrar todo" o una pista del porqué).
- **Seguridad**: cargar una tool **no** concede permisos; el gate (`tool.risk`)
  sigue aplicando.
- **No inventar un MCP que lo envuelva todo**: es una **capa de CodeLoom**
  (resolución + subset), no un servidor nuevo; y el subset se resuelve local.

## 6. Relación con lo ya documentado

- Es el **contrato de control** (§9 del mapa): la activación/carga es estado de
  primera clase, con eventos, resuelta en frontera de step.
- Reutiliza el **registro versionado** de tools (hot-reload) y el `SkillIndex`.
- Es el mecanismo que hace **sostenible** tener muchas skills/MCP: sin él, "muchas
  instaladas" = "muchos tokens cada turno".

## 7. Preguntas abiertas

- ¿La puerta es **una tool** (`discover`) o un **modo del sistema** (pre-step)?
- ¿Se cachea la resolución por tarea/sesión para no repetir el round-trip?
- ¿Tier 0 es fijo o configurable? ¿umbral de "cuántas tools" activa la puerta?
- ¿Coexiste con la "MCP tool search" del propio protocolo MCP?
