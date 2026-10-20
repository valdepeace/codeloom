# Hallazgos de código: UI de MastraCode y Crush

> Corte de investigación: 2026-10-18. Inspección estática; todavía no es una
> especificación visual 1:1. Los registros de runtime y sus límites están en
> `runtime-observations.md`.

## Alcance y fijación de versiones

- MastraCode instalado: `0.47.1`. Su bundle TUI local incluye source map con
  fuentes TypeScript (`dist/tui-B5689bGt.cjs.map`, 167 fuentes). El mapa se
  inspeccionó localmente sin ejecutar el bundle ni copiar sus fuentes al repo.
  SHA-256 del bundle TUI previamente registrado:
  `0c2de11834e3a3c4f5f67acb1c8cdc05ef38505da51d8e4b1c4dff684d0ff034`.
- Crush instalado: `v0.98.2-0.20261017181040-852f410573e1+dirty`. `go version
  -m` fija `vcs.revision=852f410573e199fa0235cd8a8f32d6b0840373d3`,
  `vcs.time=2026-10-17T18:10:40Z` y `vcs.modified=true`. El binario lleva
  Bubble Tea v2.1.0, Bubbles v2.2.1, Lipgloss v2.0.6 y Ultraviolet.
- Se encontró el checkout local `E:\workspace\personal\crush` en ese mismo
  SHA. Los archivos bajo `internal/ui/` están limpios respecto al commit; los
  cambios de trabajo detectados están fuera de esa carpeta. Por eso el
  recorrido de UI usa la fuente exacta del binario. `+dirty` sigue indicando
  otros cambios locales no identificables desde el binario, pero no afectan
  estos archivos según el estado actual del checkout.

## MastraCode 0.47.1: hallazgos de fuente local

Fuentes del source map que se relacionan directamente con la interfaz:

- `src/tui/components/custom-editor.ts`: subclass del `Editor` de `pi-tui`.
  Renderiza una fila de prompt con indicador dependiente del contenido (`→`,
  `/`, `@`, `!`), sombreado gradual y sin un borde rectangular tradicional.
  Delega edición y wrap a `pi-tui`, aparta marcadores decorativos del texto
  durante el cálculo de layout y restaura cursor/estado al terminar. Conserva
  indicadores de scroll del editor; implementa además estados de dictado.
- `src/tui/theme.ts`, `bundled-themes.ts`, `custom-themes.ts`,
  `detect-theme.ts`, `commands/theme.ts`: modo dark/light, detección del fondo
  de terminal, temas emparejados light/dark y JSON de temas propios. Los
  colores tienen roles semánticos separados para texto, bordes, selección,
  mensajes del usuario, thinking y herramientas; fondos de panel se derivan
  para mantener contraste. Cambiar tema invalida componentes que cachean el
  render.
- `src/tui/components/assistant-message.ts`: crea nodos Markdown/texto por
  partes y reconcilia sus componentes durante el streaming; distingue texto y
  thinking y puede ocultar thinking.
- `src/tui/components/tool-execution-enhanced.ts`: componente de herramienta
  con fila compacta o bloque expandible, previews específicos por clase de
  herramienta, salida parcial, indicador de error/estado, formato de resultados
  y límite de líneas al colapsar. La presentación no es un único bloque
  genérico.
- `src/tui/components/tool-approval-dialog.ts`: aprobación en línea debajo de
  la llamada; ajusta opciones a terminal estrecho y muestra argumentos completos
  con wrap o un resumen expandible antes de aprobar.

### Implicación visual para Loom

La firma de MastraCode se apoya en superficies suaves y sangrías, prompt con
marcador cromático integrado, respuestas Markdown como flujo y herramientas
compactas/expandibles con vistas particulares. Copiar solo la paleta perdería
la mayor parte de la identidad observada. Para una reproducción fiel hay que
mapear también geometría, espaciado, filas de estado, wrapping, cursor y estados
de herramientas.

### Dependencia `@earendil-works/pi-tui` y comparación del editor

La instalación local fija `@earendil-works/pi-tui` **0.80.6** en
`mastracode/package.json`; los artefactos publicados incluyen
`node_modules/@earendil-works/pi-tui/dist/components/editor.js` y
`editor.d.ts`. El paquete declara licencia **MIT** y enlaza al repositorio
`earendil-works/pi`, directorio `packages/tui`. El paquete npm instalado no trae
un archivo `LICENSE`; corroboré la licencia en el `package.json` del repositorio
oficial. Si se copia o adapta código fuente, la redistribución debe conservar
el aviso de copyright y el texto de licencia MIT. Reimplementar los patrones de
interacción en Rust evita incorporar código JavaScript; no equivale a integrar
la librería. La clase `CustomEditor` pertenece a MastraCode y su código tiene
una licencia distinta (el paquete MastraCode declara Apache-2.0); el MIT de
pi-tui no cubre esa extensión.

Pi TUI aporta un `Editor` con buffer/cursor multilínea, navegación vertical con
columna preferida, undo/kill ring, historial, wrap por palabras con fallback
por grafemas, scroll que mantiene visible el cursor, autocomplete de comandos y
rutas, y detección de pegados largos que los representa como marcador. Renderiza
líneas horizontales arriba/abajo, cursor invertido y, cuando hay scroll,
indicadores de líneas ocultas. `Enter` envía; Shift/Ctrl/Alt+Enter añaden línea
cuando el terminal transmite esas combinaciones. El README ofrece `\\`+Enter
como alternativa en terminales que no distinguen Shift+Enter.

MastraCode **extiende** `Editor` con `CustomEditor` (confirmado en el `.d.ts`
publicado) para interceptar acciones propias, pegado de imágenes, voz y
renderizado contextual. Por tanto, no es correcto atribuir al editor base toda
la conducta de MastraCode. La app añade prompt estilizado y coloca el editor en
su composer/footer.

| Aspecto | pi-tui 0.80.6 / MastraCode | Loom actual |
|---|---|---|
| Buffer y cursor | Multilínea; cursores lógico y visual; conserva columna al subir/bajar | `batten::state::Composer` multilínea, cursor por índice de carácter y columna preferida en líneas lógicas |
| Ajuste al ancho | Wrap por palabras/grafemas; navegación sigue las filas visuales | Widget `Paragraph` con texto del buffer; no implementa wrap/navegación sobre filas visuales |
| Altura y scroll | Límite visible proporcional a la altura del terminal (mínimo 5 filas), desplaza para seguir el cursor e indica contenido oculto | `render.rs` reserva altura a partir del número de líneas lógicas; el widget no desplaza internamente el texto largo |
| Teclas | Enter submit; combinaciones modificadas insertan línea; historial y edición extendida | Enter submit; Ctrl+J inserta línea; flechas editan líneas lógicas o navegan historial |
| Pegado | Modo bracketed paste y marcador para pegados grandes con expansión disponible | Evento `Paste` inserta el texto multilínea completo; no hay marcador/resumen para pegado grande |
| Integración | TypeScript; `Editor` requiere `TUI`, tema, terminal y bucle de render de pi | Rust; Batten/Ratatui y `crossterm`, estado independiente de Ratatui |

**Conclusión:** el editor de pi-tui es una buena referencia de comportamiento y
es funcionalmente más completo; no se puede importar como crate Rust ni montar
dentro de Ratatui como widget. Adoptarlo literalmente exigiría mover el control
del terminal a Node o sostener dos runtimes de TUI. Para CodeLoom, conviene
reproducir primero en Batten wrap visual, scroll con cursor visible, indicadores
de overflow y pegado largo, conservando el contrato de teclas de Loom. El wrap
requiere ampliar el modelo de cursor actual y validar Unicode/anchuras antes de
prometer paridad.

### Alternativa nativa encontrada en `awesome-ratatui`

El catálogo lista `ratatui-textarea` como editor multilinea y apunta a su fork
mantenido por la organización de Ratatui. En la inspección del repositorio
oficial, el manifiesto anuncia versión **0.9.3**, Rust edition 2024, MSRV 1.86 y
licencia MIT. El changelog de 0.9.x confirma que el fork incorporó wrap suave
Unicode y movimiento del cursor por filas visuales; permite seleccionar wrap
por palabra, grafema o combinación. El workspace de Loom usa Ratatui **0.30.2**
según `Cargo.lock` y `rust-toolchain.toml` fija `stable`; por versión/API parece
un candidato compatible, pero todavía no se ha añadido ni compilado en Loom.

`ratatui-textarea::TextArea` implementa `Widget`, guarda su propio buffer y
cursor y maneja edición, scroll, historial undo/redo, selección, búsqueda
opcional, yank y wrap. A diferencia de pi-tui, puede renderizarse dentro de un
`Rect` de Ratatui y recibir los eventos de crossterm que Loom ya maneja. Sus
métodos públicos permiten controlar cada tecla en vez de usar el mapa por
defecto: Loom puede conservar Enter=submit y Ctrl+J=nueva línea llamando las
operaciones correspondientes de forma explícita. El widget de ejemplo renderiza
en el rectángulo que entrega la app; Loom sigue decidiendo la altura variable,
el marco sin borde y el resto del composer.

| Candidato | Encaje con composer de Loom | Límites frente a pi-tui/MastraCode |
|---|---|---|
| `ratatui-textarea` 0.9.3 | Mejor candidato directo: widget Ratatui, editor multilínea, cursor, wrap visual, auto-scroll; MIT | No trae por sí solo el prompt dinámico, autocompletado de archivos/comandos ni el tratamiento Mastra de pegados grandes; Loom mantiene layout y submit |
| `tui-textarea` 0.7.0 | Editor conocido y MIT | Proyecto original; el fork `ratatui-textarea` incorpora el trabajo reciente de wrap visual y se mantiene aparte |
| `edtui` | Editor modal Vim inspirado en Ratatui | Orientado a un editor completo/modal; demasiado amplio para un composer de chat |
| `tui-input` | Estado headless de entrada | Está orientado a input sencillo, no es el editor multilínea equivalente |
| `rat-widget::textarea` | TextArea configurable dentro de una colección amplia de widgets | Añade una dependencia más general y su gestión de foco/eventos necesita evaluarse frente al App existente |

**Recomendación revisada:** antes de portar a mano el algoritmo de wrap de
pi-tui, hacer un prototipo mínimo con `ratatui-textarea` en el composer de
`codeloom-tui`. Mantener `batten::state::Composer` como contrato del core y
comprobar la forma menos duplicada de sincronizar texto/cursor con el estado del
widget. Si ese adaptador causa doble estado o rompe la independencia del core,
portar solo las piezas necesarias (wrap Unicode, navegación de filas visuales,
scroll/cursor) a Batten. Comprobar en el prototipo el submit Enter, Ctrl+J,
historial, pegado, foco del transcript, cursor real y terminal estrecho antes
de decidir adopción.

### Actividad compacta: tool calls y thinking

Esto es otra parte de la UI, separada del editor de entrada. Ratatui aporta
primitivas de composición y render (filas, spans, bloques, widgets con estado y
scroll), pero `awesome-ratatui` no presenta un widget estándar de conversación
que agrupe tool calls y thinking en tarjetas compactas con expansión. Sí lista
apps de agentes hechas con Ratatui: Martty declara que muestra reasoning
streaming y tool calls; Mindfork describe bloques de thoughts plegables. Son
referencias de aplicaciones completas, no una librería enchufable de
transcript.

Loom/Batten ya ofrece las primitivas y el showcase ahora compone una tarjeta de
tool con thinking para recorrer el flujo de actividad. Esto es un ejemplo de
widget en Batten, no integración con los eventos de CodeLoom:

- `batten::state::ToolItem` conserva nombre, estado, resumen, salida y estado
  plegado. `batten::widgets::tool_item` dibuja cabecera, resumen, estado de
  ejecución y preview; al expandir presenta las líneas de salida disponibles.
- `batten::widgets::thinking` presenta una fila de actividad animada. El demo
  `chat` la alterna con una tarjeta de tool simulada y respuesta en streaming;
  `tool_item` permite recorrer todos los estados y expandir resultados.
- `codeloom-core::event::LiveEvent` declara `ReasoningDelta`, `ToolStarted` y
  `ToolFinished`, pero AgentLoop actualmente solo emite los deltas de reasoning;
  los eventos de inicio/fin de tool no se publican. `codeloom-tui::App::apply`
  ignora reasoning, tool events y mensajes no-user, aunque `Message` contiene
  `ContentPart::ToolCall` y `ToolResult`.
- El transcript persiste mensajes como texto derivado por `LineKind`; todavía
  no tiene una entrada viva agrupada por `call_id`. El showcase compone el tool
  actual encima del transcript, pero el runtime no puede actualizar una tarjeta
  dentro del feed cuando llegan inicio, deltas y resultado.

La mejora debe estar en una entidad/widget de actividad del transcript, no en
`ratatui-textarea`: una fila de cabecera (icono + nombre/título + duración), una
fila de estado mientras corre y un preview de una línea para resultado/error;
al expandir, argumentos y salida con wrap/scroll. El thinking debe acumularse en
un bloque separado, contraído por defecto o según el perfil visual. Al recibir
eventos, actualizar el mismo item por `call_id`, para que las filas no se
dupliquen en cada delta. Los estados semánticos vienen del core; el perfil
Mastra/Crush decide densidad, glyphs, colores y presentación. El composer
`ratatui-textarea` puede desarrollarse en paralelo como entrada, pero no
resuelve esta agrupación del transcript.

Como guía visual de esta primitiva, Pawan describe tarjetas enmarcadas de 6
líneas contraídas y 40 expandidas; Martty enseña el recorrido de eventos y
expansión del transcript; Mindfork agrupa thoughts y tools en el feed. Son
referencias de aplicación, no dependencias reutilizables ni prueba de paridad
visual con MastraCode/Crush. Batten aún muestra todo el resultado expandido
dentro del alto que reserve el consumidor; no tiene viewport/scroll propio por
tarjeta.

## Crush: hallazgos de fuente exacta

- `internal/ui/model/ui.go`: el editor es `Bubbles textarea.Model`, sin
  números de línea, sin límite de caracteres, cursor no virtual y altura
  dinámica limitada a 3–15 filas. El layout añade 2 filas para adjuntos y
  margen inferior; el help ocupa normalmente una fila. El crecimiento del
  editor reduce el área de chat y hay pruebas específicas para ese
  comportamiento.
- La pantalla de chat no compacta divide el área en chat + sidebar de 32
  columnas; editor y chat ocupan la columna principal. La sidebar ordena título,
  proyecto/rama, modelo/proveedor/razonamiento/uso, archivos, LSP, MCP y Skills
  en un panel desplazable con logo fijo. El modo compacto sustituye la sidebar
  por una cabecera de una fila; hay umbrales de ancho 120 y alto 30, además de
  un toggle persistente. El landing usa cabecera de 4 filas, contenido, editor
  y help.
- `internal/ui/model/sidebar.go`, `ui.go`, `chat/assistant.go`, `chat/tools.go`
  y `chat/file.go` trazan vistas concretas. Assistant separa thinking, texto y
  error/refusal, con thinking colapsable en estados (colapsado, cola, completo).
  Las herramientas distinguen permiso pendiente, ejecución, éxito, error y
  cancelación; los renderers dedicados de archivos y shell presentan
  parámetros, resultados, errores/diffs y salida expandible de manera
  específica. Hay cachés de render por ancho/versión.
- `internal/ui/styles/quickstyle.go`, `themes.go`, `palette.go` y
  `theme_file.go`: tokens semánticos alimentan el struct central `Styles`;
  dos temas integrados (`charmtone-panther`, `gruvbox-dark`) y temas JSON
  heredan una paleta base. El tema refresca textarea, sidebar, header, diálogos,
  completions y cachés de mensajes. Esto es una selección de estilo completa,
  pero los dos temas integrados no cambian la geometría de la interfaz.
- Las observaciones locales de runtime coinciden con la estructura: banner,
  ruta, proveedor/modelo y razonamiento, paneles de estado LSP/MCP/Skills,
  prompt `> Ready...` y ayudas al pie. La captura disponible es texto VT; la
  sesión seleccionó OpenRouter y no se envió petición.

### Implicación visual para Loom

El estilo visible de Crush incluye composición y densidad: cabecera/banner,
contexto del proveedor, panel lateral de recursos, textarea de altura dinámica
y footer de atajos. Para emularlo, Loom necesita un perfil que cambie
estructura/layout, no solo tokens de color. El modo compacto también forma
parte de su responsive layout y debe incluirse en las capturas comparables.

## Decisión técnica que sí está sustentada

En Loom, `batten::widgets::Theme` define estilos semánticos, glifos y carga
parcial de colores desde TOML. Batten tiene ahora seis `ThemePreset` inspirados
en los repositorios y el showcase permite recorrerlos con F7/Shift+F7. Aún son
paletas aplicadas a los widgets/layout de Batten; `codeloom-tui::render` decide
cabecera, transcript, sidebar, cambios de archivos y composer y todavía no
selecciona perfiles desde la configuración de TUI. `Theme::from_toml` y los
presets asignan principalmente tokens de color; no cubren todas las
superficies, fondos por componente y geometrías de los referentes. Por tanto:

1. Modelar los dos referentes como perfiles de presentación (estructura,
   componentes, espaciado y tokens), no como dos temas cromáticos.
2. Mantener compartidos `App`, transcript, herramientas, permisos y teclas; el
   perfil solo selecciona render/layout/estilos.
3. Implementar reproducciones con código propio guiado por el comportamiento
   visible, sin trasladar implementaciones fuente de esos proyectos.
4. Después de cerrar esta inspección, hacer capturas comparables con mismo
   tamaño de terminal y estados equivalentes; recién allí ajustar fidelidad y
   diseñar el perfil original de Loom.

### Mapa de reproducción hacia Loom

| Parte visible | MastraCode | Crush | Base actual en Loom | Trabajo para acercarse al referente |
|---|---|---|---|---|
| Prompt | Flecha/indicador cromático, panel sombreado sin marco rígido, editor wrap/scroll | Textarea con prompt/placeholders y altura dinámica | Batten composer con borde, tema y cursor; Loom reserva su área en `render.rs` | Dos renderers/layouts; medir alto, padding, cursor, scroll y prompts con capturas |
| Transcript | Markdown fluido con separación temática y thinking opcional | Chat como lista virtual con thinking, contenido y error separados | Transcript Batten con Markdown, scroll y selección | Ajustar indentación, separadores, densidad y estados por perfil |
| Tools | Compacto/expandido, preview específico, salida/error y aprobación inline | Estado de permiso/ejecución/final; renderers dedicados por familia de tool | Batten `tool_item`/transcript y modelo de permisos Loom | Comparar por herramienta y estado; no reducir todo a un único bloque |
| Marco general | Fila de estado inferior con modo/modelo/contexto; sin sidebar visible en el frame base observado | Logo/header, sidebar con proyecto/modelo/recursos, compact mode y help inferior | Header y sidebar Loom propios | Perfil Mastra: reducir/rehacer chrome; perfil Crush: regiones, sidebar y compacto |
| Apariencia | Dark/light, pares de temas y JSON con fondos/superficies/contraste | Paleta semántica, 2 temas incluidos + JSON derivados de base | Tokens TOML y seis paletas de referencia en Batten; showcase las recorre con F7 | Llevar la selección a CodeLoom y extender estilos a superficies/layout por perfil |

El perfil “MastraCode” o “Crush” significa reproducir composición y reglas de
presentación con componentes propios. Los dos no deben reducirse a un selector
de paleta; la elección ha de afectar solo al renderer/layout/estilo, manteniendo
compartidos el estado y el contrato funcional de Loom.

No se concluye aún que pueda alcanzarse equivalencia píxel/celda por celda: la
TUI depende de capacidades, fuente, terminal, tema activo y estado de la sesión.

## Fuentes

- Fuente local MastraCode: source map empaquetado descrito arriba; rutas de
  fuentes listadas por nombre para trazabilidad, sin reproducir su contenido.
- `mastracode/package.json` y `node_modules/@earendil-works/pi-tui/package.json`
  locales (versiones 0.47.1 y 0.80.6); `pi-tui/dist/components/editor.js`,
  `editor.d.ts` y `README.md`; fuente oficial de
  [editor.ts](https://github.com/earendil-works/pi/blob/main/packages/tui/src/components/editor.ts),
  [package.json](https://github.com/earendil-works/pi/blob/main/packages/tui/package.json)
  y [licencia del repositorio](https://github.com/earendil-works/pi/blob/main/LICENSE).
- Loom: `crates/batten/src/state/composer.rs`,
  `crates/batten/src/widgets/composer.rs`, `crates/codeloom-tui/src/app.rs`,
  `run.rs` y `render.rs`.
- `awesome-ratatui` y repositorio oficial de
  [ratatui-textarea](https://github.com/ratatui/ratatui-textarea),
  [README/features](https://github.com/ratatui/ratatui-textarea/blob/main/README.md),
  [Cargo.toml/licencia/MSRV](https://github.com/ratatui/ratatui-textarea/blob/main/Cargo.toml)
  y [changelog de wrap](https://github.com/ratatui/ratatui-textarea/blob/main/CHANGELOG.md).
- `awesome-ratatui` [lista de apps AI](https://github.com/ratatui/awesome-ratatui#-ai-and-agents),
  [Martty Agent UI](https://github.com/openma-ai/Martty) y
  [Mindfork](https://github.com/vshylov/mindfork-rs); [Pawan](https://github.com/dirmacs/pawan)
  para sus tarjetas compactas y el indicador de thinking. SHA inspeccionados,
  licencias, rutas y conclusiones: `repos-analisis.md`.
- Crush instalado: `go version -m C:\Users\negrero\bin\crush.exe`.
- Fuente local Crush, commit `852f410573e199fa0235cd8a8f32d6b0840373d3`:
  `internal/ui/model/ui.go`, `sidebar.go`, `layout_test.go`,
  `internal/ui/chat/{assistant,tools,file,shell}.go`,
  `internal/ui/styles/{styles,quickstyle,themes,palette,theme_file}.go`,
  `internal/config/config.go`.
