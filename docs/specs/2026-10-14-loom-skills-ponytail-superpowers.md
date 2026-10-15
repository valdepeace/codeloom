# CodeLoom — Idea/spec: consumo y activación de skills (ponytail, superpowers)

> Estado: **idea en exploración**. No implementar.
>
> Fecha: 2026-10-14
>
> Relacionado: `docs/specs/2026-10-13-loom-design.md` (§5.5 extensiones, §6
> subagentes, §4.3 step-loop) y los specs de
> `2026-10-14-loom-mcp-skills-hot-reload.md` y
> `2026-10-14-loom-decision-hub-jevalan.md`.
> Fuentes: skills `superpowers` (suite de proceso) y `ponytail` (gobernanza de
> alcance), en `~/.agents/skills/`.

## 0. La idea

CodeLoom debe poder **consumir skills** de base (empezando por `superpowers` y
`ponytail`) y **decidir cuáles están activas** en cada momento: activas siempre,
apagadas, invocadas a mano, o **elegidas por un clasificador** (jevalan). Como
las dos familias pueden **contradecirse**, CodeLoom necesita también una **regla de
arbitraje** explícita, no dejar que se pisen.

## 1. Qué son las dos familias

- **`superpowers`** — suite de **skills de proceso** que fijan *cómo* se trabaja:
  `brainstorming` -> `writing-plans` -> `executing-plans` /
  `subagent-driven-development`, más `systematic-debugging`,
  `test-driven-development`, `verification-before-completion`,
  `requesting/receiving-code-review`, `using-git-worktrees`,
  `finishing-a-development-branch`, `writing-skills`,
  `dispatching-parallel-agents`. Su regla base: *invocar la skill relevante antes
  de cualquier acción*, y las instrucciones del usuario mandan sobre las skills.
  Escriben **especificaciones y planes** y exigen verificación.

- **`ponytail`** — gobernanza de **alcance**: obliga a la solución más perezosa
  que funcione (escalera YAGNI), sin abstracciones no pedidas ni andamiaje "para
  luego". Tiene intensidad `lite | full | ultra` y salida "código primero, luego
  ≤3 líneas". Pero **no es descuidada**: "never lazy about understanding the
  problem", y en validación de límites de confianza, manejo de errores, seguridad
  y accesibilidad **no simplifica**, y "anything explicitly requested" se
  construye entero.

## 2. La tensión (documentada a propósito)

El usuario apunta un conflicto real: **uno escribe especificaciones; el otro
puede ser tan escueto que no deje configurar la arquitectura.**

| Eje | `superpowers` | `ponytail` | Riesgo |
|---|---|---|---|
| Especificar antes de codificar | spec + plan (arquitectónico) | "no essays, no design notes", menos es más | ponytail puede saltarse el spec que la tarea sí necesitaba |
| Abstracciones | las que el diseño pida | ninguna no pedida | chocan cuando la arquitectura sí quiere una interfaz |
| Documentación | spec escrito, revisable | prosa no pedida = deuda | ponytail puede dejar sin documentar decisiones |
| Rigor | TDD, verificación, review | una sola comprobación runnable | ponytail recorta el proceso |

**No es contradicción total, y las propias skills lo admiten:**
- `ponytail` dice *"never lazy about understanding the problem"* y *"anything
  explicitly requested"* se construye; si el usuario pide arquitectura, la pide.
- `superpowers` (brainstorming) **escala el artefacto al tipo de tarea**:
  arquitectónico -> spec + plan; acotado -> diseño corto en chat.

**Regla de arbitraje propuesta:**
- **`ponytail` gobierna el *tamaño* de la solución; `superpowers` gobierna el
  *proceso*.** No son el mismo eje, así que pueden coexistir sin pisarse.
- **En tarea arquitectónica gana `superpowers`** (el spec/plan es el trabajo
  explícito) y `ponytail` **baja a `lite` o se apaga**: deja de recortar el
  diseño/arquitectura, pero sigue vigilando el alcance dentro de cada pieza.
- **En tarea acotada gana `ponytail` (`full`)**; de `superpowers` solo lo barato
  (`verification-before-completion`, la comprobación runnable).

## 3. Modelo de skills en CodeLoom

Una skill es un **`SKILL.md` con frontmatter** que aporta **fragmentos de prompt
(policy)** y, opcionalmente, scripts/tools. Encaja con lo ya diseñado:

- **Descubrimiento + hot-reload**: `SkillIndex` versionado con watcher
  (ver `2026-10-14-loom-mcp-skills-hot-reload.md`). Cambiar un `SKILL.md` no
  exige reiniciar.
- **Resolución por turno**: un `SkillResolver` corre al inicio del turno (o en un
  `agent/pre-step` tipo dsh) y produce el **conjunto activo** -> fragmentos de
  prompt. Se aplica en **frontera de step**, nunca a mitad de stream (coherente
  con hot-reload y con el prompt cache).
- **Alcance**: sesión, subagente o turno. Un builder puede llevar `ponytail`; un
  crítico del Gauntlet **no** (juzga con su propio criterio).

## 4. Activación

Modos por skill:

| Modo | Qué hace |
|---|---|
| `off` | nunca activa |
| `on` | siempre activa (p. ej. `verification-before-completion`) |
| `explicit` | solo cuando el usuario la invoca (`/ponytail ultra`) |
| `auto` | la decide un clasificador (jevalan, `skill.route`) según la tarea |

Además, `ponytail` lleva **intensidad** (`lite`/`full`/`ultra`), que es lo que el
usuario puede fijar a mano o dejar que el clasificador elija.

Ejemplos:

- "añade un flag a este comando" -> `ponytail: full`, superpowers: solo
  verificación.
- "rediseña el motor de subagentes" -> `superpowers: brainstorming+writing-plans`,
  `ponytail: lite`.
- "arregla este bug intermitente" -> `superpowers: systematic-debugging`, luego
  implementación.

## 5. Decisión por jevalan

La activación `auto` es una **clasificación barata**, no una llamada cara:

- **Capacidad `skill.route`** (nueva, servida por flujo jevalan): preguntas
  `noul`/`score` como *"¿la tarea es arquitectónica o acotada?"*, *"¿toca
  seguridad/datos de producción/dinero?"* y *"¿hay barra inspeccionable?"*.
- Salida: el **conjunto de skills** y la intensidad de `ponytail`.
- Se apoya en el hub de decisiones (`2026-10-14-loom-decision-hub-jevalan.md`):
  la misma llamada de ~100 ms que enruta el modelo puede enrutar las skills.
- Si jevalan no responde: **valores por defecto conservadores** (proceso mínimo +
  `ponytail full` en tareas acotadas), nunca arrancar el modo caro a ciegas.

## 6. Arbitraje de conflictos

Pieza central, porque las dos familias pueden pisarse. Reglas, en orden:

1. **Instrucción explícita del usuario** manda (la misma regla que declara
   `using-superpowers`: usuario > skills).
2. **Skill de proceso (`superpowers`) vs skill de alcance (`ponytail`)**: cada
   una en su eje; el conflicto real (spec vs minimalismo) se resuelve por **tipo
   de tarea** (§2).
3. **Dos skills del mismo eje en conflicto**: gana la más específica a la tarea;
   empate -> la que fije `codeloom.toml`.
4. **Nunca apilar sin presupuesto**: cada fragmento de skill consume contexto y
   puede cambiar el prefijo de la petición (prompt cache). El resolver tiene un
   **tope** y prioriza.

El conflicto `ponytail`(ultra) vs arquitectura se resuelve **downgradeando
ponytail**, no apagando superpowers: se mantiene el vigilante de alcance sin
que decida el tamaño de la arquitectura.

## 7. Eventos y UI

- **Durable**: `skills/resolved { generation, active, modes }` (replay).
- **Live**: cambios en el header (como el flock): qué skills están activas y en
  qué modo/intensidad.
- Transparencia: que un cambio de comportamiento ("ahora `ponytail ultra`") se
  vea, no que sea magia invisible.

## 8. Config (borrador)

```toml
[skills]
hot_reload = true

[skills.superpowers]
mode = "auto"          # off | on | explicit | auto

[skills.ponytail]
mode = "auto"
intensity = "full"     # lite | full | ultra  (auto lo puede ajustar)

[skills.conflict]
# en tarea arquitectónica, degradar ponytail en vez de apagar superpowers
architectural_ponytail = "lite"
```

## 9. Gotchas

- **Pueden contradecirse**: sin arbitraje explícito, el resultado es un prompt
  incoherente. Por eso §6 existe.
- **`ponytail ultra` y arquitectura no se llevan**: ultra recorta justo lo que un
  diseño pide; degradar a `lite` en arquitectónico.
- **El prompt cache**: activar/desactivar skills cambia el prefijo; aplicar en
  frontera de step y agrupar cambios.
- **Sobre-apilamiento**: demasiadas skills inflan el contexto y diluyen la
  instrucción; el resolver tiene tope y prioridad.
- **Herencia a subagentes**: decidir si un hijo hereda las skills del padre
  (dsh usa `tool_filter`/`persona` por subagente); el crítico no debe heredar el
  sesgo del builder.
- **`ponytail` no es descuidada**: no usa su "pereza" para saltarse validación,
  manejo de errores, seguridad ni lo pedido explícitamente.
- **Dependencia externa**: las skills viven fuera del repo (`~/.agents/skills`),
  como herdr/jevalan; CodeLoom las consume, no las vendoriza.

## 10. Preguntas abiertas

- ¿El resolver vive en `codeloom-core` (con las skills como datos) o en un crate
  opcional?
- ¿La precedencia se resuelve por tipo de tarea (clasificador) o por un orden
  fijo declarado?
- ¿Cómo se representa una skill que trae **tools/scripts** (no solo prompt)?
- ¿Se expone la activación en la TUI (`/ponytail ultra`) o solo en `codeloom.toml`?
- ¿Firma la skill de gobernanza (ponytail) su propia degradación, o la decide el
  clasificador?
