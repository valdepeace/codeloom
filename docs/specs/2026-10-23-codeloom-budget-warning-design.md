# Aviso de presupuesto de gasto

Estado: diseño, 2026-10-23. Cierra a medias la regla 7 del decálogo
(`2026-10-22-codeloom-decalogo-de-contexto-y-coste.md`): el coste también tiene techo.

## Decisión de producto (de la persona usuaria, 2026-10-23)

**Solo avisar.** Al acercarse o superar el presupuesto, la persona usuaria ve un aviso. El turno
**no** se detiene, **no** se pregunta nada y **no** se avisa al modelo: su comportamiento no cambia.

## Qué es de estándar y qué es decisión propia

No consta ningún estándar. Precedentes consultados en `docs/references/contexto-y-coste/`:
qwen-code termina el turno al superar `sessionTokenLimit`; mastra ofrece `block` o `warn` con aviso
suave a un porcentaje; codex avisa **al modelo** cuando quedan 6 144 tokens. Aquí se toma solo el
modo `warn` de mastra, dirigido a la persona usuaria. Las cifras y los nombres de claves son
**decisiones propias**.

## Diseño

### Configuración

Sección opcional `[budget]` de `codeloom.toml`, aparte de la configuración principal, igual que
`[agents]`. Sin ella, o con ambas claves ausentes, **no hay presupuesto ni aviso** (por defecto
ilimitado, como qwen-code).

```toml
[budget]
session_tokens = 500000     # tokens de entrada + salida acumulados en la sesión
session_cost_usd = 2.50     # dólares acumulados en la sesión
```

Pueden darse una, ambas o ninguna. Cada una avisa por su cuenta.

### Cuándo se avisa

Al apuntar el consumo de cada respuesta (`AgentLoop::record_usage`) se compara el acumulado de la
sesión **antes** y **después** de ese apunte contra dos umbrales fijos, el **80 %** y el
**100 %** del presupuesto. Se avisa cuando el acumulado **cruza** un umbral: `antes < umbral ≤
después`. Es una función pura y **sin estado**: cada umbral avisa una sola vez por sesión sin
recordar nada en memoria, y reabrir la sesión no repite un aviso ya dado salvo que se vuelva a
cruzar. Una respuesta que salta de golpe del 70 % al 130 % emite los dos avisos.

El apunte de la llamada de resumen de la compactación cuenta como cualquier otro.

### Coste en dólares

Solo suman los apuntes con precio conocido (`UsageRecord::cost`). Si algún apunte no lo tiene,
el aviso lo dice («según los precios conocidos») en lugar de afirmar un total que no consta.

### Observabilidad

- `LiveEvent::BudgetWarning { metric, level_percent, used, limit }`.
- La TUI lo muestra como línea de sistema rotulada como aviso **de CodeLoom** («aviso de
  presupuesto»), nunca como estado de A2A ni como texto de la persona usuaria.

## Fuera de alcance, declarado

- **Subagentes:** inicialmente fuera de alcance; resuelto el 2026-10-23 por decisión de la persona
  usuaria (véase «Estado»): el presupuesto es de la **familia** (sesión raíz y sus hijos).
- **Presupuesto por turno** (qwen-code lo ofrece por directiva) y **por día**: no.
- **Aviso al modelo** (codex): descartado por la decisión de producto.
- **Recarga en caliente** de `[budget]`: se lee al arrancar.

## Estado

Implementado el 2026-10-23, **sin verlo aún en una terminal real**. Commits: `2a8b09c`
(configuración y cruce de umbrales), `36b7984` (bucle y arranque) y `96387df` (línea en la TUI).

### Desviaciones respecto al diseño original, deliberadas

- **Solo cuentan los apuntes de la propia sesión.** El almacén (`session_usage`) devuelve también
  el consumo de los subagentes. De contarlo, un hijo podía rebasar un umbral entre dos respuestas
  del padre y el aviso no saltaba jamás, porque el acumulado «anterior» ya lo superaba. Lo
  detectó la revisión de la implementación y lo fija un test.
- **Un `[budget]` mal escrito impide arrancar**, igual que `[agents]`. Un aviso por la salida de
  errores se perdería tras la pantalla alternativa de la interfaz.

### Los subagentes suman al total (2026-10-23)

La persona usuaria eligió el **total compartido**. Sustituye a la desviación anterior («solo
cuentan los apuntes de la propia sesión»), que ya no rige.

- `budget_crossings` cuenta **todos** los apuntes que recibe, sean de la sesión que sean. Quien la
  llama pasa la familia entera: el agente raíz lee `session_usage(su sesión)` y un hijo lee
  `session_usage(sesión del padre)` (`with_budget_family`), que devuelve la raíz y sus hijos.
- El lanzador de subagentes recibe el presupuesto (`InProcessSubagentLauncher::with_budget`) y lo
  pasa a cada hijo. Con ello el problema que motivó la desviación anterior (un hijo rebasa un
  umbral y el padre ya no avisa) desaparece: el aviso lo da quien cruza el umbral, una vez por
  nivel para toda la familia.
- El aviso que cruza un hijo llega al bus del padre como `ChildEvent` y la interfaz lo pinta
  como la misma línea de sistema, con el texto «de la sesión y sus subagentes». Hasta este
  cambio la interfaz descartaba en silencio cualquier `BudgetWarning` de un hijo.
- Los contadores de consumo de la interfaz y el presupuesto miden ya lo mismo (la familia).

### Lo que no consta o queda pendiente

- **No atómico:** leer el acumulado y apuntar el consumo son dos pasos. Dos respuestas
  simultáneas (padre e hijo, o dos hijos) que cruzan el mismo nivel pueden publicar el aviso dos
  veces. No se ha medido con qué frecuencia ocurre.
- **Un solo nivel de familia:** los hijos no reciben la herramienta `subagent`, así que no hay
  nietos; si algún día los hubiera, un nieto contaría su familia desde el padre, no desde la raíz.
- **Sin tope duro** (por decisión): el hijo sigue sin detenerse al llegar al 100 %.
- Sin verlo en una terminal real; probado con un lanzador real y un proveedor simulado.
- Los tokens se muestran con `format_tokens` (400 123 sale como «400k»): la precisión del aviso es
  la de ese formato.
- Con dólares, el límite lleva «USD» y lo usado solo la cifra («8.00 usados»).
- Presupuesto por turno, por día y recarga en caliente: sin hacer.
