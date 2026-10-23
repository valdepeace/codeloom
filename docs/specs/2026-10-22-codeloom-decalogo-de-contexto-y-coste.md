# Decálogo de contexto y coste

Estado: vigente desde el 2026-10-22. Nace de un incidente real y se amplía con lo que se vaya
aprendiendo.

## Qué es y qué no es

**No es un estándar.** Es una síntesis propia, escrita a partir de un incidente y contrastada el
2026-10-22 con tres fuentes primarias. Lo que respalda cada una, y lo que no:

- **OWASP Top 10 for LLM Applications, LLM10:2025 «Unbounded Consumption»**
  (`genai.owasp.org/llmrisk/llm102025-unbounded-consumption/`). Respalda: validar que las
  entradas no superen «reasonable size limits», límites de tasa y cuotas por usuario, timeouts y
  *throttling*, y limitar las acciones en cola. **No** respalda límites de tokens ni de salida,
  ni límite de pasos o recursión de un agente, ni topes de gasto como mitigación (menciona
  «Denial of Wallet» solo como ejemplo de vulnerabilidad), ni el tamaño de la salida de
  herramientas. Corrección: una versión previa de este documento, basada en resúmenes de
  terceros, daba la numeración LLM06:2026; la página oficial dice LLM10:2025.
- **Especificación de MCP (versión 2026-07-28).** **No consta** ningún límite, truncado ni
  paginación del resultado de `tools/call` (la paginación solo cubre los listados). Fija que el
  servidor debe limitar la tasa y sanear las salidas, y que el cliente debería validar los
  resultados y poner timeouts, sin concretar tamaño. Solo una guía **no normativa** de buenas
  prácticas para clientes pide «validate and truncate» la salida de un sandbox antes de devolverla
  al modelo. Es decir: acotar el resultado es responsabilidad del cliente, no del protocolo.
- **Anthropic, «Writing effective tools for AI agents» (2025-09-11) y «Effective context
  engineering for AI agents» (2025-09-29).** Es lo más cercano a una referencia de ingeniería:
  - Único tope numérico: «For Claude Code, we restrict tool responses to 25,000 tokens by
    default» (comportamiento de Claude Code en esa fecha, no recomendación universal).
  - Para cualquier respuesta voluminosa, «some combination of pagination, range selection,
    filtering, and/or truncation with sensible default parameter values»; al truncar, orientar al
    agente (búsquedas pequeñas y dirigidas) y devolver errores accionables.
  - Preferir herramientas dirigidas (`search_logs`) frente a volcados (`read_logs`).
  - Compactar el historial resumiéndolo, y la forma más ligera es limpiar resultados antiguos de
    herramientas; un subagente devuelve un resumen condensado, «often 1,000-2,000 tokens».
  - **No dan** umbral de compactación, tamaño de página por defecto, ni presupuesto por
    herramienta distinto de esos 25 000 tokens.

Las URL consultadas, con fecha y lo que respalda cada una, están en
`docs/references/contexto-y-coste/fuentes.md`.

Consecuencia: las reglas 1, 3, 4, 5 y 8 tienen respaldo de Anthropic; la 7 lo tiene solo en parte
(OWASP cubre timeouts y cuotas, **no** el límite de pasos ni el presupuesto de tokens, que son
decisión propia); la 2, la 6, la 9 y la 10 son propias. Las cifras de Codeloom (20 000 bytes
≈ 5 000 tokens, 300 coincidencias, 200 elementos) son **decisiones propias**, deliberadamente
más prudentes que los 25 000 tokens de Claude Code.

## Por qué existe

Un `glob **/*` devolvió casi seis megabytes de rutas (recorría `target/`). Se guardaron en la
sesión, se reenviaron al proveedor y este respondió con un 400; de paso, se pagó una cantidad
desmedida de tokens. Nada en el código lo impedía: ninguna herramienta acotaba su salida.
Cabe señalar que el fallo no fue de un proveedor ni de un modelo, sino nuestro, y que cualquier
herramienta nueva puede reproducirlo. Este decálogo es la lista que se repasa **antes** de dar
una herramienta, un prompt o un bucle por terminado.

La regla de fondo: **todo lo que entra en el contexto del modelo cuesta dinero y espacio, y lo
que no tiene cota acabará, tarde o temprano, desbordándose.**

## Las diez reglas

1. **Toda salida que llega al modelo tiene cota.** Ninguna herramienta devuelve «lo que haya».
   Cota general: `MAX_TOOL_OUTPUT_BYTES` (20 000 bytes), con aviso expreso del tamaño original.
   *Estado: hecho (`d9959e5`).*
2. **La cota se aplica al crear y al serializar.** El historial se persiste y se reenvía: lo
   guardado antes de existir la cota también debe recortarse al salir hacia el proveedor.
   *Estado: hecho, en los dos proveedores.*
3. **Los listados respetan `.gitignore` y tienen su propio máximo.** Nunca entran en `.git`,
   `target/` ni `node_modules/`. El glob devuelve a lo sumo 300 coincidencias y dice cuántas
   había. *Estado: hecho para `glob`. `grep` tiene ahora máximo de 200 coincidencias y filtro `glob`.*
4. **Recortar avisa y orienta.** Un recorte silencioso engaña al modelo. El aviso dice cuánto se
   mostró, cuánto había y qué hacer («estrecha el patrón», «usa `offset`»).
5. **El historial no crece sin límite.** Cada paso reenvía toda la conversación. Hace falta
   compactación o resumen de lo antiguo. *Estado: **hecho en lo esencial**. Limpieza de
   resultados antiguos de herramientas (`2026-10-22-codeloom-context-compaction-design.md`) y,
   desde el 2026-10-23, resumen del historial con el modelo al alcanzar el 80 % de la ventana
   (`2026-10-23-codeloom-context-summary-design.md`). Falta `/compact` manual. Ante un
   rechazo `400`/`413` por contexto excedido se compacta y se reintenta (fórmulas de error sin
   contrastar con los proveedores); sin ventana conocida no se compacta por umbral. Desde el
   mismo día existe, **opt-in** (`[compaction] background = true`), la preparación del resumen en
   segundo plano al 60 % para activarlo al 80 % sin detener el turno
   (`2026-10-23-codeloom-background-compaction-design.md`); su coste real está sin medir. Véase
   también `2026-10-14-loom-kpis-prompt-compression.md`.*
6. **El prompt de sistema también tiene cota.** Ficheros de contexto (`AGENTS.md`,
   `CODELOOM.md`), lista de agentes, habilidades y memoria se inyectan en cada petición.
   *Estado: **parcial** (2026-10-23). Cada fichero de contexto se lee con tope de 16 000 bytes
   (`MAX_CONTEXT_FILE_BYTES`; se conserva el principio y se avisa del corte), la sección de agentes
   se limita a 8 000 y el prompt propio de un agente a 16 000 (recorte por cabeza y cola, con
   marcador). Queda **sin tope** el total del prompt de sistema (tres secciones sumadas) y no hay
   aún habilidades ni memoria que acotar porque no existen; el tope de cada sección es una
   decisión propia, sin medir contra ningún proveedor.*
7. **Todo bucle tiene techo, y el coste también.** Existen `max_steps` y `max_turns`. Desde el
   2026-10-23 hay **aviso** de presupuesto por sesión (tokens y/o dólares, sección `[budget]`),
   al 80 % y al 100 %, por decisión de producto **solo avisa**: no detiene el turno, no
   pregunta y no informa al modelo (`2026-10-23-codeloom-budget-warning-design.md`). El sondeo
   (`job_output` en bucle) es un bucle disfrazado: se evita en el prompt, pero no se garantiza.
   *Estado: **parcial**: sin tope duro por decisión. Desde el 2026-10-23 el gasto de los
   subagentes **suma al mismo total** que el del padre (la familia: sesión raíz y sus hijos): el
   hijo hereda el presupuesto y el aviso lo cruza quien gaste, una sola vez por nivel. Dos
   gastos simultáneos pueden duplicar un aviso, porque leer y apuntar no es atómico; con
   subagentes sin anidar la familia es de un nivel.*
8. **Lo que cruza de un agente a otro también se acota.** El resultado de un subagente vuelve al
   padre y lo paga cada paso siguiente. *Estado: hecho: el aviso de finalización del hijo se recorta a 8 000 bytes.*
9. **La memoria de la interfaz tiene cota.** Registros por hijo (200 elementos), cachés de
   líneas, transcript. *Estado: cumplido en el registro de hijos; el resto, por auditar.*
10. **Se prueba el peor caso, no el caso feliz.** Toda herramienta nueva trae un test con una
    entrada desmesurada (miles de ficheros, una línea de un megabyte, salida binaria) y se
    prueba en un repositorio real con `target/`, no solo en un directorio temporal vacío.

## Cómo se aplica

- Al diseñar una herramienta: ¿cuál es su salida máxima en bytes? Si no hay respuesta, no está
  terminada.
- Al revisar: se buscan `read_to_string`, `collect`, `join` y recorridos de directorio sin
  límite; y todo `format!` que meta contenido externo en un mensaje.
- Ante un 400 del proveedor: se mira primero el **tamaño** de lo enviado (por ejemplo, el campo
  `payload` de la tabla `messages` de `~/.codeloom/sessions.db`), antes de culpar al proveedor
  o al modelo.
- Los avisos de recorte y los topes se rotulan como lo que son: decisiones propias, no de A2A.

## Preguntas abiertas

- ¿Qué tope tiene sentido por herramienta? 20 000 bytes (≈ 5 000 tokens) es una cota general
  prudente, muy por debajo de los 25 000 tokens de Claude Code, y no una medida afinada. Conviene
  medir con sesiones reales.
- Presupuesto de tokens: resuelto como aviso por sesión y familia (regla 7). Queda abierto el
  presupuesto por turno.
- ¿Compactación a petición (`/compact`)? La automática ya existe; falta el mando manual, que
  exige una infraestructura de comandos con barra que la TUI no tiene.
