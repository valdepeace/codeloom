# Memoria observacional de mastra (OM)

Estado: referencia de solo lectura, 2026-10-23. A petición de la persona usuaria, que la considera
**interesante y no debe olvidarse**: es la alternativa más elaborada al resumen único que hacen los
demás agentes del registro (`comparativa-herramientas.md`, «Compactación del historial»).

## Qué se leyó y qué no

Leído: la documentación (`docs/src/content/en/docs/memory/observational-memory.mdx` del clon de
mastra), el componente de la TUI de mastracode que pinta el progreso
(`mastracode/tui/src/tui/components/om-progress.ts`) y el indicador de contexto de su línea de
estado (`status-line.ts`, `commands/context.ts`). **No se leyó** la implementación (unas 6 000
líneas en `packages/memory/src/processors/observational-memory/`): ni los *prompts* del
observador y del reflector, ni cómo se decide qué se anota. Todo lo que sigue sobre el
funcionamiento es lo que **afirma su documentación**, no lo que se ha comprobado.

## Qué es: tres niveles

1. **Mensajes recientes:** el historial literal de la tarea en curso.
2. **Observaciones:** un registro de notas concisas sobre lo ocurrido. Un *observador* (llamada
   al modelo) las crea cuando los mensajes superan un umbral (por defecto 30 000 tokens) y los
   mensajes observados **salen** de la ventana.
3. **Reflexiones:** cuando las observaciones superan su propio umbral (por defecto 40 000), un
   *reflector* **reescribe todo el registro**, condensando más lo antiguo y conservando el detalle
   reciente. No es una capa que crece aparte: la memoria queda acotada alrededor del umbral por
   larga que sea la conversación.

Con los valores por defecto, la ventana **oscila** (según su doc): el historial crece de unos 6k a
30k, se observa y se reduce, y se repite; el registro de observaciones crece mucho más despacio.
La compresión típica que afirman es de 5 a 40 veces. **Cifra suya, no medida aquí.**

## Lo que hace distinto a esta aproximación

- **Búfer asíncrono (activado por defecto):** el observador trabaja **en segundo plano** cada
  cierto número de tokens (`bufferTokens`, un 20 % del umbral) y deja las observaciones listas.
  Al llegar al umbral se **activan al instante** y el agente **no se detiene**. Sin búfer, el
  observador es síncrono y el agente se para a esperarlo. Hay una red de seguridad (`blockAfter`,
  1,2 veces el umbral) si el búfer no da abasto.
- **Activación por inactividad** (`activateAfterIdle`): forzar la activación tras un tiempo sin
  actividad, o con «auto» según la vida de la caché de *prompt* del proveedor.
- **Modo de recuperación (*retrieval*):** cada grupo de observaciones queda **enlazado a los
  mensajes en bruto** que lo produjeron, y el agente dispone de una herramienta `recall` para
  recuperar el texto exacto, la salida de herramientas y la cronología que el resumen perdió.
- **Extractores:** valores que se guardan junto a las observaciones: tarea actual, respuesta
  sugerida, título del hilo, y los que defina la persona integradora.
- **Alcance:** por hilo (por defecto) o por recurso (experimental, compartido entre hilos).
- **Modelo por tamaño de entrada** (`ModelByInputTokens`) y presupuesto compartido
  (`shareTokenBudget`) entre mensajes y observaciones.

## Cómo lo representa (lo que le gusta a la persona usuaria)

Indicador de contexto en la línea de estado (`formatOMContextIndicator`):

- **Diez celdas `━` de tres clases:** memoria (atenuada), mensajes (texto normal) y libres
  (apagadas). La capacidad es **umbral de mensajes + umbral de reflexión**; lo usado es
  **mensajes pendientes + tokens de observaciones**. Las celdas de memoria y de mensajes se
  reparten en proporción a lo que pesa cada una, y si hay ambas se garantiza al menos una celda
  de cada clase.
- **Fracción** `usado/capacidad` en miles (`12.4/70k`) y una flecha `↓` cuando hay ahorro
  pendiente (observaciones en búfer que, al activarse, retirarán mensajes).
- **Estado de actividad:** `Observing… Ns` y `Reflecting… Ns` con *spinner* y color distinto cada
  uno; en reposo, `OM ━━━──── 40%` con color según cercanía al umbral (a partir del 70 % aviso,
  del 90 % rojo; **umbrales de mastra**, no nuestros).
- El pie degrada con el ancho: primero deja de pintar la barra, luego el directorio, luego acorta
  el nombre del modelo.
- En su estudio web, una pestaña de memoria muestra las barras de progreso, el **registro de
  observaciones activo en línea**, las observaciones anteriores navegables y los fragmentos
  en búfer mientras se procesan.
- `/context` desglosa de dónde sale el consumo (arranque, *skills*, herramientas, conversación
  acumulada, observaciones). Su propio aviso: los porcentajes son del total auditado, **no** de la
  ventana del modelo, y las cuentas son estimaciones.

**Aviso de honestidad:** su barra mide el relleno de **su memoria** frente a **sus umbrales**, no
la ventana del modelo. Copiar el aspecto sin cambiar lo que mide es válido, pero hay que rotularlo.

## Qué podría tomar CodeLoom: ideas, no decisiones

Nada de esto está decidido. Se anota con su encaje en lo que ya existe
(`2026-10-23-codeloom-context-summary-design.md`):

1. **Barra de dos segmentos (resumen y mensajes).** Encaja casi gratis: tras compactar, la
   petición es «resumen + cola», y el tamaño de cada parte se conoce. El medidor
   (`2026-10-23-codeloom-context-meter-design.md`) puede mostrar el resumen atenuado y la cola
   normal. **Es lo que más se parece a lo que le gusta a la persona usuaria y es barato.**
2. **Compactación anticipada en segundo plano.** Hoy el resumen es **síncrono**: el turno se
   detiene mientras el modelo resume. El búfer asíncrono de mastra lo evita. Es la mejora de más
   valor de cara a la fluidez, pero exige concurrencia con el bucle y un estado de «resumen
   preparado»: una spec propia.
3. **Herramienta `recall` sobre el registro íntegro.** CodeLoom ya conserva todos los mensajes
   (el registro no se toca al compactar): leerlos por rango o buscar en ellos sin pasar por el
   resumen es factible y cuadra con «la conversación es el diario».
4. **Reescritura del registro de memoria** (reflexión) para que el resumen no crezca con
   compactaciones encadenadas. Hoy los resúmenes se encadenan (el nuevo resume el anterior más lo
   nuevo) y el tope de 16 000 bytes lo acota, pero no hay una política explícita.
5. **Activación por inactividad** (precedente también en qwen-code): compactar cuando la persona
   usuaria se aleja, no en mitad de un turno.
6. **Estado visible de la compactación** («Resumiendo… Ns») en la línea de estado: hoy solo se ve
   el aviso final.
7. **Registro de resúmenes consultable en la interfaz**, como la pestaña de memoria de su estudio.
