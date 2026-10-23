# Resumen del historial con el modelo (compactación completa)

Estado: diseño, 2026-10-23. Complementa a `2026-10-22-codeloom-context-compaction-design.md`
(limpieza de resultados antiguos, ya hecha) y cierra la regla 5 del decálogo
(`2026-10-22-codeloom-decalogo-de-contexto-y-coste.md`).

## Propósito

Cada paso del agente reenvía todo el historial. La limpieza de resultados antiguos retrasa el
problema, pero no lo elimina: una sesión larga acaba rozando la ventana del modelo. Los demás
agentes del registro (`docs/references/contexto-y-coste/comparativa-herramientas.md`: codex, crush,
opencode, qwen-code, deepseek-harness) resuelven esto igual: al aproximarse a la ventana, una
llamada extra al modelo resume lo antiguo y el resumen sustituye a los mensajes en la petición.
Se hace lo mismo, con una diferencia deliberada: **el registro de la sesión no se toca** (la
conversación es el diario), solo cambia lo que se envía.

## Qué es de estándar y qué es decisión propia

No consta ningún estándar para esto (ni A2A ni MCP lo definen). Es una práctica común de los
agentes citados; las cifras y el formato son **decisiones propias**, rotuladas como tales.

## Diseño

### Cuándo se compacta

Antes de construir cada petición del bucle, si el tamaño estimado del contexto alcanza el
**80 %** de la ventana del modelo (`COMPACT_RATIO = 0.80`; los precedentes van del 80 % de
deepseek-harness al 90 % de codex, y se elige el extremo prudente porque la propia petición de
resumen necesita holgura).

- **Ventana:** la publica el catálogo de models.dev (`CatalogModel::context`), que `AgentLoop`
  ya comparte. Se añade `Catalog::context_window(base_url, model) -> Option<u64>`. **Si no se
  conoce la ventana, no se compacta de forma automática** (sin dato no se afirma nada) y la
  limpieza de resultados antiguos sigue protegiendo.
- **Tamaño:** se ancla en el recuento real del proveedor (`Usage::input_tokens` de la última
  respuesta, más su salida) y se estima solo lo posterior con bytes/4; sin ancla válida, se
  estima todo (prompt de sistema incluido). Es el método de opencode y qwen-code.
- **Ancla válida:** solo si la respuesta es posterior al último resumen; una respuesta anterior
  midió un contexto que ya no existe.

### Qué se resume y qué se conserva

- Se conserva la **cola reciente**: los mensajes más recientes hasta unos `KEEP_TAIL_TOKENS =
  10_000` estimados (opencode conserva 15 000; codex, 20 000 de mensajes de usuario).
- El corte **nunca separa una llamada a herramienta de su resultado**: la cola no puede empezar
  por un mensaje de rol `Tool`; el corte retrocede hasta el mensaje del asistente que la emitió.
- Se resume todo lo anterior a la cola. Si no hay nada que resumir (el corte cae al principio),
  no se hace nada.

### Cómo se resume

Una petición al mismo proveedor y modelo, **sin herramientas**, con un prompt de sistema fijo y
la parte antigua (ya pasada por la limpieza de resultados) más una última instrucción de
resumir. El prompt pide secciones fijas (adaptación propia de la plantilla de ocho secciones de
deepseek-harness): objetivo, instrucciones y restricciones de la persona usuaria, lo hecho,
estado actual y ficheros tocados, decisiones y hallazgos, problemas abiertos y siguientes pasos.

El resumen **tiene cota** (regla 1): `max_tokens = 4096` y, además, `MAX_SUMMARY_BYTES = 16_000`
con `limit_bytes`.

### Dónde vive el resumen

Como un mensaje más del registro (rol `User`), **añadido al final** y con una cabecera fija:

```
[Context summary: first N messages]
<texto del resumen>
```

`N` es el número de mensajes del registro que cubre (posición, no identificador: el almacén no
conserva `Message::id` al leer). Es el mismo recurso que ya usan los avisos `[Job …]` y
`[Subagent … completed]`: un mensaje de usuario con cabecera reconocible, sin variante nueva de
`ContentPart` (que obligaría a tocar todos los `match` exhaustivos de las tres crates).

La petición al proveedor se arma así: el **último** resumen, seguido de los mensajes de
posición ≥ N, sin ningún otro resumen. Resúmenes sucesivos se encadenan: el nuevo resume el
anterior más lo que vino después.

### Límites del propio mecanismo (decálogo, regla 7)

- Como mucho `MAX_COMPACTIONS_PER_TURN = 3` compactaciones por turno.
- Un fallo (error del proveedor, resumen vacío, cancelación) **no aborta el turno**: se registra
  y el turno sigue sin compactar; no se reintenta dentro del mismo turno.

### Observabilidad (valor central)

- `LiveEvent::ContextCompacted { replaced: usize, tokens_before: u64, summary_bytes: usize }`.
- La TUI muestra el mensaje de resumen como **aviso propio** («contexto compactado»), no como
  texto de la persona usuaria, y rotula que es una función de CodeLoom, no de A2A.
- Los subagentes heredan el mecanismo: usan el mismo bucle, con la ventana de su modelo.

## Qué queda fuera (declarado)

- **`/compact` manual:** **hecho el 2026-10-23**. La nota original decía que la TUI no tenía
  infraestructura de comandos con barra; era falso (`commands.rs` ya tenía diecisiete, entre ellos
  `/agents`). Véase la sección «`/compact` manual» más abajo.
- **Reintento reactivo** al recibir del proveedor un rechazo por contexto excedido (lo hacen
  deepseek-harness, opencode y qwen-code): **hecho el 2026-10-23** (`is_context_overflow` en
  `compact.rs`, rama en `run_turn`). Se reconoce un `400`/`413` cuyo cuerpo contiene alguna de
  siete fórmulas («maximum context length», «prompt is too long»…), que son **las que se recuerdan
  de los proveedores, sin contrastar con su documentación**: un proveedor con otra redacción no se
  reconoce y su error se propaga como antes. Comparte el tope `MAX_COMPACTIONS_PER_TURN`.
- **Restaurar ficheros** tras compactar (qwen-code restaura hasta 5): no se hace.
- **Presupuesto de tokens de gasto:** decisión de producto sin tomar; es otra cosa que la ventana.

## Riesgos que se verifican, no se suponen

- Un resumen de rol `User` seguido de una cola que empieza por mensaje de usuario produce dos
  mensajes `user` consecutivos. Se comprueba con las serializaciones de OpenAI y Anthropic.
- La estimación bytes/4 subestima el texto no latino; el ancla real lo corrige tras la primera
  respuesta.

## Estado

Implementado el 2026-10-23, **sin verlo aún en una terminal real ni contra un proveedor real**.
Commits: `99d9c34` (ventana de contexto en el catálogo), `005bd7e` (funciones puras),
`854f91c` (llamada de resumen y evento), `356b1a8` (integración en el bucle), `c7dd41b` (aviso
propio en la TUI) y `cd04d13` (apunte del consumo de la llamada de resumen).

### Desviaciones respecto al diseño original, todas deliberadas

- **El corte nunca cae antes de un resumen previo.** El plan lo permitía y se perdía ese resumen;
  lo detectó la implementación y se fijó con un test. Si el presupuesto de cola se agota antes del
  resumen, la cola queda más corta que `KEEP_TAIL_TOKENS`.
- **Un resumen truncado se descarta.** Un flujo que acaba en `Done(Error)` o sin `Done` no se
  persiste: guardarlo envenenaría el contexto de toda la sesión.
- **El tope de `MAX_SUMMARY_BYTES` es exacto.** El hueco del marcador de omisión de `limit_bytes`
  (64 bytes) sale del propio tope.
- **«Nada que resumir» no es un fallo.** Se comprueba que hay corte posible antes de llamar, de
  modo que no se apaga la compactación el resto del turno.
- **El consumo de la llamada de resumen se apunta** en el almacén y se publica a los contadores,
  se acepte el resumen o no. Sin eso, el coste de compactar quedaba invisible.
- **El resumen no entra en el historial del composer** (flecha arriba), y se reconoce desde la
  cadena sin construir un `Message` por línea.

### Lo que no consta o queda pendiente

- **No se ha probado contra un proveedor real.** La calidad del resumen (qué conserva y qué pierde)
  es una hipótesis hasta verla en sesiones largas.
- **Dos mensajes `user` seguidos** (resumen y cola que empieza por usuario): ya ocurrían con los
  avisos `[Job …]` tras un resultado de herramienta, y la API de Anthropic admite turnos
  consecutivos del mismo rol, pero **no se ha contrastado** con la API real.
- **Caché de prompt de Anthropic:** si algún día se activa, `input_tokens` puede no contar los
  tokens cacheados y la estimación se quedaría corta; habría que sumar los campos de caché.
- **Sin ventana conocida no se compacta** (modelo ausente del catálogo, catálogo sin descargar o
  sin red). La limpieza de resultados antiguos sigue activa.
- El **reintento reactivo** ante un rechazo por contexto excedido está hecho (véase arriba), con
  fórmulas de error sin contrastar.

### `/compact` manual (2026-10-23)

`/compact` resume ya el historial antiguo de la sesión abierta, sin esperar al 80 %.

- **Núcleo:** `AgentLoop::compact_now(session, cancel) -> ManualCompaction`. Usa la misma vía que
  la compactación automática (`compact_or_activate`: activa un resumen en segundo plano si lo hay
  y, si no, llama al modelo). Publica siempre `LiveEvent::ManualCompactionEnded` con uno de tres
  resultados: `Compacted`, `NothingToSummarize` (no hay corte posible: no se llama al modelo) o
  `Failed` (resumen vacío, cortado o error: el historial queda como estaba). El prompt de sistema se
  extrajo a `system_prompt()` para estimar `tokens_before` igual que en un turno.
- **Interfaz:** `Command::Compact`. Solo con sesión abierta y sin turno, despertar ni otra
  compactación en curso; si no, el motivo sale en el aviso del composer y se conserva el texto
  escrito. Mientras dura, la cola de prompts y los despertares esperan (`compact_in_flight`).
- **Trabajador (`codeloom-cli`):** canal propio (`compact_tx`), junto al de despertares.
- **Rotulado como propio:** nada de esto viene de A2A; es mando de la interfaz.
- **No consta / límites:** no se puede cancelar con `Esc` mientras resume (la cancelación de la
  interfaz solo actúa sobre turnos); no se ha visto en una terminal real; con la compactación
  manual no hay topes propios más allá de los de la automática (el resumen se acota a
  `MAX_SUMMARY_BYTES`), y el mando puede repetirse cuantas veces se quiera. No informa de cuántos
  mensajes cubrió: eso ya lo pinta `ContextCompacted` en el transcript.
- **Presupuesto de tokens de gasto:** decisión de producto sin tomar.
- **Subagentes:** heredan el mecanismo porque usan el mismo bucle y el catálogo compartido, pero
  no hay un test específico de un hijo que compacte.
- **Look clásico:** el cuerpo del resumen no se pliega (igual que el aviso de subagente).
