# Inventario funcional de MastraCode y Crush

> Estado: inventario visual por código completado para composer, transcript,
> herramientas y temas. El inventario funcional general y la comparación runtime
> siguen parciales; se separan de los hallazgos de código.
>
> MastraCode instalado: `0.47.1` (`mastracode` npm, Apache-2.0).
> Crush instalado: `v0.98.2-0.20261017181040-852f410573e1+dirty`.
>
> Alcance del inventario: capacidades del producto que podrían informar el
> diseño/evolución de CodeLoom; no es un compromiso de implementar todo.

## Catálogo documentado inicial

| Área | MastraCode | Crush | Estado de revisión | Relación potencial con Loom |
|---|---|---|---|---|
| Inicio y contexto de proyecto | TUI desde directorio, onboarding, contexto por proyecto | CLI/TUI, inicialización que genera archivo de contexto, reglas globales/proyecto e ignore | Documentado; ejecución pendiente | Mejorar inicio/configuración sin acoplar el core al TUI |
| Modelos y proveedores | OAuth/API keys, modelos por modo, packs, cambiar modelo/modo, thinking level | Model picker, proveedores integrados/custom y cambio de modelo en sesión | Documentado; ejecución pendiente | UX de selección; Loom ya soporta proveedor/modelo por config |
| Sesiones y mensajes | Threads persistentes por proyecto; crear, clonar, listar, renombrar, reanudar; cola de follow-up | Múltiples sesiones/contextos por proyecto | Documentado; código/runtime pendiente | Loom ya tiene SessionStore; revisar navegación y continuidad |
| Composer y modos | `pi-tui` Editor envuelto por custom prompt; multilinea, wrap/scroll, `→ / @ !`, panel sombreado, tema por modo | `Bubbles textarea`, altura dinámica 3–15 + márgenes, prompt/placeholders, layout compacto | Código de prioridad inspeccionado; capturas multilínea/cursor pendientes | Prioridad inmediata: reproducir geometría y reglas en perfiles separados |
| Transcript y respuesta | Markdown por partes y streaming; thinking ocultable; tools compactas/expandibles y tipadas | Lista virtual; thinking en modos; errores/refusal; tools tipadas con estado y expansión | Código de prioridad inspeccionado; estados runtime equivalentes pendientes | Batten ya tiene transcript/widgets; comparar densidad, separación, wrap y estados |
| Herramientas incorporadas | View, edit, search, shell, web search y herramientas listadas en docs | Bash, edit, view, grep, glob, LSP y otras según build | Catálogo por nombre/argumento pendiente | Comparar con herramientas actuales de Loom; adoptar solo vacíos necesarios |
| Permisos y aprobación | Permisos por herramientas, YOLO, sandbox y aprobación | Ask/allow/deny, YOLO, herramientas denegadas/ocultas | Documentado; flujo en captura pendiente | Prioridad de seguridad; conservar barreras y flujo de permisos Loom |
| Contexto y memoria | Observational Memory, skills, prompts/contexto de proyecto | Archivos globales/proyecto, AGENTS/CRUSH.md, `.crushignore` | Documentado; comportamiento pendiente | Contexto simple útil; memoria avanzada y skills son posteriores en Loom |
| Extensiones y personalización | MCP, hooks, comandos Markdown, plugins, workflows, custom modes/tools/subagents/storage | MCP stdio/HTTP/SSE, skills, hooks, shell config, agentes/herramientas | Por verificar por versión | Mapear principalmente a etapas m2+; no añadir al MVP por referencia |
| Agentes y tareas largas | Goals persistentes con juez, subagentes, workflows dinámicos | Agent/coordinator, tools de agente y resumen automático | Verificar builds y flujos | Encaja con flock/jobs futuros; no implica llevarlo a m1 |
| Integraciones y ciclo de desarrollo | Git, PR review/GitHub, browser, headless/CI, API y notificaciones | LSP, MCP, notificaciones, provider updates, initialize/context, CLI | Parcialmente documentado | Priorizar integración por necesidad del MVP y etapa del roadmap |
| Navegación y comandos | Slash commands, atajos de teclado, settings panel, completions | Comandos CLI, model picker y command palette de skills invocables | Captura y lista exhaustiva pendiente | Comparar discoverability, foco, navegación y ayudas |
| Apariencia y terminal | Roles de color/superficie, dark/light, pares de temas, temas JSON y contraste con terminal | Tokens centralizados, Charmtone/Gruvbox, temas JSON y layout compacto | Código de prioridad inspeccionado; Loom tiene tokens TOML parciales, sin perfil de layout | Recomendación: perfiles completos de presentación; paletas actuales no bastan |
| Integración externa | Varios proveedores de modelos; MCP y APIs | LSP/MCP y proveedores de modelos | Verificar contra builds instaladas | Adoptar solo integraciones compatibles con límites y roadmap de Loom |

## Observaciones runtime de las builds locales

- MastraCode `0.47.1`: composer vacío visible como flecha de entrada; pie con
  modo `build`, modelo `llama-server/local`, uso/contexto y ruta del proyecto.
  `/help` presenta comandos agrupados y atajos. En una petición local de solo
  lectura, el transcript mostró estado `working` con tiempo/escape para
  interrumpir, chip `view` con ruta, salida en bloque y resumen final.
- Crush `v0.98.2-0.20261017181040-852f410573e1+dirty`: banner ilustrado,
  carpeta, modelo y razonamiento, tres columnas para LSP/MCP/Skills, composer
  `Ready...`/`>` y ayuda inferior. `Ctrl+C` abre confirmación; doble `Ctrl+C`
  sale directamente. La aplicación seleccionó OpenRouter, así que no se envió
  una petición. Detalle y límites en `runtime-observations.md`.
- Aún no es comparación geométrica controlada: no se fijó igual tamaño de
  terminal y no se guardaron PNG. REA `capture-process` está deshabilitado en
  Windows.

## MastraCode: comandos y flujos de interfaz documentados

### Comandos vistos en `/help` de la instalación local 0.47.1

La ayuda local incluyó `/new`, `/clone`, `/threads`, `/resume`, `/thread`,
`/thread:tag-dir`, `/name`, `/rename`, `/resource`, `/skills`, `/skill/<name>`,
`/models`, `/packs`, `/model`, `/custom-providers`, `/subagents`,
`/permissions`, `/settings`, `/memory` (alias `/om`), `/review`,
`/report-issue`, `/cost`, `/context`, `/profile`, `/prune`, `/diff`,
`/sandbox`, `/hooks`, `/mcp`, `/connect`, `/login`, `/logout`, `/setup`,
`/browser`, `/api-keys`, `/plugins`, `/theme`, `/update`, `/observability`,
`/github`, `/goal`, `/goal judge`, `/mode`, `/exit` y `/help`; también mostró
el comando personalizado `//morning` y la ejecución de shell `!<cmd>`. Esta
lista describe lo que presentó esa build local, no valida que cada comando se
haya ejecutado ni que tenga una implementación activa idéntica a la documentada.

Atajos observados en la ayuda local: `Enter` envía, `Ctrl+F` pone un follow-up
en cola, `Ctrl+T` alterna thinking, `Ctrl+E` expande/colapsa salidas de tools,
`Ctrl+Y` alterna YOLO, `Shift+Tab` cambia modo, `Ctrl+C` interrumpe/limpia y
dos pulsaciones salen, `Ctrl+D` sale con editor vacío, `Ctrl+Z` suspende y
`Alt+Z` deshace el último borrado. La lista se observó en `/help`; solo se
ejercitaron el envío y lectura del fixture, y `/exit`.

La documentación de CLI enumera `/new`, `/clone`, `/threads`, `/thread`,
`/name`, `/model`, `/models` (`/packs`), `/mode`, `/goal`, `/think`,
`/permissions`, `/yolo`, `/settings`, `/om`, `/subagents`, `/skills`,
`/skill/<name>`, `/plugins`, `/workflows`, `/profile`, `/cost`, `/diff`,
`/resource`, `/thread:tag-dir`, `/sandbox`, `/review`, `/github`,
`/report-issue`, `/connect`, `/login`, `/logout`, `/api-keys`,
`/custom-providers`, `/observability`, `/voice`, `/setup`, `/theme`,
`/browser`, `/update`, `/hooks`, `/mcp`, `/help` y `/exit`.

Atajos documentados: `Ctrl+C` interrumpe/limpia (doble toque sale), `Ctrl+D`
sale con editor vacío, `Ctrl+Z` suspende, `Alt+Z` deshace un borrado,
`Ctrl+T` muestra/oculta thinking, `Ctrl+E` expande/colapsa herramientas,
`Enter` envía o pone en cola y `Ctrl+Y` cambia YOLO. Confirmar estos controles
en la versión local antes de proponer compatibilidad con Loom.

## Crush: capacidades documentadas a completar por código/runtime

- Proveedores/modelos: múltiples modelos, proveedor compatible custom y cambio
  durante la sesión.
- Sesión/proyecto: sesiones múltiples, contexto por proyecto, archivos de
  contexto globales, inicialización del contexto y patrones de exclusión.
- Coding: herramientas integradas, contexto LSP automático/configurable y
  soporte de varios terminales/plataformas.
- Control: permisos por herramienta (ask/allow/deny), modo YOLO y herramientas
  que se pueden deshabilitar/ocultar.
- Extensión: MCP por stdio/HTTP/SSE, Agent Skills, hooks y configuración de
  agentes/herramientas.
- UI: TUI adaptable, compact mode, selector de modelo, notificaciones
  configurables (auto/native/OSC/bell/disabled), progresos de operación y
  preferencias de tema por confirmar contra schema/build.
- Operación: comandos CLI, provider updates, métricas desactivables y reglas de
  atribución para contenido generado.

La documentación upstream consultada añade matices útiles al mapa: Crush ofrece
`crushrc` ejecutable con built-ins de configuración, además de JSON legado;
configura proveedores/modelos, MCP (stdio/HTTP/SSE), LSP, hooks y permisos. Sus
permisos pueden omitir confirmaciones (`allow`) o esconder herramientas (`deny`);
`--yolo` omite todas las confirmaciones. `crushrc` es código de shell ejecutado
al cargar la configuración, por lo que estos detalles solo se estudiarán como
funcionalidad y no se activarán en la inspección. Fuentes upstream: [guía de
configuración](https://github.com/charmbracelet/crush/blob/main/docs/config/README.md),
[guía de hooks](https://github.com/charmbracelet/crush/blob/main/docs/hooks/README.md).
La rama `main` puede diferir de la build local `dirty`; no atribuir estas
características a la versión instalada hasta comprobar su fuente/tag.

## Evidencia técnica ya reunida

El recorrido por código del composer, temas y presentación de herramientas
queda ampliado en [`code-findings.md`](code-findings.md). Confirma que los dos
referentes difieren en composición, no solo en color: MastraCode integra un
editor tipo prompt sombreado y herramientas expandibles dentro del flujo de
mensajes; Crush organiza la pantalla en regiones con encabezado, editor,
recursos laterales y estado. La evidencia exacta de Crush sigue limitada por
la build local `+dirty`; ver el informe antes de tratar las referencias a
`main` como hechos de esa build.

- MastraCode: `README.md` de la instalación npm describe agente de terminal,
  conversaciones persistentes por proyecto, varios proveedores, herramientas,
  goals, plugins, workflows dinámicos y Observational Memory. El bundle TUI
  `dist/tui-CLkPtYAM.js` se analizó con REA; ver el Evidence ID registrado en
  `docs/specs/2026-10-18-loom-composer-tool-visual-references.md`.
- Crush: el README upstream enumera multi-modelo, cambio de modelo en sesión,
  sesiones, LSP y MCP. El AGENTS.md upstream documenta arquitectura, comandos,
  agentes y herramientas. La build local está marcada `dirty`, por lo que el
  número de versión no identifica por sí solo todos los cambios locales.
- REA `inspect-go-binary` confirmó en el ejecutable local Crush Go `1.27.0`,
  `amd64`, Bubble Tea v2.1.0, Bubbles v2.2.1, Lipgloss v2.0.6 y Glamour v2.0.1
  (evidencia `ev_1079fc964bcb01da5141a7e63054f64a28e19922422fdb37e90dc18641e333c4`).
  Esto identifica tecnologías incluidas, no funciones visibles.
- REA analizó estáticamente una copia del bundle TUI de MastraCode con Node
  24.21 (evidencia `ev_52ddd92ce1fd4c2aac95ae38bc0eff8443d6295578265484b80105fdc204654b`).
  No ejecutar el bundle es una propiedad de este análisis; el grafo es grande y
  todavía falta consultar sus nodos con una vista acotada.
- REA `capture-process` no puede ejecutar procesos TUI en Windows: el proveedor
  rechaza la operación antes de crear el hijo porque no acredita limpieza de
  descendientes. La captura runtime debe usar otro mecanismo en este host.
- Las listas de documentación son un mapa inicial, no una afirmación de que
  cada función esté activa en las instalaciones actuales ni se comporte igual
  en todos los terminales.

## Fuentes

- MastraCode instalado: `C:\Program Files\nodejs\node_modules\mastracode\README.md`,
  `package.json` y bundle TUI `dist/tui-CLkPtYAM.js`.
- [Mastra Code overview](https://code.mastra.ai/),
  [Modes](https://code.mastra.ai/modes), [Tools](https://code.mastra.ai/tools),
  [Configuration](https://code.mastra.ai/configuration),
  [Customization](https://code.mastra.ai/customization) y
  [Headless mode](https://code.mastra.ai/headless).
- [Crush README](https://github.com/charmbracelet/crush) y
  [Crush AGENTS.md](https://github.com/charmbracelet/crush/blob/main/AGENTS.md).
- [Crush configuration](https://github.com/charmbracelet/crush/blob/main/docs/config/README.md)
  y [hooks](https://github.com/charmbracelet/crush/blob/main/docs/hooks/README.md);
  upstream `main`, no necesariamente idéntico al ejecutable local.
- [Crush schema](https://github.com/charmbracelet/crush/blob/main/schema.json)
  para opciones y configuración del TUI.
- [REA: análisis de aplicaciones JavaScript](https://github.com/janblade/REA#what-you-can-analyze).

## Próximo paso

Cambiar cada fila pendiente por capacidades y flujos concretos, añadir una
referencia por característica (ruta/módulo, sección de documentación o captura)
y clasificarla como: ya existe en Loom, adoptar en m1, considerar en m2+, o no
aplica. No copiar código fuente de los productos de referencia.
