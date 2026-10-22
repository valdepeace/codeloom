# Decálogo de contexto y coste

Estado: vigente desde el 2026-10-22. Nace de un incidente real y se amplía con lo que se vaya
aprendiendo.

## Qué es y qué no es

**No es un estándar.** Es una síntesis propia, escrita a partir de un incidente y de lo que se ve
en el código. Lo único externo que se ha localizado es la entrada *Unbounded Consumption* del
OWASP Top 10 for LLM Applications (LLM06 en la edición 2026; las fuentes secundarias no coinciden
en la numeración), cuyas mitigaciones respaldan las reglas 1, 7 y 10: topes de herramientas,
tokens y gasto por tarea, límites de pasos y cortacircuitos. Solo se han leído resúmenes de
terceros, no el documento original. **No consta** referencia normativa sobre el tamaño de la
salida de una herramienta (ni en MCP ni en guías de truncado): una búsqueda sin resultado no
prueba que no exista. Las cifras (20 000 bytes, 300 coincidencias, 200 elementos) son decisiones
**propias**, sin respaldo externo.

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
   había. *Estado: hecho para `glob`. `grep` respeta `.gitignore` pero no tiene máximo propio
   de coincidencias (lo cubre solo la cota general): por afinar.*
4. **Recortar avisa y orienta.** Un recorte silencioso engaña al modelo. El aviso dice cuánto se
   mostró, cuánto había y qué hacer («estrecha el patrón», «usa `offset`»).
5. **El historial no crece sin límite.** Cada paso reenvía toda la conversación. Hace falta
   compactación o resumen de lo antiguo. *Estado: **pendiente**; no existe. Es el mayor gasto
   latente. Véase `2026-10-14-loom-kpis-prompt-compression.md`.*
6. **El prompt de sistema también tiene cota.** Ficheros de contexto (`AGENTS.md`,
   `CODELOOM.md`), lista de agentes, habilidades y memoria se inyectan en cada petición.
   *Estado: **pendiente**; hoy se cargan enteros y sin tope.*
7. **Todo bucle tiene techo, y el coste también.** Existen `max_steps` y `max_turns`; no existe
   presupuesto de tokens por turno ni por sesión. El sondeo (`job_output` en bucle) es un bucle
   disfrazado: se evita en el prompt, pero no se garantiza. *Estado: **parcial**.*
8. **Lo que cruza de un agente a otro también se acota.** El resultado de un subagente vuelve al
   padre y lo paga cada paso siguiente. *Estado: por comprobar que el resultado del hijo tenga
   cota propia (hoy solo le alcanza la general de herramientas).*
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

- ¿Qué tope tiene sentido por herramienta? 20 000 bytes es una cota general prudente, no una
  medida afinada. Conviene medir con sesiones reales.
- ¿Presupuesto de tokens por turno y por sesión, con aviso a la persona usuaria al acercarse?
- ¿Compactación automática o a petición (`/compact`)?
