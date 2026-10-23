# Compactación anticipada en segundo plano

Estado: implementado el 2026-10-23, opt-in y sin verlo en una terminal real (véase «Estado» al
final). Parte de la idea 2 de
`docs/references/contexto-y-coste/memoria-observacional-mastra.md` y extiende
`2026-10-23-codeloom-context-summary-design.md`.

## Propósito

Hoy el resumen es **síncrono**: al llegar al 80 % de la ventana, `run_turn` espera a que el modelo
resuma y el turno se **detiene** (decenas de segundos con contexto grande). La fluidez es
prioridad máxima del proyecto. La memoria observacional de mastra lo evita calculando las
observaciones **en segundo plano** y **activándolas al instante** al llegar al umbral.

## Qué es de referencia y qué es propio

Según la documentación de mastra (no se leyó su implementación): el observador buferiza cada
cierto tramo de tokens, al llegar al umbral los fragmentos buferizados se activan sin pausa, y
hay una red de seguridad (`blockAfter`) si el búfer no da abasto. Aquí se adopta la idea, no su
mecanismo. Todo lo siguiente (umbrales, estados, el sondeo, la configuración) es **decisión
propia**.

## Diseño

### Idea

Cuando el contexto estimado alcanza `BUFFER_RATIO` (**0,60**) de la ventana, se lanza **una**
tarea en segundo plano que genera el resumen del tramo antiguo con el mismo corte
(`choose_cut`) y el mismo prompt que la compactación síncrona. El turno sigue sin esperar. Al
alcanzar `COMPACT_RATIO` (0,80, sin cambios), si el resumen ya está listo se **activa**: se
persiste como mensaje de resumen y entra en efecto de inmediato.

### Máquina de estados por sesión

`Idle` → `Running` → `Ready` → (activado o descartado) → `Idle`.

- **Arranque:** solo desde `Idle`, con la función de decisión `should_start_background` (contexto
  ≥ `BUFFER_RATIO`, ventana conocida, hay corte posible, la opción está activada).
- **Una sola tarea por sesión** a la vez; el estado es en memoria (se pierde al cerrar, a
  propósito: nada se persiste hasta activarlo).
- **Activación** al llegar a `COMPACT_RATIO`:
  - `Ready` y vigente → se activa: instantáneo, sin llamada al modelo.
  - `Running` → se **espera** a esa tarea (ya está en curso, y esperar es más barato que lanzar
    otra) con tope de `BACKGROUND_WAIT_SECS` (**60**). Si vence, se cancela la tarea y se cae al
    comportamiento síncrono de hoy.
  - `Idle` (no arrancó o falló) → compactación síncrona de hoy.
- **Vigencia:** el registro solo crece por el final, así que un resumen sigue valiendo mientras la
  posición del último resumen persistido sea la que había al lanzarlo. Si entre medias se
  persistió otro (por la vía síncrona), el pendiente se **descarta**.
- **Fallos:** un error o un resumen vacío/truncado deja la sesión en `Idle` y no se reintenta en
  el mismo turno (igual que la vía síncrona).

### Qué cubre un resumen en segundo plano

Cubre hasta el corte del momento del arranque, no hasta el del momento de la activación. Los
mensajes posteriores quedan en la cola. Con 200 000 de ventana, arranque a 120 000 y activación a
160 000, la cola resultante es de unos 10 000 + 40 000 tokens estimados, bastante por debajo del
80 %. En ventanas pequeñas la cola puede seguir siendo grande: se mide tras activar y, si aún
supera `COMPACT_RATIO`, se usa la vía síncrona en el siguiente paso (ya existe).

### Coste: declarado

- La tarea manda como entrada el tramo antiguo (hasta ~60 % de la ventana, ya pasado por la
  limpieza de resultados antiguos). **Si la sesión termina entre el 60 % y el 80 %, ese gasto se
  pierde.** Por eso la función es **opt-in** (`[compaction] background = true`, por defecto
  `false`) hasta medir en sesiones reales cuántas veces se desperdicia.
- Su consumo se apunta y cuenta para el presupuesto como cualquier llamada.

### Configuración

Sección opcional `[compaction]` de `codeloom.toml`, aparte de la principal (patrón de `[agents]`
y `[budget]`): `background = false` por defecto. Una sección mal escrita impide arrancar.

### Observabilidad

- `LiveEvent::BackgroundCompactionStarted { tokens_before }` y
  `LiveEvent::BackgroundCompactionReady { summary_bytes }`. `ContextCompacted` gana
  `activation: Activation` con `Sync`, `Background` o `BackgroundAwaited { waited_ms }`.
- **Estado visible**, al estilo de «Observing…» de mastra: en la línea de estado, «resumiendo…
  Ns» mientras corre.
- **Marca de listo** en el medidor: una flecha `↓` (la de mastra para «ahorro pendiente») cuando
  hay un resumen listo sin activar.
- **Arreglo del medidor:** el consumo de la llamada de resumen deja de publicarse como
  `LiveEvent::Usage` (movía la barra al tamaño de esa petición). Pasa a
  `LiveEvent::CompactionUsage { input_tokens, output_tokens }`, que suma a los contadores pero
  no al medidor.

### Concurrencia

La tarea usa `tokio::spawn` con copias (`Arc`) del proveedor, la configuración y el almacén; no
toca el hilo de la interfaz. El estado compartido es un `Mutex` por sesión con operaciones breves,
sin mantener el cierre durante `await`.

## Fuera de alcance, declarado

- **Activación por inactividad** (mastra `activateAfterIdle`; qwen-code): idea 5, otra spec.
- **Varios fragmentos buferizados** (mastra buferiza cada 20 %): aquí hay un solo resumen.
- **Persistir el resumen pendiente** entre ejecuciones.
- **Subagentes:** el mecanismo es por bucle y por sesión, así que un hijo lo heredaría con su
  propia configuración, pero no se prueba.
- **Medir en el mundo real** cuánto se ahorra: sin datos.

## Estado

Implementado el 2026-10-23 con la opción **apagada por defecto** (`[compaction] background =
false`). **Sin verlo aún en una terminal real**: solo hay pruebas con proveedor simulado y
`TestBackend`. Commits: `260d374` (separar generar de activar), `ac18bc4` (estado por sesión),
`32e16e5` (registrador de consumo clonable, `Activation`, configuración), `cce1c54` (integración
en el bucle), `c379366` (estado visible y flecha en el medidor) y `2a04751` (aviso de descarte).

### Decisiones tomadas durante la implementación

- **Las pruebas de concurrencia no usan `sleep`**: el proveedor simulado retiene la primera
  petición de resumen con un semáforo y avisa de su llegada por un canal. Dos de ellas (resumen en
  curso y plazo vencido) llaman a la función de activación directamente y no al turno, porque con
  el turno completo la espera es una carrera con el reloj pausado.
- **Cancelar el turno no mata el resumen**: la tarea lleva su propio token de cancelación. Solo lo
  cancela `abort` (plazo vencido).
- **Cada tarea lleva un identificador**: una entrega tardía tras un `abort` no pisa a la tarea
  posterior de la misma sesión.
- **Fallo hallado al integrar la interfaz**: si el resumen fallaba, venía vacío o se cancelaba, el
  estado volvía a reposo sin avisar y la interfaz se quedaba en «resumiendo…». Se añadió
  `LiveEvent::BackgroundCompactionDiscarded`, que también se publica al tirar uno caduco.

### Lo que no consta o queda pendiente

- **El contador «resumiendo… Ns» solo avanza cuando hay otro repintado**: no se añadió a la
  cadencia de animación para no repintar a 20 fps durante todo el resumen, de modo que puede verse
  congelado hasta el siguiente evento.
- **Coste real sin medir**: si la sesión acaba entre el 60 % y el 80 %, la llamada se pierde. Por
  eso sigue siendo opt-in. Si el plazo vence y se cae a la vía síncrona, se paga además un segundo
  resumen.
- **Un salto de menos del 60 % a más del 80 % en un solo paso** lanza el resumen y lo espera en el
  mismo paso: no ahorra nada respecto a la vía síncrona.
- **Subagentes**: no heredan la opción y usan siempre la vía síncrona.
- **El resumen listo no se persiste**: se pierde al cerrar la aplicación, a propósito.
- **Etiqueta en el demo**: los últimos pasos antes de la caída llevan la flecha pero conservan la
  etiqueta «compactando…».
