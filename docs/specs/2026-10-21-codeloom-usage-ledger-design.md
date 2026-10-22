# Libro de consumo por sesión (tokens y precio por modelo)

Estado: propuesta del 2026-10-21 (petición del usuario: «deberíamos de guardar el
precio por modelo para calcular el total de la sesión»). Plan:
`docs/plans/2026-10-21-codeloom-usage-ledger.md`.
Relacionado: `2026-10-21-codeloom-cost-estimate-design.md` (la estimación en vivo
que esto sustituye), `2026-10-21-codeloom-models-catalog-design.md`.

## Intención

La estimación actual multiplica **todos** los tokens de la sesión por el precio
del modelo *actual*, y los contadores viven solo en memoria. Falla en tres casos:
cambiar de modelo a mitad de sesión (el total se recalcula con el precio
equivocado), reabrir una sesión (todo vuelve a cero) y los subagentes de
sesiones anteriores (sus métricas no persisten). Se pretende un registro durable
de cada consumo con el modelo y el precio vigente en ese momento, de modo que el
total de una sesión sea sumable, correcto y recuperable.

## Objetivos

1. Persistir, por cada respuesta del proveedor, un apunte: sesión, modelo, tokens
   de entrada y salida, precio de entrada y salida por millón (si se conocía) y
   marca de tiempo.
2. Que el total de una sesión sume sus apuntes **y los de sus subagentes**.
3. Que al abrir una sesión el rail muestre el total guardado, y que en vivo siga
   sumando con el precio vigente en cada momento.
4. Mostrar en el rail el precio por millón del modelo actual (`$3/$15 /M`), que
   hoy no se ve en ningún sitio.

## No objetivos

- Facturación exacta (precio de caché, razonamiento, imágenes): sigue siendo
  orientativo.
- Informes, gráficos o exportación del histórico.
- Reescribir apuntes si el catálogo cambia de precios: el apunte conserva el
  precio de su momento (esa es la gracia).

## Anti-objetivos

- Red o trabajo pesado en el camino de teclado o de dibujado.
- Que un fallo al guardar un apunte rompa o retrase el turno: se registra el
  error y se sigue.
- Inventar precio: sin precio conocido, el apunte guarda tokens y precio `NULL`, y
  el total lo marca como parcial.

## Decisiones de diseño (mías, marcadas para revisión)

- **Núcleo.** Tabla `usage(id, session_id, model, input_tokens, output_tokens,
  input_price REAL NULL, output_price REAL NULL, created_at_ms)`. Tipo
  `UsageRecord`. En `SessionStore`: `record_usage(session, &UsageRecord)` y
  `session_usage(session) -> Vec<UsageRecord>` que incluye los apuntes de los
  hijos de esa sesión (unión con la tabla `subagents`).
- **Precios.** `Catalog::prices(base_url, model) -> Option<(f64, f64)>` en el
  núcleo, con la misma coincidencia de URL que usa hoy la TUI (sin barra final ni
  mayúsculas); la TUI pasa a usarla. El agente recibe un `Arc<RwLock<Catalog>>`
  compartido (`AgentLoop::with_catalog`) que la tarea de carga rellena cuando el
  catálogo llega; sin él, los apuntes llevan precio `NULL`.
- **Cuándo se apunta.** Al recibir el `Usage` del proveedor en cada paso del
  agente (el mismo punto donde hoy se publica `LiveEvent::Usage`), con el modelo
  y la URL de la configuración de ese instante. Los hijos usan su propio
  `AgentLoop`, así que apuntan solos bajo su sesión.
- **TUI.** Un nuevo `SessionUpdate::Usage(SessionId, Vec<UsageRecord>)` que el
  trabajador envía junto a `Loaded`; la TUI guarda `usage_records` y, en vivo,
  añade apuntes locales equivalentes (modelo y precio del momento) al llegar
  `Usage`/`ChildEvent::Usage`. Recargar la sesión sustituye la lista por la
  verdad de la base de datos. `estimated_cost()` pasa a sumar los apuntes.
- **Rail.** La línea de coste muestra `≈ $0.0123` y, si algún apunte carece de
  precio, `≈ $0.0123+`; bajo el modelo, `$3/$15 /M` cuando el precio se conoce.

## Qué se deja al implementador

Nombres internos no fijados en el plan, la migración de la tabla (`CREATE TABLE
IF NOT EXISTS`, como las demás) y el formato exacto de las líneas del rail.

## Verificación

Pruebas puras (SQLite en memoria o temporal, sin red) de: guardar y leer apuntes,
agregación con hijos, aislamiento entre sesiones, `Catalog::prices`, el agente
registrando un apunte por respuesta con modelo y precio vigentes (y precio `NULL`
sin catálogo), el cambio de modelo a mitad de sesión (dos apuntes con precios
distintos suman bien), y la TUI (carga de apuntes, suma en vivo, marca `+`).
Validación visual en terminal real: pendiente.

## Estado (2026-10-21)

Implementado, cada tarea en su commit: núcleo, con tabla `usage`,
`Catalog::prices` y apunte por respuesta (`972f412`); carga compartida del
catálogo y entrega de apuntes a la interfaz (`d5d10fe`); total y precio por
millón en el rail (commit de la tarea 3).

Cómo se ve: bajo los tokens del rail, `≈ $0.0123` (o `≈ $0.0123+` si algún
apunte carece de precio); bajo el modelo, `$3/$15 /M`. Al reabrir una sesión el
total reaparece, porque sale de la base de datos y no de contadores en memoria.

Decisiones tomadas al implementar:

- El agente apunta **una** fila por respuesta: acumula el uso del flujo y apunta al
  cerrarlo (Anthropic emite dos eventos de uso por respuesta).
- En vivo la interfaz añade un apunte local por cada evento de uso; el coste es
  lineal, de modo que el total coincide, y al recargar se sustituye por la verdad
  de la base de datos.
- Si ningún apunte tiene precio, la línea de coste se omite (un `≈ $0+` no
  informaría de nada).
- El modo `--print` no carga el catálogo: sus apuntes llevan precio `NULL`.
- Si falla la lectura de apuntes al abrir una sesión, falla la apertura, igual
  que con mensajes y subagentes.

Limitaciones conocidas:

- No hay precio de caché ni de razonamiento: es una cota orientativa.
- El `output_tokens` de Anthropic puede contarse con un exceso de unos pocos
  tokens (el evento inicial trae un valor mínimo que se suma al acumulado final).
- Los apuntes anteriores a esta versión no existen: las sesiones antiguas no
  muestran coste.
- `AgentLoop::with_jobs` llamado después de `new` deja al lanzador de hijos con
  el registro anterior (hoy nadie lo hace).
- Validación visual en terminal real: pendiente.

Coste por hijo: cada apunte lleva ya la sesión que lo originó (`UsageRecord::session`).
El del turno propio se atribuye a la sesión abierta; el de un subagente, a su hijo. Así
el rail muestra en la línea de métricas de cada subagente su coste estimado, entre los
tokens y el tiempo (`2/5 read_file · 1.2k/300 · ≈$0.004 · 7s`), con `+` si algún apunte
del hijo carece de precio. Si el hijo no tiene apuntes, o ninguno lleva precio, el grupo
se omite y no se muestra `≈$0`. Con esto queda superado el no-objetivo «coste por hijo»
de la spec de estimación (`2026-10-21-codeloom-cost-estimate-design.md`). Sin sesión
abierta, el apunte vivo usa `SessionId(0)` como marcador, que la recarga sustituye. La
tabla `usage` ya tenía la columna `session_id`, así que no hace falta migración.
