# CodeLoom — Idea/spec: Gauntlet Loop (builder + crítico ciego)

> Estado: **idea en exploración**. No implementar.
>
> Fecha: 2026-10-14
>
> Relacionado: `docs/specs/2026-10-13-loom-design.md` (§4.3 turn/step y StopWhen,
> §6 subagentes/jobs, §5.4 permisos, §5.5 extensiones) y
> `2026-10-14-loom-decision-hub-jevalan.md`.
> Fuente: `gauntlet-decision-lab` (`E:/workspace/personal/gauntlet-decision-lab`),
> informes `reports/Gauntlet Loop metodo agentes.md` y
> `reports/informe-jev-gauntlet-loop.md`.

## 0. La idea

Añadir a CodeLoom el **Gauntlet Loop** como método **opcional y activable en ciertos
casos**: para tareas con un **objetivo de calidad** y una **barra real
inspeccionable**, en vez de un solo agente que dice "ya está", se divide el
trabajo, cada pieza la pule un **constructor** y la juzga un **crítico
independiente con contexto limpio** que compara **a ciegas** contra la barra y
devuelve la mayor brecha. Se repite hasta que el humano para, la mejora es
marginal o se agota el presupuesto.

No es un modo por defecto: es caro (muchas rondas) y solo gana donde la calidad
subjetiva cuenta. Por eso se **activa en determinados casos**.

## 1. Qué es el método (reconstrucción)

Propiedades que definen el Gauntlet Loop (del prompt original y la guía del
autor, según el informe):

1. **Objetivo, no implementación** ("give it the destination, let it choose the
   route").
2. **Barra real e inspeccionable** (una referencia concreta que el agente pueda
   mirar; p. ej. capturas reales de Call of Duty).
3. **Descomposición** por el agente líder en las piezas más pequeñas que se
   puedan mejorar y juzgar por separado.
4. **Constructor y crítico separados** ("never let the builder grade itself");
   el crítico corre con **contexto fresco**.
5. **Comparación ciega A/B** contra la barra ("without being told which is
   which").
6. El crítico juzga el **artefacto real** (píxeles, producto en ejecución,
   página renderizada, tests), **nunca un resumen escrito por el constructor**.
7. Si gana la barra, el crítico explica **la mayor brecha restante** y devuelve
   el trabajo al constructor.
8. **Sin ronda final arbitraria**: se para cuando lo decide el humano, la mejora
   es marginal o se agota el presupuesto.
9. Opcional: **pase de suavizado** al cerrar una oleada, para reconciliar piezas.

```
 OBJETIVO + BARRA REAL ──► dividir en piezas ──► por pieza:
                                      builder ──► artefacto
                                         ▲            │
                                         │            ▼
                                    mayor brecha ◄── critic (contexto limpio)
                                         │       A/B CIEGO vs barra
                                    ¿gana el artefacto?  sí ─► pieza fuera del loop
                                    no ─► vuelve al builder
 para: humano / mejora marginal / presupuesto agotado · suavizado final
```

## 2. Lo que la evidencia corrige (no copiar el prompt a ciegas)

El informe del lab separa lo que ordena el prompt de lo que **muestra la
ejecución real**. Cuatro correcciones que CodeLoom debe incorporar:

- **El paralelismo pierde.** "Sequential single-owner passes beat parallel
  fan-out decisively": pases secuenciales con **un dueño por preocupación
  acoplada** dieron +1.00 y bajaron defectos 66 -> 26; el fan-out en paralelo dio
  +0.46 y **subió** los defectos (60 -> 47 -> 66). CodeLoom debe favorecer
  **secuencial por preocupación acoplada**, no repartir a lo loco.
- **El crítico juzga el artefacto, no un resumen.** Y puede equivocarse: "every
  critic for three rounds reported the weapon as untextured; it wasn't". Hay que
  darle **acceso al artefacto real**, no a la narración del constructor.
- **Contaminación entre iteraciones.** Reutilizar la misma sesión/página entre
  medidas ensucia el resultado ("two identical runs differed on 10 of 11
  shots"). Cada ronda debe empezar en **estado fresco**; medir con un baseline
  aislado.
- **La métrica puede engañar.** "Median frame time hides the actual problem: a
  static-camera benchmark reported 94 fps while the game was unplayable." Elegir
  la métrica es parte del diseño, no un detalle.

Y del informe sobre Jev dentro del Gauntlet: **Jev solo como filtro**, nunca como
crítico (ver §5).

## 3. Cómo encaja en el modelo de CodeLoom

El Gauntlet es un **patrón de orquestación sobre subagentes**, y CodeLoom ya diseña
justo eso (§6):

| Rol del Gauntlet | Pieza de CodeLoom | Notas |
|---|---|---|
| Agente líder que descompone | el agente principal (session) | usa su propio loop |
| Constructor (una pieza) | `trait SubagentProvider` in-process | child con su `AgentLoop` y `Session` |
| Crítico (contexto limpio) | **otro** subagente, `tool_filter` de solo lectura | jamás el mismo child que el builder |
| Barra real | ruta/paths a la referencia + una tool de inspección | el crítico la lee/ejecuta |
| Herramientas del crítico | `view`/`glob`/`grep` + capture/benchmark | read-only; sin escribir el artefacto |
| Estado en vivo | catálogo del header ("flock") | builders/críticos con `StateDot` |
| Parada | `StopWhen` (§4.3) | humano, mejora marginal, presupuesto |
| Subagentes largos | jobs (§6.2) | una pieza = un job con su ring |

Claves del encaje:

- **Contexto limpio por crítico**: ya es natural en el modelo de subagentes de
  dsh/CodeLoom (child con sesión propia). El point central es **no** reusar el
  contexto del builder.
- **A/B ciego**: el orquestador **aleatoriza el orden** artefacto/barra y
  registra el veredicto. Medir el **cambio al invertir el orden** es una métrica
  obligatoria (sesgo de posición).
- **Artefacto real**: el crítico recibe las mismas tools de inspección que el
  humano usaría para comprobar (incluida la tool `bash` para correr tests/bench),
  no el diff narrado.
- **Pase de suavizado**: un último subagente que reconcilia piezas antes de
  cerrar la oleada.
- **Sin ronda final fija**: `StopWhen` por mejora marginal (delta de la métrica
  por debajo de un umbral) y por presupuesto (steps/tokens/tiempo).

## 4. Activación "en determinados casos"

El Gauntlet no se enciende siempre. Tres formas de decidir:

1. **Explícito**: un modo/comando (`/gauntlet`) o una tool `gauntlet` que arranca
   el patrón sobre un objetivo y una barra.
2. **Por clasificador**: preguntar a un modelo de decisión barato (jevalan,
   `noul`/`score`) "¿esta tarea tiene un objetivo de calidad y una barra
   inspeccionable, y merece el coste de un crítico ciego?" -> sí/no. Es una
   llamada de ~100 ms que evita arrancar el loop caro cuando no toca.
3. **Heurísticas de disparo**:
   - Hay una **barra concreta** que el agente puede inspeccionar (referencia,
     captura, ejemplo, benchmark objetivo).
   - La calidad es **subjetiva/de conjunto** (UI, redacción, diseño, jugabilidad,
     refactor de estilo), no un sí/no determinista.
   - **No** la activar donde basta una puerta barata: tests, compilación,
     lint, una comprobación binaria legible del texto (§5).

Regla de oro: si el criterio de "bien" no es inspeccionable ni comparable, el
Gauntlet degenera en un bucle caro.

### 4.1 Control, estado y anidamiento

Tres preguntas: ¿cómo se controla?, ¿lo detectan las iteraciones siguientes?,
¿puede un subagente entrar en él?

**La activación es un estado de primera clase, no una variable oculta.**

- Cada gauntlet tiene un **`id`** y una **`generation`**, y vive en la **sesión** y
  en el **bus durable**: `gauntlet/started { id, objective, bar, pieces, budget,
  who }`, `gauntlet/round { id, scores }`, `gauntlet/stopped { id, reason }`.
  `who` = `user | lead | router | subagent`. Visible en el **header** (flock:
  builders/críticos con `StateDot`).
- **Detección en iteraciones siguientes**: el step-loop resuelve los modos activos
  al inicio de cada step (mismo patrón que skills/memoria, frontera de step). Un
  step posterior **sabe** que está dentro de un gauntlet, en qué **ronda**, con
  qué **presupuesto** restante; y el log durable lo **replaya**. No hay que
  "adivinar": se lee del estado.

**Quién puede activarlo y hasta dónde (anidamiento):**

| Rol | ¿Abre gauntlet? | Regla |
|---|---|---|
| Agente principal | sí | explícito, tool `gauntlet` o `gauntlet.route` |
| **Crítico** (subagente) | **no** | es read-only y no debe convertirse en builder abriendo otro loop (regresión infinita) |
| **Builder** (subagente) | por defecto **no** | si se permite, con **profundidad máxima dura** y prohibido dentro de un crítico |
| Pase de suavizado | no | solo reconcilia, no abre rondas |

- **Guardia de profundidad**: como dsh, `max_depth` (def. **1**) y
  `max_active_subagents` (def. **8**). Un gauntlet anidado pide permiso al
  presupuesto del padre o tiene **techo propio**.
- **Filtro por subagente**: un subagente solo puede invocar lo que esté en su
  `tool_filter`; el crítico no lleva la tool `gauntlet`.
- **Idempotencia**: activar dos veces no abre dos gauntlets; se detecta por `id` y
  estado.

**Control:** `start` / `stop`, cancelación con `CancellationToken`, y `StopWhen`
(humano / mejora marginal / presupuesto). Todo se registra para poder **medir** el
coste real (KPIs) y decidir si mereció la pena.

## 5. El rol de Jev (el filtro)

Del informe: **"usar solo como filtro"**, nunca como crítico ni sustituto.

- **Sí** donde la evidencia es fuerte:
  - **F1, comprobación binaria legible del texto** (¿incluye el fichero X?, ¿el
    JSON cumple el esquema?, ¿aparece el término prohibido?) — `noul`/`choice`.
  - **F2, A/B de texto** (artefacto vs referencia, ambos textuales) — como
    *prior* de preferencia.
- **No** en imágenes/píxeles (Jev es solo texto), **no** ejecutando código, **no**
  explicando el defecto (no genera texto).
- **Cascade**: Jev filtra; si su **confianza < τ**, **escala al crítico LLM**.
  τ se **congela antes** de evaluar y se valida en held-out.
- **Aviso**: los errores de Jev y del LLM están **~96% correlacionados** (§4.2
  del informe), así que el cascade **solo gana precisión si los errores son
  complementarios**. Por eso Jev aquí ahorra **coste/latencia**, no sustituye al
  crítico.
- Herramienta de decisión: se pide por **jevalan** (`tool.risk`/capacidades), no
  se habla con Jev directamente (ver el spec del hub de decisiones).

## 6. Contrato y config (borrador)

```toml
[gauntlet]
enabled = false                 # off por defecto; caro
# activación
trigger = "explicit"            # explicit | classify | off
bar = "path/a/la/referencia"    # ruta/s donde vive la barra real
critic_tools = ["view", "glob", "grep", "bash"]  # solo lectura + medición
ab = true                       # A/B ciego con orden aleatorizado

[gauntlet.stop]
on_human = true
marginal_delta = 0.02           # mejora por debajo de esto -> parar
budget_steps = 200
budget_tokens = 2_000_000

[gauntlet.jev_filter]           # opcional, sobre jevalan
enabled = false
threshold = 0.5                 # τ congelado; < τ -> escala al crítico LLM
```

## 7. Gotchas

- **No paralelizar por reflejo**: por defecto, un dueño por preocupación
  acoplada, en secuencia. El paralelismo empeoró la calidad en la ejecución real.
- **El constructor no se puntúa a sí mismo**, ni el crítico ve su resumen.
- **Contexto fresco y estado fresco** por ronda: la contaminación entre
  iteraciones invalida la medición.
- **Elegir la métrica es del diseño**: una métrica cómoda (fps de cámara fija)
  puede ocultar el fallo (el juego injugable).
- **Presupuesto desde el principio**: sin ronda final arbitraria, pero con tope
  de steps/tokens/tiempo y parada por mejora marginal.
- **Jev nunca decide `allow` a ciegas** ni actúa de crítico; solo filtra y
  escala.
- **Coste real**: es el modo más caro de CodeLoom; por eso `enabled = false` y
  activación por caso.

## 8. Encaje y preguntas abiertas

- Es una extensión de §5.5 del spec principal, hermana del hub de decisiones:
  jevalan **decide si merece Gauntlet** y **filtra** dentro del loop.
- Se apoya en subagentes/jobs (§6), `StopWhen` (§4.3) y el header (§6.4). No
  necesita un modelo de dominios nuevo.
- Preguntas abiertas:
  - ¿El Gauntlet vive como **modo del agente principal** o como **tipo de job**
    (una pieza = un job)?
  - ¿Cómo se define la **barra** de forma reutilizable (un `bar.md` como en la
    integración encontrada, o una ruta a artefactos)?
  - ¿La parada por mejora marginal usa la métrica elegida por el humano o la del
    crítico?
  - ¿Se expone `/gauntlet` en la TUI o como capacidad de jevalan?
  - ¿Se miden y publican falsos-aprobados y desacuerdo humano (las métricas que
    el informe exige) como parte del propio CodeLoom?
