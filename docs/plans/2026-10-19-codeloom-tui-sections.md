# Plan de trabajo de la TUI por secciones

El trabajo se cierra por flujos completos de usuario, no por atajos aislados.
Cada sección tiene alcance verificable y se marca terminada solo cuando sus
estados principales y casos de error están cubiertos.

## Cierre M1 — 2026-10-20

La implementación y la suite quedan cerradas: `cargo fmt --all --check`,
`cargo build`, `cargo test` (367 pruebas) y `cargo clippy -- -D warnings`
pasan con un target local. El usuario confirmó en terminal interactiva que
`Ctrl+J` conserva las líneas y el cursor visibles y que doble Esc sale. La
inspección visual amplia del resto de estados sigue pendiente.

## Estado

| Sección | Alcance | Estado |
|---|---|---|
| Conversación | Composer multilínea, comandos slash, envío, streaming, markdown, selección y scroll; look Crush (composer sin caja con wrap/scroll, tools inline en orden) | Implementada; validación visual pendiente |
| Cola de prompts | Encolar, pausar, editar, reordenar, borrar, previsualizar y cancelar el turno activo | Implementada; validación visual pendiente |
| Sesiones | Crear, reanudar, filtrar, refrescar, indicar la activa y recuperar errores | Implementada; validación visual pendiente |
| Actividad y workspace | Estado de tools, inspector de llamadas y salida, métricas, cambios de archivos y sidebar adaptable | Implementada; validación visual pendiente |
| Permisos y proveedor | Resolver permisos y editar/guardar configuración desde la TUI | Implementada; validación visual pendiente |
| Terminal y navegación | Atajos, foco, overlays, ratón, terminales pequeños y restauración al salir | Implementada; validación en terminal real pendiente |

## Criterio de cierre por sección

- Los estados normal, vacío/cargando y error tienen una respuesta visible y
  permiten continuar o recuperarse.
- Los controles están documentados en la ayuda contextual correspondiente.
- Las pruebas cubren el flujo y sus límites; `cargo fmt --check` y
  `git diff --check` pasan.
- La inspección en terminal real queda registrada como hecha o pendiente; las
  pruebas con `TestBackend` no cuentan como confirmación visual.

## Composer: comandos slash

Escribir `/` al inicio de una entrada de una sola línea abre las sugerencias de
Batten. Filtra por prefijo; ↑/↓ selecciona, Tab completa y Enter ejecuta la
opción seleccionada. Comandos disponibles:

- `/help`: ayuda de teclado.
- `/new`: crea una sesión nueva cuando el turno está inactivo.
- `/sessions`: abre el selector de sesiones cuando el turno está inactivo.
- `/settings`: ajustes de proveedor/modelo.
- `/queue`: gestión de la cola.
- `/tools`: inspector de llamadas recientes.
- `/theme`: cambia al siguiente tema disponible y muestra el activo en la cabecera.
- `/cancel`: cancela el turno activo y pausa los pendientes; si no hay turno,
  conserva el comando y muestra el motivo.
- `/pause`: pausa los pendientes sin interrumpir el turno activo.
- `/resume`: reanuda la cola y mantiene el despacho de uno en uno.
- `/sidebar`: alterna la barra lateral, como Ctrl+B.
- `/quit`: sale de CodeLoom.

Estas acciones reutilizan los flujos y atajos existentes. No entran en el
historial de prompts ni se encolan para el provider. Los comandos desconocidos
y las acciones bloqueadas conservan el texto y muestran el motivo.
Pausar/reanudar es idempotente. Al terminar o fallar un turno se limpia cualquier
petición de cancelación pendiente para que no alcance al turno siguiente.

Esc cierra las sugerencias sin cancelar el turno; el texto se conserva y puede
enviarse literalmente con Enter. Editarlo vuelve a activar las sugerencias.
Las entradas multilínea, frases como `/help me`, rutas como `/tmp/file.rs` y
texto con `/` intermedio siguen siendo prompts normales. Un token aislado como
`/tmp` requiere Esc para enviarlo literalmente. No hay argumentos ni comandos
personalizados en esta primera versión.

Validación: build, 357 pruebas, formato y revisión del diff pasan con el target
local. Se cubren selección, completado, ejecución local, protección de sesiones
activas, errores, texto literal, prioridad de permisos y render en tamaños
pequeños. Las pruebas de control verifican pausa sin cancelación, cancelación con
pausa, reanudación secuencial y cancelación tardía al terminar o fallar.
Clippy mantiene los siete avisos previos de Batten; validación visual
en terminal real pendiente.

## Actividad: inspector de herramientas

Ctrl+T abre el inspector de las ocho llamadas recientes conservadas por la app.
↑/↓ selecciona la llamada, PgUp/PgDn desplaza argumentos y salida, ←/→ permite
leer líneas anchas, Home/End salta al inicio/final y Y copia la salida completa
recibida mediante el portapapeles existente. Esc o Ctrl+T cierra sin cancelar
el turno ni alterar el borrador.

Muestra el estado y distingue ausencia de llamadas, argumentos y salida. Los
eventos actualizan el inspector mientras permanece abierto. Si una llamada se
descarta por el límite existente de ocho, la selección se ajusta; si era la
seleccionada, se reinicia el desplazamiento. Los permisos cierran el inspector
para recibir el foco. Reanudar otra sesión también restablece su estado.

La salida no se envuelve: el desplazamiento horizontal permite leer las líneas
completas. En terminales pequeñas se indica que hay que ampliar la ventana.
La copia usa OSC52 y depende del soporte y configuración del terminal.

Validación: build, 351 pruebas, formato y revisión del diff pasan con el target
local. Hay pruebas de teclado, copia, retención/selección, cambio de sesión,
desplazamiento en ambos ejes, estado vacío y tamaños pequeños. Clippy mantiene
los siete avisos previos de Batten; inspección visual y copia en terminal real
pendientes.

## Cola de prompts: pausa, edición y cancelación

Ctrl+K abre la cola y suspende el despacho mientras el diálogo está abierto.
Space alterna pausa/reanudación; al cerrar, los pendientes solo se ejecutan si
la cola no está pausada. La pausa queda indicada también en el header.
No interrumpe por sí sola el turno activo.

Enter abre un editor independiente para el prompt seleccionado: Enter confirma
un texto no vacío, Ctrl+J añade línea, Ctrl+A limpia y Esc descarta la edición.
El original permanece en cola hasta confirmar y el borrador del composer no se
toca. Si llega un permiso, la edición se descarta conservando el original.

Ctrl+X desde la lista cancela el turno activo y deja la cola pausada. El usuario
puede revisar los pendientes antes de reanudar. La cola sigue siendo volátil.

Validación: 348 pruebas pasan; incluyen edición/descartado, pausa, cancelación,
reanudación secuencial, canal cerrado y render del editor y sus controles. Build,
formato y revisión del diff pasan; Clippy mantiene los siete avisos previos de
Batten. La validación en terminal real sigue pendiente.

## Permisos y proveedor: integración

El guardado exige un turno inactivo y bloquea la edición hasta recibir respuesta.
Un fallo mantiene el modelo activo y permite reintentar; un worker cerrado tiene
un error visible. Los permisos cierran ayuda, ajustes y cola de prompts para
recibir el foco, y omiten peticiones en cola cuyo solicitante ya se ha cerrado.
El pegado respeta los diálogos. Ajustes muestra una indicación para ampliar la
terminal cuando no cabe el formulario.

Las pruebas de teclado, estado y render cubren guardado, error, reintento, cierre
del worker, prioridad de permisos y formulario en terminal pequeña. La inspección
visual de todas las secciones queda pendiente de una sesión en terminal real.

Validación de esta integración: `cargo build`, `cargo test` (342 pruebas),
`cargo fmt --check` y `git diff --check` pasan. Build y tests usan
`CARGO_TARGET_DIR=E:\workspace\personal\loom\target` porque el target compartido
de jevalon tiene `loom.exe` abierto. `cargo clippy -- -D warnings` falla por siete
avisos preexistentes en `attachment_preview`, `help_overlay`, `profile_view`,
`session_list`, `settings_panel` y `status_bar` de Batten.

## Terminal y navegación: integración

Tab y Shift+Tab alternan composer y transcript conservando el borrador. La
repetición de teclas permite escribir, borrar y navegar en la conversación;
no confirma envíos ni permisos ni actúa sobre diálogos. El cursor del composer
queda oculto mientras un diálogo recibe el foco.

Sesiones, ayuda, cola y ajustes muestran una indicación cuando no caben. La
ayuda tiene espacio para todos sus controles y el indicador de carga centra su
ancho sin restas que desborden en terminales estrechos.

La guarda de restauración se crea antes de configurar la pantalla alternativa,
para cubrir fallos parciales de arranque. Al salir también cierra el modo de
actualización sincronizada y muestra el cursor.

Validación: `cargo test` (345 pruebas), `cargo build`, `cargo fmt --check` y
`git diff --check` pasan usando el target local indicado arriba. Las pruebas
cubren seis estados de diálogo desde 0×0 hasta 120×40 y la conservación del foco
y del borrador. Clippy mantiene los siete avisos previos de Batten. La
restauración y la inspección visual en terminal real quedan pendientes.
