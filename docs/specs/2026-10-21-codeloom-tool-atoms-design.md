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

## Estado (2026-10-21)

Las tareas 1 a 6 del plan están implementadas, cada una en su commit. Queda
pendiente la validación visual en terminal real.

### Qué se implementó

| Tarea | Contenido | Commit |
|---|---|---|
| T1 | Frontera `ToolFacts`, trait `ToolAtom`, registro `atom_for` con la pieza genérica de reserva y adaptador `tool_facts` en `codeloom-tui` | `e615a86` |
| T2 | Piezas de `bash`, `view`, `grep` y `glob` | `3dae77b` |
| T3 | Pieza de `edit` con diff y la variante `LineKind::ToolDiff(DiffKind)` | `30c253b` |
| T4 | Piezas de `ask_user`, `subagent`, `subagent_control`, `subagent_list`, `job_start`, `job_output`, `job_kill` y `job_list` | `afb3070` |
| T5 | Tokens `ToolPath`, `ToolCommand` y `ToolCount`, con valores por defecto derivados y sobrescribibles por preset | `0ece023` |
| T6 | Catálogo «CodeLoom tools» en el showcase, con fixtures tras la feature `fixtures` | `ce21ff2` |

Pruebas: `cargo test -p batten --lib` pasa con 370 pruebas y
`cargo test -p codeloom-tui` pasa con 258.

### Decisiones tomadas al ejecutar

- `grep` usa el campo `glob` de su entrada como `secondary`, porque su entrada
  no tiene `path`. El resumen queda `patrón in {glob}`.
- `view` informa de `range` solo cuando hay `limit`. Sin él no hay fin conocido.
  La numeración empieza en `offset + 1`.
- La marca del diff (`+`, `-` o un espacio de contexto) viaja en el propio texto
  de la línea, no en un campo aparte. Las cabeceras de hunk van sin marca.
- El diff de `edit` se limita a 12 líneas con 2 de contexto. `Ctrl+O` (que
  alterna `expanded` de la herramienta seleccionada) las muestra todas.
- Las listas (`subagent_list`, `job_list`) no infieren recuento de una salida de
  una sola línea, porque el núcleo devuelve JSON compacto. Sin recuento
  explícito, solo cuentan las salidas de más de una línea.
- Los fixtures replican a mano lo que extrae el adaptador (`ToolItem` más
  `ToolFacts`). No pasan por `tool_facts`.
- `todo_write` no pasa por `atom_for`. Su vista es `todo_lines`, igual que en la
  app.
- Las entradas del catálogo van al final de la lista de demos, bajo el proveedor
  «CodeLoom».
- Crush redefine solo `ToolPath` (con su `success`, verde menta). `ToolCommand` y
  `ToolCount` heredan `default` y `dim`. El resto de presets hereda los tres.

### Limitaciones conocidas

- El diff de `edit` se recalcula completo en cada `sync_tool`, es decir, en cada
  transición de estado. El coste es O(n) en el tamaño del diff.
- El recuento de `subagent_list` y `job_list` queda pendiente. Hará falta que el
  núcleo emita formato multilínea o un campo de recuento explícito.
- Solo Crush da valores propios a los tokens nuevos, y de ellos solo `ToolPath`.
- `write` no existe aún en `codeloom-core` (solo hay `edit`), y tampoco las
  herramientas A2A. Cuando existan, serán una pieza más.
- El catálogo no incluye `job_output` ni `job_kill`. Sus piezas existen y se
  prueban, pero no tienen entrada de demo.
- Validación visual en terminal real: pendiente.

### Cómo verlo

Desde la raíz del repositorio:

```
cargo run -p batten --example showcase
```

En el selector, busca el proveedor «CodeLoom». Dentro de cada demo, `r` cicla los seis
estados (pendiente, en curso, éxito, error, esperando permiso y cancelada) y `e`
alterna entre plegada y expandida.
