# Inventario de componentes visuales de referencia

Registro de qué superficies visibles de **Martty**, **Mindfork**, **Pawan** y
**Crush** se han reimplementado en Batten y cuáles siguen pendientes. El alcance es la UI
interactiva del terminal revisada en los snapshots locales del
`2026-10-18`; no pretende inventariar cada pantalla funcional de las apps
completas.

Las implementaciones son widgets Ratatui propios inspirados en las referencias;
no se importa su código fuente. **Importado** significa que Batten tiene widget
y demo en el showcase. **Parcial** significa que existe una demo, pero faltan
partes de la superficie o hay una diferencia de render conocida. **Pendiente**
significa que la superficie se identificó en la fuente y aún no tiene demo
dedicada. El estado de copia y el de comparación visual se registran por
separado: ninguna fila se considera 1:1 verificada hasta comparar capturas a la
misma resolución.

## Martty

Referencia fijada: `openma-ai/Martty`, snapshot `bf66b4a3dfbcebbfe1d91e76c5af39d63f921337` (MIT).

| Componente de referencia | Fuente upstream | Batten / demo | Importación | Comparación visual / notas |
|---|---|---|---|---|
| Composer y estado de entrada | `src/input/composer.rs`, `src/input/editor.rs` | `widgets/composer::render_martty` · `Martty · composer` | Importado | Captura comparativa pendiente |
| Model picker | `src/ui.rs`, `src/app.rs` | `widgets/model_picker::render_martty` · `Martty · model picker` | Importado | Captura comparativa pendiente |
| Selector de harness | `src/harness_badge.rs`, `src/ui.rs` | `widgets/harness_picker::render_martty` · `Martty · harness picker` | Importado | Captura comparativa pendiente |
| Add harness: catálogo, filtro y grupos | `src/ui.rs` | `widgets/harness_picker::render_martty_add` · `Martty · add harness` | Importado | Captura comparativa pendiente |
| Remove harness y confirmación | `src/ui.rs` | `widgets/harness_picker::render_martty_removal` · `Martty · remove harness` | Importado | Captura comparativa pendiente |
| Menú de slash commands / skills | `src/ui.rs`, `src/slots.rs` | `widgets/command_suggestions::render_martty` · `Martty · commands/skills` | Importado | Captura comparativa pendiente |
| Marca Martty: ballena, wordmark y welcome | `src/logo.rs`, `src/ui.rs`; `src/slots.rs` selecciona marca | `Martty · welcome`, `Logo · Martty · whale + wordmark` | Parcial | Cubre la identidad Martty; la ballena está adaptada del SVG. Falta cotejo visual |
| Marca DeepSeek Harness y tamaños responsivos | `src/deepseek_logo.rs` (`lines`, `whale_for_width`, `wordmark_lines`), `src/slots.rs` | Sin demo dedicada | Pendiente | Segunda identidad de welcome, seleccionada para el agente DeepSeek Harness; whale cambia entre 4 tamaños y el wordmark cambia por anchura (caption, arte ASCII y co-marca a ≥126 columnas) |
| Preview de adjunto de imagen | `src/attachments.rs`, `src/ui.rs` | `widgets/attachment_preview::render_martty_preview` · `Martty · attachment preview` | Parcial | Miniatura en media celdas en vez del protocolo Kitty; comparar recorte, escala y tarjeta |
| Tool call y salida | `src/transcript.rs` | `widgets/tool_item::render_martty` · `Martty · tool call` | Importado | Captura comparativa pendiente |
| Thinking / reasoning plegable | `src/transcript.rs` | `widgets/thinking::render_martty` · `thinking · Mindfork + Martty` | Importado | Captura comparativa pendiente |
| Transcript y mensajes completos | `src/transcript.rs`, `src/markdown.rs` | No hay demo integral de transcript Martty | Parcial | Hay demos de tool/thinking; faltan mensajes, markdown y composición completa del transcript |
| Elicitación / solicitud interactiva | `src/elicitation.rs`, `src/ui.rs` | Sin demo dedicada | Pendiente | Revisar estados y controles de respuesta |
| Lista y cambio de sesiones | `src/sessions.rs`, `src/ui.rs` | Sin demo dedicada | Pendiente | Revisar selector/listado visible de sesiones |
| Badge de harness | `src/harness_badge.rs`, `src/ui.rs` | `widgets/harness_picker` solo muestra el harness dentro del selector | Parcial | Falta aislar el badge pequeño de sesión y sus variantes |
| Mención `@archivo` y sugerencias de archivos | `src/file_ref.rs`, `src/ui.rs` | Sin demo dedicada | Pendiente | Incluir token inline y popup de archivos como piezas distintas |
| Sesiones hijas / navegación compacta | `src/ui.rs` (`draw_session_tabs`, `draw_child_navigation`) | Sin demo dedicada | Pendiente | Revisar pestañas, estado activo y pistas de navegación |
| Indicador de mascota junto al composer | `src/pet.rs`, `src/ui.rs` | Logo/banner únicamente en welcome | Parcial | La mascota del dock no tiene una demo aislada |
| Dock del composer y caja de entrada | `src/ui.rs` (`draw_composer_dock`, `draw_composer_box`, `draw_input`) | `widgets/composer::render_martty` · `Martty · composer` | Parcial | Falta cotejar por separado caja, prompt y dock/meta row |
| Metadatos debajo del composer | `src/ui.rs` (`draw_meta_row`) | No hay demo dedicada | Pendiente | Fila compacta de modelo/contexto/estado asociada a la entrada |
| Menú de archivos | `src/ui.rs` (`draw_file_menu`), `src/file_ref.rs` | Sin demo dedicada | Pendiente | Distinguir lista de ficheros, selección, carpeta y empty state |
| Diálogo de permiso/approval | `src/ui.rs` (`draw_permission_ask`, `draw_cordis_approval`) | Demo general de permiso, sin variante Martty | Parcial | Falta reproducir 1:1 el prompt y sus acciones |
| Bandeja de cola | `src/ui.rs` (`draw_queue_shelf`) | Sin demo dedicada | Pendiente | Fila compacta de prompts en espera |
| Overlay de selección / prompt flash | `src/ui.rs` (`draw_selection_overlay`, `draw_prompt_flash`) | Sin demo dedicada | Pendiente | Selección de texto y aviso temporal |
| Selector de vista | `src/ui.rs` (`draw_view_overlay`) | Sin demo dedicada | Pendiente | Overlay de visualización de archivos/resultados |
| Navegación lateral / agent rail | `src/ui.rs` (`draw_navigation_dock`, `draw_agent_rail`) | `widgets/sidebar` genérico | Parcial | Falta variante Martty y estado activo/compacto |
| Paneles de plugin y custom slots | `src/ui.rs` (`draw_plugin_tree`, `draw_plugin_slider`, `draw_plugin_select`, `draw_right_slot`), `src/slots.rs` | Sin demo de plugin Martty | Pendiente | Superficie avanzada; documentar como familia independiente, no átomo |

## Mindfork

Referencia fijada: `vshylov/mindfork-rs`, snapshot `5f18fceb0cdcfb4052583d26902b7e60c0fce1e4` (MIT).

| Componente de referencia | Fuente upstream | Batten / demo | Importación | Comparación visual / notas |
|---|---|---|---|---|
| Chat: composición base | `src/screens/chat/render.rs`, `src/widgets/message_feed.rs` | `widgets/*::render_mindfork` · `Mindfork · chat` | Parcial | Recreación desde screenshot; falta validar el layout completo en terminal |
| Composer | `src/screens/chat/input.rs` | `widgets/composer::render_mindfork` · `Mindfork · composer` | Importado | Captura comparativa pendiente |
| Barra de estado | `src/widgets/status_bar.rs` | `widgets/status_bar::render_mindfork` · `Mindfork · status bar` | Importado | Captura comparativa pendiente |
| Lista de conversaciones | `src/screens/chat/*`, captura `chat-list-dark-en.png` | `widgets/session_list::render_mindfork` · `Mindfork · chat list` | Importado | Captura comparativa pendiente, incluyendo resize |
| Thinking y tool calls plegables | `src/widgets/message_feed.rs`, `src/screens/chat/feed.rs` | `widgets/thinking::render_mindfork`, `widgets/tool_item::render_mindfork` · demos Mindfork | Importado | Captura comparativa pendiente |
| Settings / Tools | `src/screens/settings/` | `widgets/settings_panel::render_mindfork_tools` · `Mindfork · settings/tools` | Importado | Captura comparativa pendiente |
| Settings / Model-server | `src/screens/settings/` | `widgets/settings_panel::render_mindfork_model` · `Mindfork · settings/model` | Importado | Captura comparativa pendiente |
| Self-model (perfil, goals y observations) | `src/screens/self_model/`, `src/widgets/profile_list.rs` | `widgets/profile_view::render_mindfork` · `Mindfork · self-model` | Importado | Captura comparativa pendiente |
| Variantes light | `src/shared/theme.rs` y capturas light publicadas | `widgets/settings_panel::apply_mindfork_light` · `Mindfork · light components` | Parcial | Cinco superficies adaptadas; comparar color, contraste y estados en captura |
| Popups y comandos del chat | `src/screens/chat/popups.rs`, `src/screens/chat/commands.rs` | Sin demos específicas | Pendiente | Inventariar cada popup visible antes de elegir componentes |
| Selectores de proyecto, RAG e impersonation | `src/screens/chat/project.rs`, `src/screens/chat/rag.rs`, `src/screens/chat/impersonation.rs` | Sin demos específicas | Pendiente | Revisar si son superficies estables y reutilizables |
| Selector de emoji | `src/widgets/emoji_picker.rs` | Sin demo dedicada | Pendiente | Popup de búsqueda/selección; pieza autónoma |
| Selector de enlace a chat | `src/widgets/chat_link_picker.rs` | Sin demo dedicada | Pendiente | Popup y resultado seleccionable |
| Selector de servidor local | `src/widgets/local_server_picker.rs`, `src/screens/settings/picker.rs` | La demo de settings/model-server cubre solo la pantalla principal | Parcial | Falta el popup de selección como componente pequeño |
| Vista previa de impersonation | `src/widgets/impersonation_preview.rs` | Sin demo dedicada | Pendiente | Vista previa independiente del flujo de chat |
| Diálogo de ayuda y tira de pestañas | `src/widgets/help_dialog.rs` (`render_help`, `help_tab_strip`) | `widgets::help_overlay::render_mindfork_tabs` · `Mindfork · help tabs`; sin demo del diálogo completo | Parcial | Tira de seis tabs, selección y overflow añadidos; falta diálogo y contenido de cada tab |
| Logo/glyph, wordmark y lockup de Mindfork | `src/widgets/logo.rs` (`logo_lines`, `wordmark_lines`, `lockup_lines`) | `widgets/logo::{mindfork_glyph,mindfork_wordmark,mindfork_lockup}` · `Mindfork · glyph, wordmark y lockup` | Importado | Las tres formas se muestran sobre fondo dark/light; falta cotejo por captura frente a las variantes SVG oficiales |
| Adjuntos e imágenes inline | `src/screens/chat/attachments.rs`, `src/screens/chat/images.rs` | Sin demo Mindfork dedicada | Pendiente | Faltan el preview/listado de adjuntos y la presentación inline de imagen |
| Selector de perfil | `src/widgets/profile_list.rs` | El self-model está representado, pero el selector popup no | Parcial | Añadir el listado compacto con foco/selección |
| Campo editor popup | `src/screens/settings/render.rs` (`render_editor_popup`) | Sin demo dedicada | Pendiente | Overlay de edición de valor/textarea |
| Selector de opción | `src/screens/settings/render.rs` (`render_choice`) | Sin demo dedicada | Pendiente | Lista con marca de selección y highlight |
| Picker de settings | `src/screens/settings/render.rs` (`render_picker`) | La pantalla settings está representada | Parcial | Añadir popup de selección y filas |
| Search de settings | `src/screens/settings/render.rs` (`render_search`) | Sin demo dedicada | Pendiente | Input/filtro y lista de resultados |
| Composer tags / modo comando | `src/widgets/input_box.rs` (`render_tag`, `render_single_line`) | `widgets/composer::render_mindfork` | Parcial | El composer grande existe; falta aislar tags, slash mode y formato de línea |
| Fila de preview de impersonation | `src/widgets/impersonation_preview.rs` | Sin demo dedicada | Pendiente | Preview compacto insertado en el feed |
| Microchips y glyphs de estado | `src/widgets/status_bar.rs` (`state_chips`, `chat_chip`, `secondary_chip`, `status_glyph`) | Cubiertos solo dentro de `Mindfork · status bar` | Parcial | Falta demo por estados: conectado, ocupado, desconectado y secundarios |
| Keycaps y grupos de atajos | `src/widgets/status_bar.rs` (`hotkey_list`) | `widgets/key_help` genérico | Parcial | Existe la primitiva general; falta variante visual Mindfork y navegación por ancho |
| Rails/marcadores por rol de mensaje | `src/widgets/message_feed.rs` (`role_header`, `rail_of`, `prepend_rail`) | Agrupados en `Mindfork · chat` | Parcial | Falta demo compacta de rail/encabezado para comparar roles |

## Pawan

Referencia fijada: `dirmacs/pawan`, snapshot `e15a5d22f1d06b1a1643c3d9757816d0af0ea08b` (MIT).

| Componente de referencia | Fuente upstream | Batten / demo | Importación | Comparación visual / notas |
|---|---|---|---|---|
| Welcome | `crates/pawan-cli/src/tui/render/` y snapshot `render_welcome_snapshot.snap` | `widgets/welcome::render_pawan` · `Pawan · welcome` | Importado | Recreación del snapshot 80×24; falta captura de terminal |
| Model picker | `crates/pawan-cli/src/tui/render/overlays.rs` y snapshot de model selector | `widgets/model_picker::render_pawan` · `Pawan · model picker` | Importado | Snapshot de referencia 80×24; captura comparativa pendiente |
| Fuzzy search | `crates/pawan-cli/src/tui/fuzzy_search.rs`, `tui/render/overlays.rs` | `widgets/fuzzy_search::render_pawan` · `Pawan · fuzzy search` | Importado | Snapshot 80×24; captura comparativa pendiente |
| Mensajes, roles y estado de espera | `crates/pawan-cli/src/tui/render/messages.rs` | `widgets/transcript::render_pawan` / `render_pawan_waiting` · `Pawan · messages` | Importado | Incluye roles, tool/error, cursor y spinner; captura comparativa pendiente |
| Identidad del diálogo TUI | `crates/pawan-cli/src/tui/render/overlays.rs` (`title(" पवन — pawan ")`) | No hay logo gráfico; el título textual aparece en la UI | Importado como texto | El SVG `docs/static/pawan-logo.svg` se usa en README/documentación, no en el TUI observado; no se cuenta como componente visual de terminal |
| Ayuda | `crates/pawan-cli/src/tui/render/overlays.rs` y snapshot help | `widgets/help_overlay::render_pawan` · `Pawan · help` | Importado | Snapshot 80×24; captura comparativa pendiente |
| Diálogo de permiso | `crates/pawan-cli/src/tui/render/overlays.rs` y snapshot permission | `widgets/permission::render_pawan` · `Pawan · permission` | Importado | Geometría basada en snapshot; captura comparativa pendiente |
| Tool call plegada / expandida | `crates/pawan-cli/src/tui/tool_display.rs`, `tui/render/messages.rs` | `widgets/tool_item::render_pawan` · `Pawan · tool call` | Importado | Estados de ejecución/salida/plegado; captura comparativa pendiente |
| Barra de estado | `crates/pawan-cli/src/tui/status_bar.rs` | `widgets/status_bar::render_pawan` · `Pawan · status bar` | Importado | Captura comparativa pendiente |
| Badge de rol y marca temporal | `crates/pawan-cli/src/tui/render/messages.rs` (`render_message_to_lines`) | Agrupados en `Pawan · messages` | Parcial | Falta demo de badges user/assistant/system/tool/error aislados |
| Barra de uso de contexto | `crates/pawan-cli/src/tui/status_bar.rs` (`context_bar`) | Agrupada en `Pawan · status bar` | Parcial | Falta ejemplo pequeño con 0/50/100% y truncado |
| Símbolo y resumen de tool | `crates/pawan-cli/src/tui/tool_display.rs` | Agrupados en `Pawan · tool call` | Parcial | Falta separar estados ✓/✗, duración y preview de una/dos líneas |
| Separadores y elipsis de ancho | `crates/pawan-cli/src/tui/status_bar.rs`, `tui/render/messages.rs` | Hay primitivas genéricas de texto/regla | Parcial | No existe demo por referencia de los separadores/truncados compactos |
| Resaltado de código y fragmentos | `crates/pawan-cli/src/tui/highlight.rs`, `tui/render/messages.rs` | `widgets/ansi` es render ANSI genérico | Parcial | Falta comparar colores/estilos de código de Pawan |
| Composer y editor de entrada | `crates/pawan-cli/src/tui/input.rs` | Sin demo Pawan dedicada | Pendiente | El composer genérico y los de otras referencias no acreditan paridad Pawan |
| Browser de sesiones | `crates/pawan-cli/src/tui/session_panel.rs` (`render_session_browser`) | Sin demo específica | Pendiente | Modal de búsqueda y sesiones seleccionadas; lectura/persistencia desde `Session`, con filas reproducibles usando fixtures |
| Cola de tareas/subagentes | `crates/pawan-cli/src/tui/queue_panel.rs` (`QueuePanel::view`) | `widgets::queue::render_pawan` · `Pawan · task queue` | Importado | Cuatro estados, wrap por ancho y altura hasta tres líneas; comparar captura a varios anchos |
| Composer de IRC | `crates/pawan-cli/src/tui/irc_panel.rs` (`render_irc_compose`) | Sin demo específica | Pendiente | Modal con instrucción, destino y mensaje; envío acoplado a `App`, dibujo reproducible con fixture |
| Menús de slash commands | `crates/pawan-cli/src/tui/slash_commands.rs`, `slash_handlers.rs` | Sin demo Pawan dedicada | Pendiente | Revisar interacción y apariencia de comandos |

## Crush

Referencia fijada: checkout local `charmbracelet/crush`, snapshot
`852f410573e199fa0235cd8a8f32d6b0840373d3`. `LICENSE.md` declara Functional
Source License 1.1 con licencia MIT futura al segundo aniversario. No se ha
copiado código ni assets de Crush en Batten; las demos reproducen estructura y
datos de ejemplo. Su licencia es más restrictiva que MIT para redistribuir
derivados antes de la fecha de conversión, así que cualquier copia futura de
código o assets requiere revisar sus términos.

El barrido de superficies, overlays, estados de transcript, familias de tools
y átomos está en [`crush-component-inventory.md`](crush-component-inventory.md).
Ese archivo es la lista fuente completa de trabajo para las siguientes
actualizaciones. Su sección “Correspondencia demo por demo con el código de
Crush” enlaza cada entrada del índice del showcase con el módulo Batten y el
renderer/tipo/método de Crush que representa. La tabla breve de abajo muestra
las familias principales.

| Componente de referencia | Fuente | Batten / demo | Importación | Comparación visual / notas |
|---|---|---|---|---|
| Sidebar de contexto integrado | `internal/ui/model/sidebar.go` (`updateSidebarScrollState`, `drawSidebar`) | `Context sidebar · Crush`; integrado con feed y composer en `Crush · sidebar + chat` | Parcial | Ahora combina logo compacto real, título, ruta/rama, modelo/contexto, archivos modificados, LSP, MCP y Skills junto al transcript/composer; faltan scroll/foco y datos reales. Captura comparativa pendiente |
| Chat y transcript | `internal/ui/model/chat.go`, `internal/ui/chat/` | `Crush · sidebar + chat`, `Transcript · Crush` | Parcial | Sidebar y composer quedan en la misma composición; feed estático. Faltan viewport, selección, auto-follow y streaming |
| Mensajes, estados y handoff | `internal/ui/chat/{assistant,user,pending,question,messages}.go` | `Assistant message`, `User message`, `Plan card`, `Inline question · Crush` | Parcial | Incluye thinking pendiente, error/rechazo, plan, pregunta y adjunto de usuario; falta interacción/runtime completa |
| Landing / welcome | `internal/ui/model/landing.go` | `Welcome · Crush · landing` | Parcial | Composición de marca, proyecto, modelo y recursos; falta cotejar dimensiones y empty states |
| Onboarding / inicialización | `internal/ui/model/onboarding.go` | `Onboarding · Crush · initialize` | Parcial | Texto, ruta y botones; falta flujo de inicialización |
| Detalles compactos y scroll de sidebar | `internal/ui/model/ui.go`, `sidebar.go` | `Session details · Crush`, `Sidebar scroll · Crush` | Parcial | Overlay, ventana y thumb simulados; falta responsive, scroll por contenido y foco |
| Inline editor, help y salida | `internal/ui/dialog/inline_editor.go`, `model/status.go`, `ui/exitbanner/` | `Inline editor`, `Help`, `Exit banner · Crush` | Parcial | Muestras estáticas; falta confirmar comportamiento con sesiones reales |
| Layout compacto: cabecera en lugar de sidebar | `internal/ui/model/ui.go` (compact layout), `internal/ui/model/header.go` | `Compact header · Crush` | Parcial | Header y detalles simulados; faltan umbrales responsive y comparar con layout ancho |
| Composer / textarea dinámico | `internal/ui/model/ui.go` (`textarea.Model`) | `Composer · Crush` | Parcial | Prompt sin marco y contenido multilinea; faltan crecer 3–15 filas, modos y adjuntos |
| Landing y recursos | `internal/ui/model/landing.go` | `Welcome · Crush · landing` | Parcial | Ruta, bloque de modelo y columnas LSP/MCP/Skills; falta comparación de medidas y empty states |
| Logotipos | `internal/ui/logo/logo.go` (`Render`, `SmallRender`); `internal/ui/model/sidebar.go` (`updateSidebarScrollState`) | `Logo · Crush · wide title art`, `Logo · Crush · sidebar`, `Logo · Crush · sidebar fallback` | Referencia visual | Distingue el arte ancho, el logo que el sidebar usa tras iniciar la sesión (`Render` compacto) y el fallback `SmallRender` cuando el panel es bajo; el logo ancho muestra una muestra del estiramiento aleatorio |
| Thinking y mensaje del asistente | `internal/ui/chat/assistant.go` | `Thinking · Crush` | Parcial | Caja de thinking y plegado; faltan mensaje Markdown, error, plan y estados streaming |
| Tool calls y resultados | `internal/ui/chat/tools.go`, `file.go`, `shell.go` | `Tool call · Crush · generic states` | Parcial | Cubre estados genéricos y salida plegable; faltan renderers específicos de archivo/shell/search/MCP/etc. |
| Tool renderers por familia | `internal/ui/chat/{bash,file,search,fetch,agent,mcp,diagnostics,todos,generic}.go` | `Tool renderer · Crush · {shell/files/search/web/agents/MCP/diagnostics/todos/generic}` | Parcial | Añadidas nueve muestras de familia con estados y salida; faltan variantes por tool y estado completas |
| Diff y navegación de código | `internal/ui/chat/unified_diff.go`, `diffview/`, `chat/{definition,references,call_hierarchy,symbols,rename,replace_symbol}.go` | `Diff output · Crush`, `Code navigation tools · Crush` | Parcial | Diff scrollable y lista de acciones; falta formato específico de cada tool y vista split |
| Thinking / reasoning | `internal/ui/chat/assistant.go` | `Thinking · Crush` | Parcial | Caja, estado y cola plegable; falta cotejar Markdown/truncado/duración por captura |
| Help/status footer | `internal/ui/model/status.go` | `Status bar · Crush · help/footer` | Parcial | Muestra badges plan/YOLO/router y hints; falta comparar la adaptación por ancho y modo expandido |
| Commands y file picker | `internal/ui/dialog/commands.go`, `filepicker.go` | `Command menu · Crush`, `File picker · Crush` | Parcial | Listas y selección ilustradas; falta búsqueda, iconografía y casos vacíos |
| Sessions | `internal/ui/dialog/sessions.go` | `Conversation list · Crush · sessions` | Parcial | Selector básico; faltan metadata, filtro y acciones |
| Model picker | `internal/ui/dialog/models.go`, `models_list.go` | `Model picker · Crush` | Parcial | Variantes listadas; faltan agrupación, búsqueda, reasoning y acciones |
| Permission dialog | `internal/ui/dialog/permissions.go` | `Permission · Crush` | Parcial | Tool, comando y acciones; falta el layout real por tipo/diff |
| Question dialog | `internal/ui/dialog/question_*.go` | `Question dialog · Crush` | Parcial | Vista de selección única; faltan formularios y selección múltiple |
| Autocomplete, adjuntos y pills | `internal/ui/completions/`, `attachments/`, `model/pills.go` | `Autocomplete`, `Attachment chips`, `Todo / queue pills · Crush` | Parcial | Variantes estáticas e interacción básica; falta resize, foco real y comparación de captura |
| Provider / MCP / router / theme / notifications dialogs | `internal/ui/dialog/{auth_method,mcp_toggles,router,theme,notifications}.go` | `Settings · Crush · {providers,MCP,router,notifications,themes}` | Parcial | Estados listados; falta flujo de edición/autenticación real y captura del overlay |
| Plan handoff y quit | `internal/ui/dialog/{plan_handoff_inline,quit}.go` | `Plan handoff · Crush`, `Confirmation · Crush · quit` | Parcial | Opciones estáticas; faltan transición y comportamiento inline |
| Tema Charmtone | `internal/ui/styles/themes.go`, `palette.go`, dependencia `charmtone` | Preset `ThemePreset::Crush` | Parcial | Tokens de color mapeados; geometría, fondos por bloque y estados aún no comparados celda a celda |

## Dotmax

Referencia: [`newjordan/dotmax`](https://github.com/newjordan/dotmax), crate
`dotmax 0.1.11` (MIT OR Apache-2.0), solo como dependencia de desarrollo para
las demos. Se usa `BrailleGrid` y su API pública; no se copian fuentes/assets.

| Familia de Dotmax | Código de referencia | Showcase Batten | Estado / límite |
|---|---|---|---|
| Catálogo de progress/loading | `progress::{all_styles,styles_for_theme,ProgressStyle}`; `progress/styles/` | `Progress catalog · dotmax · 60 temas` — navegar tema con ←/→ y estilo con ↑/↓ | Navega los 692 estilos registrados y sus 60 temas; salida de cada estilo adaptada a las celdas Ratatui |
| Spinner | `progress/styles/spinner.rs` | Misma demo; comienza en el tema `spinner` | 14 estilos de spinner. El resto del catálogo incluye barras, wipes, bordes y medidores, además de visualizaciones animadas |
| BrailleGrid y primitivas geométricas | `grid.rs`, `primitives/{circle,line,shapes}.rs` | `Braille graphics · dotmax · visual lab` → `braille primitives` | Círculo, línea, rectángulo y polígono renderizados dentro del frame Ratatui |
| Rampas de densidad | `density::DensitySet`, `BrailleGrid::render_density` | `Braille graphics · dotmax · visual lab` → `density ramps` | ASCII (69 niveles), simple, bloques y Braille; demo de intensidad sintética, no de imagen |
| Paletas y esquemas RGB | `color/schemes.rs`, `color/apply.rs`, `ColorScheme` | `Braille graphics · dotmax · visual lab` → `color schemes` | Siete gradientes built-in; adaptación a spans RGB Ratatui |
| Animación y timing | `animation/{frame_buffer,differential,loop_helper,prerender,timing}.rs` | `Progress catalog` y `visual lab` usan el reloj del showcase | Se ven estilos temporales; el showcase deja a Ratatui gestionar el diff de terminal y no inicia un segundo `TerminalRenderer` |
| Captura/exportación de frames | `frame_export.rs` (`capture_frame`, `write_frame_pack`) | Solo enumerado en `capability map` | API para paquetes JSON; no hay flujo de exportación interactivo |
| Imágenes y SVG | `image/`, `media/`; Cargo features `image`, `svg` | Solo enumerado en `capability map` | No habilitado en la dependencia Batten; image incluye formatos/raster y animaciones GIF/APNG, SVG añade rasterizado vectorial |
| Vídeo y webcam | `media/`; Cargo feature `video` y renderer de terminal | Solo enumerado en `capability map` | Requiere FFmpeg; webcam requiere además cámara. No se inicia desde la TUI Ratatui |
| Renderer / detección de terminal | `render.rs`, `utils/terminal_caps.rs`, `quick.rs` | Solo enumerado en `capability map` | Dotmax puede tomar control del terminal; aquí se reutiliza la rejilla y Ratatui conserva el ciclo de vida de la TUI |
| Raytracer y ajedrez | `raytracer/`, `chess/`; features Cargo `raytracer`, `chess` | Solo enumerado en `capability map` | Módulos opcionales; no son familias de componentes para el chat |

El catálogo cubre de forma visual las capacidades compatibles con una celda
Ratatui y muestra las demás en el mapa. “Enumerado” no significa que imágenes,
vídeo, webcam, exportación o `TerminalRenderer` estén activos o integrados.

## Qué tan separables son los componentes en el código fuente

La visibilidad de una pieza no garantiza que exista como widget autónomo. Este
mapa registra dónde se puede extraer su forma visual con datos fijos y dónde
primero habría que separar la vista del estado/runtime.

| Repo | Acoplamiento observado | Piezas visuales fáciles de aislar | Piezas que requieren adaptar estado o separar renderer |
|---|---|---|---|
| Mindfork | Bajo a medio. `src/widgets/` tiene módulos explícitos. `status_bar.rs`, `logo.rs`, `help_dialog.rs` y varios pickers exponen funciones/renderers; `MessageFeed` y `InputBox` mezclan layout, estado de scroll/edición y semántica del mensaje. | Glyphs/chips/atajos de status; tira de tabs de Help; selector de emoji/chat-link/server; filas de perfil; opción y picker de Settings. | Feed completo, composer con tags/cursor, adjuntos inline y preview de impersonation. Se pueden presentar con datos de fixture sin incorporar el runtime Mindfork. |
| Martty | Alto en la pantalla principal: `src/ui.rs` es un renderer grande de métodos `draw_*` sobre `App`. Hay fronteras útiles en `src/transcript.rs`, `src/slots.rs`, `src/harness_badge.rs`, `src/file_ref.rs` y `src/input/`. | `Transcript`/celdas, badge de harness, tags `@file`, nodo de slot y las formas de logo. | Dock, navegación, approval, bandeja, overlays y menús consultan estado de `App`; un demo fiel necesita extraer datos de vista pequeños o construir fixtures para el método. |
| Pawan | Medio. `status_bar.rs`, `tool_display.rs` y `queue_panel.rs` tienen modelos/contexto propios; la UI principal, buscador de sesiones y composición IRC son métodos `App` en `render/`, `session_panel.rs` e `irc_panel.rs`. | Chips/contexto de status, pieza de tool formateada, fila/panel de cola. | Browser de sesiones, composer y modal IRC consultan almacenamiento/estado de `App`; el aspecto se puede reproducir con filas y texto de ejemplo sin sus eventos ni persistencia. |

Conclusión: no están todos demasiado acoplados para estudiarlos. Mindfork y
Pawan permiten aislar muchos átomos directamente. En Martty hay que leer cada
renderer junto con su estado, pero varias fronteras ya existen y las demás se
pueden describir con fixtures sin portar la app completa. El criterio para el
showcase es copiar la estructura visual y sus estados; no traer su runtime.

## Átomos visuales: cobertura cruzada

Esta capa registra piezas que aparecen incrustadas en componentes mayores. Una
primitiva genérica de Batten cubre comportamiento reutilizable, pero solo se
marca como reproducción de una referencia cuando tiene su estilo y estados
propios en una demo.

| Átomo | Referencias y fuente | Cobertura en Batten | Siguiente paso |
|---|---|---|---|
| Chip / badge | Mindfork `status_bar.rs` (`chip`, `chat_chip`, `secondary_chip`); Pawan `status_bar.rs` (mode/GOAL/LOOP) y `render/messages.rs` (roles); Martty `harness_badge.rs` | `widgets/chip` es genérico; badges aparecen dentro de demos Mindfork/Pawan y el selector Martty | Hacer demos de variantes por app y estados, incluyendo anchura corta |
| Keycap y ayuda inline | Mindfork `status_bar.rs` (`hotkey_list`); Martty `ui.rs` (footer/overlays); Pawan `render/messages.rs` | `widgets/key_help` existe y tiene demo genérica; `Pawan · help` muestra diálogo, no átomo inline | Añadir variantes con keycaps reales y prioridad/recorte de cada referencia |
| Indicador de actividad/resultado | Mindfork `status_bar.rs` (`status_glyph`, `state_chips`); Pawan `tool_display.rs`; Martty `transcript.rs`; dotmax `progress::styles::spinner` | `Progress catalog · dotmax · 60 temas` contiene también los 14 estilos `spinner`; Batten conserva sus variantes de una celda | Los spinners Braille de varias celdas no sustituyen el glifo compacto de estado; falta decidir cuáles encajan en tools/chat |
| Barra de contexto / token counter | Pawan `status_bar.rs` (`context_bar`, `format_tokens`); Mindfork `status_bar.rs` (`token_counter_span`) | Visible dentro de `Pawan · status bar` y `Mindfork · status bar` | Falta demo compacta para comparar 0/50/100%, estimaciones y recorte; hoy la tabla solo menciona la barra de progreso Pawan |
| Fila de cola / task pill | Pawan `queue_panel.rs` (`QueuePanel`, `format_piece`) | `Pawan · task queue` | Importada | Muestra pending/running/done/failed y wrap; queda comparar captura y truncado al estrechar |
| Tab strip con overflow | Mindfork `help_dialog.rs` (`help_tab_strip`, `tab_window`) | `Mindfork · help tabs`; `widgets/key_help` es otra superficie | Muestra seis tabs, selección por ←/→ y overflow `…` a 32 columnas; diálogo y contenido siguen pendientes |
| Píldora de modo/estado | Pawan `status_bar.rs` (mode, GOAL, LOOP); Mindfork `status_bar.rs` (`chip`, `chat_chip`, `secondary_chip`) | Cubierta dentro de las dos barras de estado, no aislada | Comparar semántica, colores, padding y comportamiento estrecho en una familia pequeña de chips |
| Sesión seleccionable | Pawan `session_panel.rs` (`render_session_browser`); Mindfork `chat_list.rs` (`Row`) | `Mindfork · chat list`; sin variante Pawan | El row Pawan depende de `SessionSummary`; demo puede fijar id/model/count y separar la fila de búsqueda/browser |
| Modal de composición IRC | Pawan `irc_panel.rs` (`render_irc_compose`) | Sin demo Pawan | Vista pequeña y separable con título, hint `<peer> <message>` e input; la acción de envío sí depende de `App` |
| Rail o borde semántico | Mindfork `message_feed.rs` (`rail_of`, `prepend_rail`); Martty `transcript.rs`; Pawan `render/messages.rs` | `widgets/rule` es genérico; rails se ven en demos de transcript/chat/tool | Comparar grosor, sangría, continuidad y color en líneas multilínea |
| Barra de progreso compacta | Pawan `status_bar.rs` (`context_bar`) | `widgets/progress` genérico, sin equivalente Pawan aislado | Reproducir bloques usados por Pawan y su truncado |
| Mención/token inline | Martty `file_ref.rs`; Mindfork `input_box.rs` (`render_tag`) | Hay `widgets/links`; no hay demo de token coloreado dentro de composer | Añadir muestras @file, tags y cursor alrededor del token |
| Iconos/glyphs de marca | Batten `widgets/logo.rs`; Crush `internal/ui/logo/logo.go`; Martty `src/logo.rs`, `deepseek_logo.rs`, `harness_badge.rs`; Mindfork `src/widgets/logo.rs`; Pawan README SVG y título del TUI | `Logo · Batten`, `Logo · Crush · wide title art/sidebar/sidebar fallback`, `Logo · Martty · whale + wordmark`, `Logo · Mindfork · glyph, wordmark y lockup`; también `Welcome · Martty` | Batten: wordmark editable, degradado y fallback compacto. Crush: arte ancho con letra estirada, sidebar y fallback `SmallRender` (solo referencia UX; no apto para copiar/publicar sin revisión de licencia y marca). Martty: solo identidad Martty; falta el segundo logo responsivo DeepSeek Harness. Mindfork: glyph, wordmark y composición horizontal; falta comparar capturas. Pawan: el SVG solo aparece en README, su TUI usa el nombre en texto |
| Divisor/espaciado | Mindfork `status_bar.rs`; Pawan `status_bar.rs`; Martty `ui.rs` | `widgets/rule` y layout Ratatui ya disponibles; sin demos con las separaciones exactas de cada app | Registrar secuencias, márgenes y fallback en terminal estrecho |

### Primitivas base y exclusivas de Batten

`widgets/button` no tenía demo. Ya aparece como `Batten · primitivas
exclusivas / Button`, con selección por ←/→ y tonos de permitir, preguntar y
denegar. Las piezas genéricas con analogías aproximadas tienen familias propias
en el índice (chip/badge, divider/rail, key help, progress/context bar, panel,
activity spinner, code output, links/mention, profile/self-model y navigation
rail). Mientras solo esté la demo Batten, el inventario indica qué variante de
referencia falta. File changes, button, toast, effects y themes quedan bajo
`Batten · primitivas exclusivas`, sin equivalente visual directo identificado
en estas referencias.

La navegación del showcase se agrupa ahora por familia funcional: Composer,
Chat, Transcript, Tool call, Thinking, Status bar, Welcome, Logo, Model picker,
Permission, Search, Help, Conversation list, Settings, Harness picker,
Attachment preview, Command menu, chip/badge, divider/rail, key help,
progress/context bar, panel, activity spinner, code output, links/mention,
profile/self-model, navigation rail, Context sidebar y `Batten · primitivas exclusivas`. En cada
grupo, las variantes implementadas de Batten, Mindfork, Martty, Pawan y Crush aparecen
consecutivas. Lo que aún no existe como demo se mantiene en las filas
Pendiente/Parcial de este inventario.
En las familias atómicas con analogía, la variante Batten queda visible aunque
la variante de la app aún esté pendiente; así el hueco se ve dentro de la misma
familia y se puede añadir al lado cuando se implemente.

### Cobertura del barrido de fuentes

- **Martty:** se contrastaron los renderers visibles en `src/ui.rs`, el modelo y
  formato de transcript, `file_ref.rs`, `harness_badge.rs`, `slots.rs`, `logo.rs`
  y el widget de composer. Renderers aún no reflejados por una demo propia
  quedaron listados como pendientes o parciales en la tabla (approval, cola,
  overlays, navegación, slots y menú `@file`).
- **Mindfork:** se listaron todos los módulos bajo `src/widgets/`, los renderers
  del chat y los popups/editor de settings. Los nuevos popups y átomos detectados
  (logo, emoji, chat-link, local-server, perfil, ayuda, chips y rails) ya constan
  como filas separadas.
- **Pawan:** se contrastaron `src/tui/`, `src/tui/render/`, `status_bar.rs`,
  `tool_display.rs`, `queue_panel.rs`, `session_panel.rs`, `irc_panel.rs` y los
  snapshots de overlays. La UI utilitaria (`events`, `app`, `types`) no se cuenta
  como componente visual salvo que produzca una superficie identificable. La
  cola es un widget autónomo; el browser y el modal IRC son renderers visuales
  acoplados a `App`, pero se pueden reproducir con fixtures.
- **Crush:** snapshot exacto `852f410573e199fa0235cd8a8f32d6b0840373d3`;
  se revisaron `internal/ui/model/{sidebar,ui,landing,header}.go`, renderers de
  chat y estilos. La demo nueva cubre la composición del sidebar como una
  referencia inicial; los estados interactivos y las demás superficies siguen
  en la tabla como pendientes. `LICENSE.md` declara FSL-1.1-MIT; no se ha
  reutilizado código ni assets.

## Cobertura y mantenimiento

- El catálogo del showcase registra las demos en
  `crates/batten/examples/showcase/demos/mod.rs`. Al añadir o retirar una fila,
  actualizar también ese catálogo y los widgets correspondientes.
- Los estados indican cobertura en Batten, no calidad de paridad. Para cerrar la
  comparación: generar captura del upstream y del showcase a idéntico tamaño,
  revisar geometría, caracteres, color y recorte, y anotar aquí la diferencia
  concreta o el checkpoint de captura.
- Antes de cada actualización, registrar el nuevo `HEAD` de cada checkout y
  revisar el diff upstream de las rutas listadas. Si cambia el layout o aparecen
  superficies nuevas, actualizar la tabla antes de modificar el widget. No
  reemplazar el SHA de referencia hasta revisar qué cambió.
- Capturas actuales de comparación: **pendientes**. Por tanto, “Importado” no
  significa “copiado 1:1 verificado”.

## Historial de snapshots

| Repositorio | SHA analizado | Licencia declarada | Revisado |
|---|---|---|---|
| [Martty](https://github.com/openma-ai/Martty) | `bf66b4a3dfbcebbfe1d91e76c5af39d63f921337` | MIT | 2026-10-18 |
| [Mindfork](https://github.com/vshylov/mindfork-rs) | `5f18fceb0cdcfb4052583d26902b7e60c0fce1e4` | MIT | 2026-10-18 |
| [Pawan](https://github.com/dirmacs/pawan) | `e15a5d22f1d06b1a1643c3d9757816d0af0ea08b` | MIT | 2026-10-18 |
| [Crush](https://github.com/charmbracelet/crush) | `852f410573e199fa0235cd8a8f32d6b0840373d3` | FSL-1.1-MIT (MIT futura al segundo aniversario) | 2026-10-18 |
