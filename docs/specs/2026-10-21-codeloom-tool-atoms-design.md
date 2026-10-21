# Piezas atómicas por herramienta y catálogo de herramientas en el showcase

Estado: aprobada en conversación el 2026-10-21 (petición del usuario: «todas
estas tools podríamos poner alguna representación en batten, y así poder ir
afinando los themes por provider; como en compose muestra un diff, write, todo,
etc.; así podemos atomizar estilos y vistas»). Plan:
`docs/plans/2026-10-21-codeloom-tool-atoms.md`.
Relacionado: `crates/batten/examples/showcase` (catálogo por provider ya
existente, con demos dibujadas a mano).

## Intención

Hoy todas las herramientas se pintan igual: `tool_lines` produce una cabecera
(`nombre  resumen`), el cuerpo con la salida plegada y una marca de líneas
ocultas, y el «resumen» es el JSON de entrada en crudo. No hay vista propia de
`edit` (un diff), `bash` (comando y salida), `view` (ruta y rango), `grep`/`glob`
(patrón y número de coincidencias), `ask_user`, `subagent*` o `job_*`. Y el
showcase, aunque tiene demos por provider (Crush, Martty, Mindfork…), las
dibuja a mano con texto fijo, sin pasar por lo que usa la app: afinar un tema
allí no garantiza que cambie el chat.

Se pretende que cada herramienta real tenga una **pieza atómica** de render en
`batten`, única fuente de verdad que usan la app y el showcase, y poder afinar
los estilos de cada pieza por preset/provider.

## Objetivos

1. Un registro de piezas por nombre de herramienta, con una pieza genérica de
   reserva idéntica al comportamiento actual (herramientas desconocidas, MCP).
2. Piezas para las herramientas reales: `bash`, `view`, `edit` (con diff),
   `grep`, `glob`, `todo_write` (ya existe su render), `ask_user`, `subagent`,
   `subagent_control`, `subagent_list` y `job_start/list/output/kill`.
3. `batten` no depende de `serde`: un adaptador en `codeloom-tui` extrae de la
   entrada JSON de cada llamada unos «hechos» tipados (`ToolFacts`) y la pieza
   solo pinta hechos y estado.
4. Tokens de estilo propios de las piezas (ruta, comando, recuento…) con valores
   por defecto derivados de los existentes y sobrescribibles por preset.
5. Un catálogo «CodeLoom tools» en el showcase: cada herramienta real × estados
   (en curso, éxito, error, esperando permiso, plegada/expandida), con datos de
   ejemplo y pasando por la misma función de render que la app, bajo el preset y
   el filtro de provider activos.

## No objetivos

- Herramientas que no existen en CodeLoom (p. ej. `write`, web, LSP): solo hay
  `edit`; cuando existan se añaden como una pieza más.
- Cambiar el protocolo, la ejecución o los permisos de las herramientas.
- Rediseñar los presets distintos de Crush más allá de dar valores a los tokens
  nuevos (los demás heredan los valores por defecto).
- Mover a `batten` los widgets de diálogo de permiso (ya existen).

## Anti-objetivos

- Que cambie el aspecto de las herramientas desconocidas o del look Classic sin
  motivo: la pieza genérica reproduce el render actual byte a byte.
- Que parsear JSON ocurra en el camino de los deltas (por cada token) o en
  `render`: solo al sincronizar la herramienta, como hoy.
- Que `batten` adquiera dependencias de `codeloom-core` o `serde`.
- Que el showcase tenga un camino de render distinto del de la app.

## Decisiones de diseño (mías, marcadas para revisión)

- **`ToolFacts` como frontera** (en `batten::state`): campos tipados y opcionales
  (`primary`: ruta, comando o patrón; `secondary`; `range`; `before`/`after` para
  el diff; `count`; `items`: pares clave–valor para el resto). El adaptador
  `tool_facts(name, &serde_json::Value)` vive en `codeloom-tui`.
- **`trait ToolAtom`** con `fn lines(&self, item: &ToolItem, facts: &ToolFacts) -> Vec<Line>`
  y un registro `atom_for(name) -> &'static dyn ToolAtom`. Las líneas siguen
  siendo `Line`/`LineKind`/`SpanSpec` del transcript, de modo que la caché, el
  render y la selección actuales no cambian.
- **Líneas nuevas** solo donde hacen falta (`ToolDiff(DiffKind)` para las líneas
  de diff de `edit`), pintadas con los tokens `DiffInsert`/`DiffDelete`/
  `DiffHunk` que ya existen.
- **Tokens nuevos** de pieza: `ToolPath`, `ToolCommand`, `ToolCount`, con valor
  por defecto derivado de `Accent`/`Muted`/`Dim` y sobrescribible por preset en
  `Theme`.
- **El catálogo reutiliza** `ToolItem` + `ToolFacts` + `atom_for` + `Transcript` +
  `transcript::render`, no dibuja nada a mano.
- Las demos existentes por provider no se tocan (siguen como referencia visual).

## Qué se deja al implementador

Formato exacto de cada pieza (cuántas líneas de contexto del diff, truncados),
nombres internos de los campos de `ToolFacts` no fijados en el plan y la
organización de los datos de ejemplo del catálogo.

## Verificación

Pruebas puras de: registro y reserva genérica (idéntica al render actual), cada
pieza con sus estados, el adaptador con las entradas reales de cada
herramienta, los tokens y sus valores por defecto, y el catálogo (una entrada por
herramienta y estado, mismo render que la app). Validación visual en terminal
real: imprescindible y pendiente.
