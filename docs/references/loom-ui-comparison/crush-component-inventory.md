# Inventario visual de Crush

Barrido estático del checkout local `E:\workspace\personal\crush`,
`HEAD=852f410573e199fa0235cd8a8f32d6b0840373d3`, revisado el 2026-10-18.
El alcance es cada superficie y renderer visual de `internal/ui/`, no las
funciones de agente, configuración o transporte que no producen UI. Las pruebas
de layout y snapshots sirven para ubicar estados y límites, pero todavía no se
hizo comparación de capturas con Batten.

**Estados Batten:** `Parcial` = hay demo Crush pero faltan estados/medidas;
`Pendiente` = aún no hay variante Crush dedicada. Una primitiva Batten con
aspecto parecido no equivale a importar el componente de Crush. El preset
Crush existente aporta colores Charmtone, no su layout.

## Correspondencia demo por demo con el código de Crush

Las etiquetas de la primera columna son las que aparecen en el índice del
showcase (familia · variante). Las rutas son relativas a la raíz del checkout
de Crush indicado arriba. En la última columna se nombra el tipo o método que
produce la superficie cuando se pudo identificar; las filas marcadas
**composición** combinan varios renderers y no representan un widget aislado.
Esto documenta la referencia visual, no una dependencia ni código importado.

| Entrada en showcase | Implementación Batten | Crush: archivo y renderer/componente correspondiente |
|---|---|---|
| Composer · Crush | `demos/crush_composer.rs`; `widgets/composer.rs::render_crush` | `internal/ui/model/ui.go` — `(*UI).renderEditorView`; editor Bubble `textarea.Model` |
| Chat · Crush full layout | `demos/crush_chat.rs` (`new(true)`) | **Composición:** `internal/ui/model/ui.go` — `(*UI).Draw`; `model/sidebar.go` — `(*UI).drawSidebar`; `model/chat.go` — `(*Chat).Draw`; `model/status.go` — `(*Status).Draw` |
| Transcript · Crush | `demos/crush_chat.rs` (`new(false)`) | `internal/ui/model/chat.go` — `(*Chat).Draw`; items de `internal/ui/chat/` montados en `internal/ui/list/` |
| Assistant message · Crush pending | `demos/crush_message_states.rs` — `Pending` | `internal/ui/chat/pending.go` — `PendingAssistantItem.Render`; animación en `internal/ui/anim/` |
| Plan card · Crush | `demos/crush_message_states.rs` — `Plan` | `internal/ui/chat/assistant.go` — `AssistantMessageItem.Render` y `renderPlanBox` |
| Assistant message · Crush refusal | `demos/crush_message_states.rs` — `Refusal` | `internal/ui/chat/assistant.go` — `AssistantMessageItem.Render` (sección de error/rechazo) |
| Inline question · Crush | `demos/crush_message_states.rs` — `Question` | `internal/ui/chat/question.go` — `QuestionToolRenderContext.RenderTool`; diálogos completos en `internal/ui/dialog/question_*.go` |
| Assistant message · Crush summary | `demos/crush_message_states.rs` — `Summary` | `internal/ui/chat/messages.go` — `AssistantInfoItem.Render` |
| User message · Crush attachments | `demos/crush_message_states.rs` — `UserAttachment` | `internal/ui/chat/user.go` — `UserMessageItem.Render`; chips en `internal/ui/attachments/attachments.go` — `Renderer.Render` |
| Tool call · Crush generic states | `demos/crush_tool_call.rs` | `internal/ui/chat/tools.go` — `NewToolMessageItem`, `baseToolMessageItem.Render` y `DefaultToolRenderContext.RenderTool` |
| Tool renderer · Crush shell/jobs | `demos/crush_tool_renderers.rs` — `Shell` | `internal/ui/chat/bash.go` — `BashToolRenderContext.RenderTool`, `JobOutputToolRenderContext.RenderTool`, `JobKillToolRenderContext.RenderTool`; shell interactivo en `chat/shell.go` — `ShellItem.Render` |
| Tool renderer · Crush files | `demos/crush_tool_renderers.rs` — `File` | `internal/ui/chat/file.go` — renderers `View`, `Write`, `Edit`, `MultiEdit` y `Download` |
| Tool renderer · Crush search | `demos/crush_tool_renderers.rs` — `Search` | `internal/ui/chat/search.go` — renderers `Glob`, `Grep`, `LS` y `Sourcegraph` |
| Tool renderer · Crush web | `demos/crush_tool_renderers.rs` — `Web` | `internal/ui/chat/fetch.go` — renderers `Fetch`, `WebFetch` y `WebSearch` |
| Tool renderer · Crush agents | `demos/crush_tool_renderers.rs` — `Agent` | `internal/ui/chat/agent.go` — `AgentToolRenderContext.RenderTool` y `AgenticFetchToolRenderContext.RenderTool` |
| Tool renderer · Crush MCP | `demos/crush_tool_renderers.rs` — `Mcp` | `internal/ui/chat/mcp.go` — `MCPToolRenderContext.RenderTool`; Docker MCP en `chat/docker_mcp.go` — `DockerMCPToolRenderContext.RenderTool` |
| Tool renderer · Crush diagnostics/LSP | `demos/crush_tool_renderers.rs` — `Diagnostics` | `internal/ui/chat/diagnostics.go` — `DiagnosticsToolRenderContext.RenderTool`; reinicio en `chat/lsp_restart.go` — `LSPRestartToolRenderContext.RenderTool` |
| Tool renderer · Crush todos | `demos/crush_tool_renderers.rs` — `Todos` | `internal/ui/chat/todos.go` — `TodosToolRenderContext.RenderTool` |
| Tool renderer · Crush generic fallback | `demos/crush_tool_renderers.rs` — `Generic` | `internal/ui/chat/generic.go` — `GenericToolRenderContext.RenderTool` |
| Diff output · Crush | `demos/crush_code_surfaces.rs` — `CrushDiffDemo` | `internal/ui/chat/unified_diff.go`; render de diff y estilos en `internal/ui/common/diff.go`, `internal/ui/diffview/` |
| Code navigation tools · Crush | `demos/crush_code_surfaces.rs` — `CrushCodeNavigationDemo` | `internal/ui/chat/{definition,references,call_hierarchy,symbols,rename,replace_symbol}.go` — renderer específico de cada tool |
| Thinking · Crush | `demos/crush_thinking.rs`; `widgets/thinking.rs::render_crush` | `internal/ui/chat/assistant.go` — `AssistantMessageItem.Render` (sección thinking y ventana plegada) |
| Status bar · Crush help/footer | `demos/crush_chrome.rs` — `CrushFooterDemo` | `internal/ui/model/status.go` — `(*Status).Draw`; mapa de ayudas de teclado en el modelo |
| Compact header · Crush | `demos/crush_chrome.rs` — `CrushCompactHeaderDemo` | `internal/ui/model/ui.go` — `(*UI).drawHeader`; detalle de marca y sesión en `model/header.go` — `(*header).drawHeader` y `renderHeaderDetails` |
| Welcome · Crush landing | `demos/crush_landing.rs` | `internal/ui/model/landing.go` — `(*UI).landingView`; logo en `internal/ui/model/ui.go` — `renderLogo` y `internal/ui/logo/` |
| Onboarding · Crush initialize | `demos/crush_misc.rs` — `Initialize` | `internal/ui/model/onboarding.go` — vista `initializeView` |
| Session details · Crush compact overlay | `demos/crush_misc.rs` — `SessionDetails` | `internal/ui/model/ui.go` — `(*UI).drawSessionDetails` |
| Exit banner · Crush | `demos/crush_misc.rs` — `ExitBanner` | `internal/ui/exitbanner/exitbanner.go` — `exitbanner.Render` y `logoSection` |
| Inline editor · Crush | `demos/crush_misc.rs` — `InlineEditor` | Contrato y layout: `internal/ui/dialog/inline_editor.go` y `internal/ui/model/ui.go` (`activeInline`); implementaciones visibles: `dialog/plan_handoff_inline.go` y `dialog/question_{form,freetext,editor}.go`. La muestra Batten es una aproximación estática. |
| Sidebar scroll · Crush | `demos/crush_misc.rs` — `SidebarScroll` | `internal/ui/model/sidebar.go` — `(*UI).drawSidebar`; lista en `internal/ui/list/`; scrollbar en `internal/ui/common/scrollbar.go` |
| Help · Crush expanded hints | `demos/crush_misc.rs` — `Help` | Bubbles `help.Model`; integración de status/help en `internal/ui/model/status.go` — `(*Status).Draw` |
| Context sidebar · Crush | `demos/crush_context_sidebar.rs` | `internal/ui/model/sidebar.go` — `(*UI).drawSidebar`; datos de contexto se componen desde el modelo de sesión |
| Model picker · Crush | `demos/crush_dialogs.rs` — `Models` | `internal/ui/dialog/models.go` — `(*Models).Draw`; lista y filas en `models_list.go` (`ModelsList.Render`) y `models_item.go` (`ModelItem.Render`) |
| Permission · Crush | `demos/crush_dialogs.rs` — `Permission` | `internal/ui/dialog/permissions.go` — `(*Permissions).Draw` |
| Conversation list · Crush sessions | `demos/crush_dialogs.rs` — `Sessions` | `internal/ui/dialog/sessions.go` — diálogo `Sessions`; filas en `sessions_item.go` |
| Command menu · Crush | `demos/crush_dialogs.rs` — `Commands` | `internal/ui/dialog/commands.go` — `(*Commands).Draw`; filas en `commands_item.go` — `(*CommandItem).Render` |
| File picker · Crush | `demos/crush_dialogs.rs` — `Files` | `internal/ui/dialog/filepicker.go` — `(*FilePicker).Draw` |
| Question dialog · Crush | `demos/crush_dialogs.rs` — `Question` | `internal/ui/dialog/question_single.go`; variantes relacionadas: `question_multi.go`, `question_freetext.go`, `question_form.go`, `question_editor.go`, `question_choice_base.go` |
| Autocomplete · Crush | `demos/crush_atoms.rs` — `Completions` | `internal/ui/completions/completions.go` — `(*Completions).Render`; filas y teclas en `item.go` y `keys.go` |
| Attachment chips · Crush | `demos/crush_atoms.rs` — `Attachments` | `internal/ui/attachments/attachments.go` — `(*Attachments).Render` y `(*Renderer).Render` |
| Todo / queue pills · Crush | `demos/crush_atoms.rs` — `Pills` | `internal/ui/model/pills.go` — render de pills de TODO y cola |
| Settings · Crush providers | `demos/crush_dialogs.rs` — `Providers` | `internal/ui/dialog/auth_method.go` — `(*AuthMethod).Draw` (selector de método/proveedor; la demo resume la lista) |
| Credentials · Crush API key | `demos/crush_dialogs.rs` — `ApiKey` | `internal/ui/dialog/api_key_input.go` — `(*APIKeyInput).Draw`, `headerView`, `inputView` |
| Credentials · Crush OAuth | `demos/crush_dialogs.rs` — `OAuth` | `internal/ui/dialog/oauth.go` — `(*OAuth).Draw`; variantes por proveedor en `oauth_{openai,grok,hyper,copilot}.go` |
| Settings · Crush MCP | `demos/crush_dialogs.rs` — `Mcp` | `internal/ui/dialog/mcp_toggles.go` — `(*MCPToggles).Draw`, `scopeRadioView` e `innerContent` |
| Settings · Crush MCP authentication | `demos/crush_dialogs.rs` — `McpAuth` | `internal/ui/dialog/mcp_auth.go` — `(*MCPAuth).Draw`, `dialogContent` e `innerContent` |
| Settings · Crush router | `demos/crush_dialogs.rs` — `Router` | `internal/ui/dialog/router.go` — `(*Router).Draw`; modelos y pool en `router_model.go`, `router_model_pool.go` y filas en `router_item.go` |
| Settings · Crush notifications | `demos/crush_dialogs.rs` — `Notifications` | `internal/ui/dialog/notifications.go` — `(*Notifications).Draw` |
| Settings · Crush themes | `demos/crush_dialogs.rs` — `Themes` | `internal/ui/dialog/theme.go` — `(*Theme).Draw` y `ThemeItem.Render` |
| Theme editor · Crush | `demos/crush_dialogs.rs` — `ThemeEditor` | `internal/ui/dialog/theme_editor.go` — editor; creación en `theme_new.go` |
| Tool arguments · Crush | `demos/crush_dialogs.rs` — `Arguments` | `internal/ui/dialog/arguments.go` — `(*Arguments).Draw` |
| Plan handoff · Crush | `demos/crush_dialogs.rs` — `PlanHandoff` | `internal/ui/dialog/plan_handoff_inline.go` — diálogo/acciones de handoff inline |
| Confirmation · Crush quit | `demos/crush_dialogs.rs` — `Quit` | `internal/ui/dialog/quit.go` — diálogo `Quit`; confirmaciones relacionadas en `question_confirm.go` y `question_yesno.go` |

Las filas que agrupan varios tipos (por ejemplo, tools de archivo) indican la
familia de código de Crush que la muestra representa; no significan que la demo
reproduzca todos esos renderers. Para esos límites, ver la columna “Qué incluye
/ faltante” en las tablas del inventario.

## Composición de pantalla

| Superficie | Referencia de fuente | Batten | Qué incluye / faltante |
|---|---|---|---|
| Estados de aplicación | `internal/ui/model/ui.go` (`uiOnboarding`, `uiInitialize`, `uiLanding`, `uiChat`) | Parcial · demos Crush de landing, initialize y chat | Se ven estados principales; falta enlazar sus transiciones y datos |
| Header compacto | `model/header.go`, `model/ui.go` | Parcial · `Compact header / Crush` | Marca, separadores, ruta/rama, uso y ctrl+d; faltan LSP errors, créditos, recorte real por ancho y captura |
| Sidebar de sesión | `model/sidebar.go` | Parcial · `Context sidebar / Crush`, integrado al feed/composer en `Crush · sidebar + chat`; variantes del logo en `Logo · Crush · sidebar` y `sidebar fallback` | Título, ruta/rama, logo compacto real, modelo, proveedor, reasoning/contexto, archivos, LSP, MCP y Skills. Faltan scroll, scrollbar/foco, datos reales y cotejo de recortes |
| Layout ancho y compacto | `model/ui.go`, `model/layout_test.go` | Parcial · `Chat / Crush`, `Compact header / Crush` | Ambas composiciones; falta responsive por umbrales/alto y toggle persistente |
| Detalles de sesión compactos | `model/ui.go` (`drawSessionDetails`) | Parcial · `Session details / Crush` | Overlay con modelo/recursos/versión; falta detalle real y recorte |
| Landing | `model/landing.go` | Parcial · `Welcome / Crush · landing` | Marca, ruta/rama, modelo/proveedor, LSP/MCP/Skills y prompt; falta altura/espaciado exactos y fallback por terminal estrecho |
| Inicialización de proyecto | `model/onboarding.go` (`initializeView`) | Parcial · `Onboarding / Crush · initialize` | Texto, ruta, botones Yep!/Nope y selección; falta tamaño/alineación exactos |
| Banner / logotipo | `ui/logo/logo.go` (`SmallRender`, `Render`), `model/header.go`, `model/sidebar.go` (`updateSidebarScrollState`) | Referencia visual · `Logo · Crush · wide title art`, `sidebar`, `sidebar fallback`; también `Welcome / Crush` | Se muestran por separado el arte ancho `CRUSH`, el `Render(compact=true)` usado en el sidebar tras iniciar sesión y el fallback `SmallRender` para paneles bajos. El estiramiento de letra es una muestra fija; falta la variación aleatoria en runtime |
| Editor del composer | `model/ui.go` (`renderEditorView`), Bubble `textarea.Model` | Parcial · `Composer / Crush` | Sin marco, prompt normal `>`/`:::`, placeholder y texto multilinea; faltan plan/YOLO/bang, cursor editable, resize 3–15 y altura del chat ligada al editor |
| Fila de adjuntos del composer | `ui/attachments/attachments.go` | Parcial · `Attachment chips / Crush` | Chips, contador y control de quitar; faltan selección y hit testing |
| Help y status footer | `model/status.go` | Parcial · `Status bar / Crush` | Muestra ayudas y badges plan/YOLO/router; falta el modelo de atajos real, truncado/expansión y mensajes temporales |
| Pills de tareas y cola | `model/pills.go` | Parcial · `Todo / queue pills / Crush` | Pills y filas expandidas; faltan borde, navegación/foco, animación y estado de cola actualizado |
| Transcript/chat | `model/chat.go`, `ui/list/`, `ui/chat/` | Parcial · `Chat / Crush`, `Transcript / Crush` | Reproducción estática del layout y feed; faltan viewport virtual, auto-follow, selección y estados completos |
| Banner de salida | `ui/exitbanner/exitbanner.go` | Parcial · `Exit banner / Crush` | Saludo y resume hint compacto; falta logo arte y variantes por configuración |

## Mensajes, actividad y llamadas de herramientas

| Componente | Referencia | Batten | Qué hay que capturar |
|---|---|---|---|
| Mensaje de usuario | `ui/chat/user.go` | Parcial · `Transcript / Crush` | Emisor/contenido; faltan Markdown, rail de foco/selección, Skill y adjuntos |
| Mensaje de asistente | `ui/chat/assistant.go`, `streaming_markdown.go` | Parcial · `Transcript / Crush` | Emisor, texto y salida de archivo; falta Markdown incremental y cache/resize |
| Thinking / reasoning | `ui/chat/assistant.go` | Parcial · `Thinking / Crush` | Caja, estado activo/completo, cola plegada (10 líneas) y duración; falta Markdown real/estilos de truncado exactos |
| Espera / generación | `ui/chat/pending.go`, `ui/anim/` | Parcial · `Assistant message / Crush · pending` | Estado “Thinking”; falta animación/frame label y resumen |
| Error y rechazo | `ui/chat/assistant.go` | Parcial · `Transcript / Crush` | Banda de error; faltan rechazo, detalles, truncado y streaming |
| Tarjeta de plan | `ui/chat/assistant.go` | Parcial · `Plan card / Crush` | Plan y acciones representados; falta flujo streaming/handoff |
| Info/contexto de asistente | `ui/chat/messages.go` | Parcial · `Assistant message / Crush · summary` | Resumen de sesión; faltan variantes de info reales |
| Pregunta interactiva en transcript | `ui/chat/question.go`, `ui/dialog/question_*.go` | Parcial · `Inline question / Crush` | Opciones con foco; faltan más tipos y flujo de respuesta |
| Tool común | `ui/chat/tools.go` | Parcial · `Tool call / Crush` | Estados permiso/corriendo/éxito/error/cancelado, resumen y salida 10 líneas plegada/expandida; faltan iconos reales, solicitud/diff por tool y modo compacto |
| Shell / Bash y jobs | `ui/chat/bash.go`, `shell.go` | Parcial · `Tool renderer / Crush · shell/jobs` | Comando, salida y estados; faltan jobs activos, kill/output y render exacto |
| Operaciones de archivo | `ui/chat/file.go` | Parcial · `Tool renderer / Crush · files` | Preview con líneas de código; faltan View/Write/Edit/MultiEdit/Download individualizados y diffs |
| Búsqueda y navegación de archivos | `ui/chat/search.go` | Parcial · `Tool renderer / Crush · search` | Resultados ruta:línea; faltan Glob/LS/Sourcegraph y truncado real |
| Web | `ui/chat/fetch.go` | Parcial · `Tool renderer / Crush · web` | Resultado de búsqueda/fetch; faltan Fetch/WebFetch/WebSearch diferenciados |
| Agente / agentic fetch | `ui/chat/agent.go` | Parcial · `Tool renderer / Crush · agents` | Muestra resumen delegado; falta detalle de subagent y resultados reales |
| Herramientas MCP | `ui/chat/mcp.go`, `docker_mcp.go` | Parcial · `Tool renderer / Crush · MCP` | Muestra nombre/resultado; falta herramienta Docker MCP y detalles del servidor |
| LSP y diagnósticos | `ui/chat/lsp_restart.go`, `diagnostics.go` | Parcial · `Tool renderer / Crush · diagnostics/LSP` | Resultado de diagnóstico; faltan severidades/casos y renderer de restart |
| Símbolos / acciones de código | `ui/chat/{definition,references,call_hierarchy,symbols,rename,replace_symbol}.go` | Parcial · `Code navigation tools / Crush` | Lista común y resultado; faltan argumentos/formatos específicos por acción |
| Todo list de agente | `ui/chat/todos.go`, `model/pills.go` | Parcial · `Tool renderer / Crush · todos`, `Todo / queue pills / Crush` | Lista y estados básicos; falta sincronía entre ambas presentaciones |
| Preguntas / tool fallback | `ui/chat/question.go`, `generic.go` | Parcial · `Tool renderer / Crush · generic fallback` | Fallback de nombre/entrada/salida; falta bloque de pregunta interactivo |
| Unified diff | `ui/chat/unified_diff.go`, `ui/diffview/` | Parcial · `Diff output / Crush` | Hunk y líneas +/- con scroll; falta renderer side-by-side y estilo del diff de fuente |
| Imágenes del transcript | `ui/image/` | Pendiente | Codificación y salida de imagen en terminal compatible; fallback/capacidades |

El árbol `ui/chat/` también contiene infraestructura que no cuenta como
componente separado: caché por versión/ancho, render Markdown incremental,
resaltado de selección, parsing de resultados y fábrica que enruta cada nombre
de tool a su renderer. Se documenta porque determina estados y rendimiento de
las superficies anteriores.

## Popups, diálogos y edición inline

Crush implementa diálogos como overlays apilables (`ui/dialog/dialog.go`);
los componentes se dibujan sobre la pantalla activa. Cada fila
siguiente es una superficie a cotejar, incluso si se compone de una base común:

| Familia | Diálogos / fuentes | Batten |
|---|---|---|
| Comandos y archivos | `commands.go`, `commands_item.go`, `filepicker.go` | Parcial · `Command menu / Crush`, `File picker / Crush`; falta filtro real, iconos, detalles y empty state |
| Sesiones | `sessions.go`, `sessions_item.go` | Parcial · `Conversation list / Crush` — lista y foco; faltan filtros, metadatos y acciones reales |
| Confirmación y salida | `quit.go`, `question_confirm.go`, `question_yesno.go` | Parcial · `Confirmation / Crush` — confirmación de salida; falta texto dinámico y acciones |
| Permisos | `permissions.go` | Parcial · `Permission / Crush` — tool, comando y acciones; falta contenido específico y diff contextual |
| Preguntas del agente | `question_single.go`, `question_multi.go`, `question_freetext.go`, `question_form.go`, `question_editor.go`, `question_choice_base.go` | Parcial · `Question dialog / Crush` — opción única; faltan variantes múltiple, texto, formulario y editor |
| Modelos | `models.go`, `models_list.go`, `models_item.go`, `reasoning.go`, `arguments.go` | Parcial · `Model picker / Crush` — lista y selección; faltan grupos, filtros, reasoning y argumentos |
| Proveedores y credenciales | `auth_method.go`, `api_key_input.go`, `oauth.go`, `oauth_{openai,grok,hyper,copilot}.go`, `aws_sso.go` | Parcial · `Settings / Crush · providers`, `Credentials / Crush · API key` y `Credentials / Crush · OAuth` — superficies estáticas de lista, clave y espera OAuth; faltan variantes por proveedor, AWS SSO y flujo real |
| MCP | `mcp_toggles.go`, `mcp_auth.go` | Parcial · `Settings / Crush · MCP` y `Settings / Crush · MCP authentication` — lista y scopes OAuth; faltan toggles y flujo real |
| Router | `router.go`, `router_model.go`, `router_model_pool.go`, `router_item.go` | Parcial · `Settings / Crush · router` — resumen de configuración; falta selector y pool interactivo |
| Temas | `theme.go`, `theme_new.go`, `theme_editor.go` | Parcial · `Settings / Crush · themes` y `Theme editor / Crush` — lista y tokens de color; faltan preview en vivo y persistencia |
| Notificaciones | `notifications.go` | Parcial · `Settings / Crush · notifications` — lista de backends; faltan preferencias reales |
| Argumentos y confirmaciones especializadas | `arguments.go`, `auth_method.go`, `question_confirm.go` | Parcial · `Tool arguments / Crush` muestra valores de ejemplo; falta edición por tipo y confirmación funcional |
| Plan inline | `plan_handoff_inline.go` | Parcial · `Plan handoff / Crush` — opciones de start/revise/cancel; falta overlay inline y estado de foco |
| Editor inline | `inline_editor.go` y `question_editor.go` | Parcial · `Inline editor / Crush` — editor sustituto y prompt; faltan colapso/foco/resize |
| Overlay común | `common.go`, `dialog.go` | Parcial · diálogos Crush anteriores — marco redondeado y centrado; faltan gradientes, padding, scrollbar, help y ancho estrecho exactos |

## Elementos pequeños y estados

| Pieza | Fuente | Batten |
|---|---|---|
| Autocompletado | `ui/completions/{completions,item,keys}.go` | Parcial · `Autocomplete / Crush` — popup, tipos y foco; faltan filtro incremental, match highlights y posición ligada al cursor |
| Chips de adjuntos | `ui/attachments/attachments.go` | Parcial · `Attachment chips / Crush` — nombres, indicador de tipo, contador y quitar; faltan iconos exactos y hit testing |
| Lista desplazable | `ui/list/{list,item,focus,filterable,highlight}.go` | Parcial · `Sidebar scroll / Crush` — ventana/offset; faltan viewport lazy y foco/selección de listas |
| Scrollbar | `ui/common/scrollbar.go`, `ui/list/list.go` | Parcial · `Sidebar scroll / Crush` — track/thumb y movimiento; falta auto-hide y proporción por contenido |
| Ayuda y keycaps | Bubbles `help.Model`, `model/status.go`, diálogos | Parcial · `Help / Crush · expanded hints` — dos densidades; falta recorte/ancho real |
| Botones / selección | `ui/common/button.go`, `dialog/question_choice_base.go` | Parcial · init, question y permission dialogs; falta estilo por foco/hover/inactivo |
| Glyphs e iconos | `ui/styles/styles.go`, `ui/logo/`, renderers chat | Pendiente: repertorio de estados por tool, archivos, LSP/MCP, modelo y marca |
| Animación | `ui/anim/anim.go`, `chat/pending.go`, `model/pills.go` | Pendiente: frames, labels y movimiento reducido/estático |
| Diff y código | `ui/common/{diff,highlight,chromastyle}.go`, `ui/diffview/`, `ui/xchroma/` | Parcial · `Diff output / Crush`; código en tool files | Hunk y filas coloreadas; faltan sintaxis Chroma y layout unificado/split exactos |
| Markdown y código inline | `ui/common/markdown.go`, `ui/chat/streaming_markdown.go` | Pendiente: headings, listas, code fence, inline code, enlaces y render progresivo |
| Imagen terminal | `ui/image/image.go` | Pendiente: detección de capacidades y encoding de imagen |
| Notificaciones externas | `ui/notification/` | No es componente del canvas; inventariar aparte si se quiere cubrir OSC/bell/notificación nativa |
| Tema | `ui/styles/{styles,quickstyle,themes,palette,theme_file}.go` | Parcial · preset Crush | Charmtone y tema alternativo Gruvbox; estilos por estado/componentes, JSON, selección y refresh de cachés sin cambiar geometría |
| Tema por proveedor | `model/ui.go`, `styles/themes.go` | Pendiente | Cambio de familia visual según proveedor, salvo override explícito del usuario |

## Orden sugerido para completar el showcase

1. **Partes pequeñas:** header compacto, footer, chip de modo/estado, pill de
   cola/todos, completion y chip de adjunto.
2. **Piezas principales:** composer con alturas y placeholders, mensajes user/
   assistant, thinking/error y tarjetas de tools (shell, archivo, búsqueda).
3. **Marco de app:** sidebar con scroll/recursos y layout compacto, landing e
   inicialización.
4. **Overlays:** permissions, sessions, commands/file picker, model/auth,
   preguntas, MCP, themes, router y notifications.
5. **Familias especializadas:** tools LSP/MCP/web/agent, diff, imágenes,
   animación y salida.

Cada nueva demo se registra en `crates/batten/examples/showcase/demos/mod.rs`,
con familia y variante en el índice. Marcar `Importado` solo al cubrir la
superficie; usar `Parcial` si quedan estados o geometría. La comprobación 1:1
requiere capturas del Crush y Batten con terminal de igual tamaño, contenido,
tema, foco y estado; por ahora todas las comparaciones visuales de este
inventario están pendientes.

## Snapshot y licencia

Fuente: checkout local de `charmbracelet/crush` en el SHA indicado arriba.
`LICENSE.md` declara Functional Source License 1.1 con licencia MIT futura al
segundo aniversario. El inventario describe superficies y rutas; no transfiere
código ni assets. La demo actual del sidebar recrea la estructura con código
propio. Antes de copiar código/assets, volver a revisar la licencia y conservar
los avisos exigidos.
