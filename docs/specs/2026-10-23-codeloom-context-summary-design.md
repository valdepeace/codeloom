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

- **`/compact` manual:** la TUI no tiene ninguna infraestructura de comandos con barra; es otra
  tarea de producto.
- **Reintento reactivo** al recibir del proveedor un rechazo por contexto excedido (lo hacen
  deepseek-harness, opencode y qwen-code): requiere reconocer el texto del error por proveedor,
  que no tenemos contrastado. Pendiente.
- **Restaurar ficheros** tras compactar (qwen-code restaura hasta 5): no se hace.
- **Presupuesto de tokens de gasto:** decisión de producto sin tomar; es otra cosa que la ventana.

## Riesgos que se verifican, no se suponen

- Un resumen de rol `User` seguido de una cola que empieza por mensaje de usuario produce dos
  mensajes `user` consecutivos. Se comprueba con las serializaciones de OpenAI y Anthropic.
- La estimación bytes/4 subestima el texto no latino; el ancla real lo corrige tras la primera
  respuesta.
