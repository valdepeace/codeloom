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

- **Subagentes:** cada hijo tiene su propia sesión y **no** hereda el presupuesto; el gasto de los
  hijos no suma al del padre. Es la fuga más probable y queda pendiente de decisión.
- **Presupuesto por turno** (qwen-code lo ofrece por directiva) y **por día**: no.
- **Aviso al modelo** (codex): descartado por la decisión de producto.
- **Recarga en caliente** de `[budget]`: se lee al arrancar.
