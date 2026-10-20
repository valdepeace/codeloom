# Estado funcional de CodeLoom

**Corte:** 2026-10-20  
**Alcance:** motor local ( `codeloom-core` + CLI), TUI (`codeloom-tui` + Batten) y conexión entre ambos.  
**Fuente del estado:** lectura del código presente en el workspace y contraste con el plan MVP y los specs. Este documento describe el árbol de trabajo actual; no implica que los cambios estén committeados.

## Cierre de M1 — 2026-10-20

La implementación de M1 queda cerrada en el árbol de trabajo. Pasan `cargo fmt --all --check`, `cargo build`, `cargo test` (367 pruebas), `cargo clippy -- -D warnings` (sin errores de Clippy) y `git diff --check`. Se usó `CARGO_TARGET_DIR=E:\workspace\personal\loom\target` para no bloquear el target compartido. Cargo mantiene el aviso conocido de que `codeloom-cli/src/main.rs` pertenece a los binarios `loom` y `codeloom`.

La cancelación del turno y la salida por doble Esc tienen cobertura automatizada; el usuario confirmó manualmente que `Ctrl+J` mantiene las líneas y el cursor visibles y que doble Esc sale. La inspección visual amplia del resto de estados de la TUI sigue pendiente. No se enviaron prompts al provider durante el cierre.

## Verificación realizada

| Comprobación | Resultado | Observación |
|---|---|---|
| `rtk cargo build -p codeloom-cli` | ✅ Pasa | Warning existente: `main.rs` se declara para los binarios `loom` y `codeloom`. |
| `rtk cargo test -p codeloom-core -p codeloom-tui` | ✅ 89 pruebas pasan | La prueba de cancelación se corrigió para cancelar después de recibir un delta parcial, que es el caso que debe guardar la respuesta como interrumpida. |
| `rtk cargo test -p batten -p codeloom-tui` | ✅ 254 pruebas pasan | Incluye el tamaño compacto del diálogo de permisos y su límite para contenido largo. |
| `rtk cargo test -p codeloom-core -p codeloom-tui -p codeloom-cli` | ✅ 97 pruebas pasan | Incluye listado por proyecto, selección y restauración de conversaciones. |
| `rtk cargo build -p codeloom-cli` | ✅ Pasa tras integrar el workspace dashboard en la TUI | Warning existente: `main.rs` está declarado para ambos binarios. |
| `rtk cargo fmt --all --check` + `rtk cargo check -p codeloom-cli` | ✅ Pasa tras añadir fases y duración medida por evento | `cargo build` no pudo reemplazar `loom.exe` porque estaba abierto durante la prueba. |
| `rtk cargo fmt --all --check` + `rtk cargo check -p codeloom-cli` | ✅ Pasa tras añadir gestión interactiva de cola | `cargo check` informa solo del warning existente por declarar `main.rs` en ambos binarios. |
| `cargo fmt --all --check`, `cargo test -p batten`, `cargo test -p codeloom-tui` | ✅ Pasa tras ajustar composer multilínea | 212 pruebas de Batten y 50 de TUI; el alto crece con las líneas y, si se limita, el scroll mantiene visible el cursor. Falta validación en terminal real. |
| `cargo fmt --all --check`, `cargo test -p codeloom-core -p codeloom-tui`, `cargo check -p codeloom-cli` | ✅ Pasa tras añadir ajustes de provider | 101 pruebas pasan; el CLI compila. Aviso existente: `main.rs` está declarado en ambos binarios. |
| Test CLI `validates_provider_settings_before_persisting` | ✅ Pasa | Ejecutado con un target temporal porque el `loom.exe` del target compartido estaba abierto. |
| `rtk cargo build -p codeloom-cli -p batten --example showcase` | ✅ Compila | El diálogo compartido compila en la app y en el showcase. |
| Arranque de `loom` | ✅ Arranca y dibuja el dashboard | No se envió prompt ni se hizo petición al provider. |
| Confirmación manual de Ctrl+J, cursor y doble Esc | ✅ Confirmada por el usuario | La inspección visual amplia del resto de estados sigue pendiente. |

## Cómo leer el estado

| Marca | Significado |
|---|---|
| ✅ | Implementado en el flujo funcional de extremo a extremo |
| 🟡 | Parcial, con una limitación explícita o integración incompleta |
| 🟦 | Presencia visual/placeholder sin capacidad real en el motor |
| ⬜ | No implementado en el código actual |

“Motor local” es el nombre usado aquí para `core` y el CLI. CodeLoom actualmente es una aplicación local CLI/TUI que se conecta a un provider LLM; no se ha identificado un servidor HTTP/API propio.

## Inventario de funcionalidades

| Funcionalidad | Motor local (`core`/CLI) | UI (`codeloom-tui`/Batten) | Integración entre ambos | Estado |
|---|---|---|---|---|
| Envío y respuesta | `AgentLoop` recibe prompts y ejecuta turnos contra el provider | Composer multilinea; Enter envía y Ctrl+J inserta una línea | Canal de prompts hacia el worker y bus de eventos de vuelta | ✅ |
| Comandos slash | Acciones locales; no se envían al provider | `/` abre sugerencias, flechas seleccionan, Tab completa, Enter ejecuta y Esc permite texto literal; `/theme` cambia el preset | Ayuda, sesión nueva/selector, ajustes, cola, herramientas, cancelación, pausa/reanudación, sidebar, tema y salida reutilizan los flujos existentes | ✅ Sin argumentos ni comandos personalizados; validación visual pendiente |
| Streaming | Adaptadores OpenAI-compatible y Anthropic | Deltas se anexan al transcript | `LiveEvent::Delta` actualiza la entrada del asistente | ✅ |
| Thinking | Existe `ReasoningDelta` en el modelo de eventos | Muestra thinking y la última línea recibida | El evento cambia el estado y texto en la app | 🟡 Depende de que el provider emita razonamiento compatible |
| Tools | `view`, `glob`, `grep`, `edit`, `bash`; lectura paralela y escritura exclusiva | Conserva ocho llamadas; Ctrl+T inspecciona estado, argumentos y salida, permite desplazamiento y copia con Y | `ToolStarted`, `ToolFinished` y `MessageAdded` actualizan Batten y el inspector | ✅ Copia mediante OSC52; validación en terminal real pendiente |
| Permisos | Servicio con permitir, denegar y permitir durante la sesión | Diálogo compacto que crece con argumentos/diff y conserva scroll | Respuesta del diálogo vuelve al motor antes de ejecutar | ✅ |
| Cancelación | `CancellationToken` en loop, stream y ejecución de tools; respuesta parcial termina como interrumpida | Esc cancela; doble Esc sale | El TUI cancela el token del turno activo | ✅ Core probado; teclado confirmado manualmente por el usuario |
| Persistencia | SQLite lista sesiones no vacías por proyecto y carga sus mensajes; cada respuesta guarda el uso de tokens del provider | Selector inicial con preview/cuenta; flechas + Enter reabren, `N` crea sesión nueva; reconstruye transcript, historial de prompts, tools y contadores guardados | El CLI entrega el historial seleccionado a la app; `AgentLoop` continúa usando el mismo `SessionId` | 🟡 Recupera conversación, tools y tokens; la lista de archivos es estado efímero de UI según el spec actual |
| Provider/configuración | `codeloom.toml`, provider, modelo, URL y API key por variable de entorno | F2 abre ajustes; permite elegir tipo, modelo, URL y nombre de la variable de entorno | Valida, guarda solo esos campos preservando comentarios y aplica el provider al agente; exige HTTPS o HTTP local y nunca escribe el secreto | ✅ |
| Uso de tokens/contexto | Publica uso de tokens | Muestra contadores de entrada/salida | `LiveEvent::Usage` acumula contadores | 🟡 No calcula porcentaje del límite de contexto |
| Archivos cambiados | `ChangeTracker` y `FileDiff` por cada `edit` exitoso | Dashboard muestra el panel y suma de líneas | `LiveEvent::FileDiff` actualiza la lista | 🟡 El spec limita la detección a `edit`; escrituras por `bash` no se detectan |
| Estado y errores | Eventos de inicio/fin, error y `StepStarted { step, max_steps }` | Dashboard muestra fase activa, resumen acumulado de Provider/Thinking/Tools, paso actual, thinking y actividad de tools | Los eventos del bus actualizan fase, cronómetro y acumulados por turno | ✅ Duraciones acumuladas por eventos y visibles al cerrar el turno; se reinician al iniciar otro |
| Cola | El worker recibe prompts de uno en uno | Ctrl+K abre selección, Ctrl+↑/↓ reordena, Delete elimina, Enter edita, Space pausa/reanuda y Ctrl+X cancela el turno activo y pausa | El siguiente prompt se despacha al terminar o fallar el turno, si el diálogo está cerrado y la cola no está pausada | ✅ Cola volátil; edición conserva el original hasta confirmar y no altera el borrador del composer |
| Flock/subagentes | No hay runtime de subagentes implementado | Etiquetas m2 y estado “Subagents not available” | No hay eventos/modelos que conecten UI y motor | 🟦 Solo referencia visual |
| Jobs | No hay registro ni ejecución independiente de jobs | El header menciona jobs | Sin contrato ni flujo conectado | 🟦 Solo referencia visual |
| herdr | No existe crate de integración en el workspace actual | Sin modo host/socket integrado | Pendiente de implementar | ⬜ |
| MCP, skills y memoria | Hay specs; no están conectados al loop actual | Sin gestión visual funcional | Pendiente conectar descubrimiento/contexto con cada step | ⬜ |
| Voz, compresión, routing y KPIs | Opciones documentadas, fuera del MVP actual | Sin controles funcionales para esas capacidades | Requerirán módulos/eventos propios si se priorizan | ⬜ |

## Pendientes priorizados

| Prioridad | Pendiente | Trabajo del motor | Trabajo de UI / integración | Criterio para cerrar |
|---|---|---|---|---|
| ✅ | Cancelación | El core cancela el stream y guarda la respuesta parcial como interrumpida | Esc cancela, doble Esc sale y la TUI vuelve a `ready`; automatizado | Cobertura automatizada y confirmación manual de teclado completadas |
| ✅ | Archivos modificados según el spec | `ChangeTracker` usa Git y respaldo por líneas; `edit` exitosos publican `FileDiff` | Dashboard muestra conteos y rutas; path largo y área 0×0 cubiertos | Implementado y probado; escrituras de `bash` y persistencia de la lista siguen fuera del spec |
| P1 | Historial de duración por fase | Conservar las duraciones cerradas de provider/thinking/tools | Mostrar un resumen de tiempo por fase | Implementado en el acumulador por turno y panel Agent; validar con el test de transición |
| P1 | Gestión de cola | El worker procesa un prompt cada vez | Ver, quitar o reordenar prompts con Ctrl+K | Implementado en UI; cola volátil, no persistida entre sesiones |
| P1 | Ajustes de provider | Validación y persistencia de configuración | Panel F2 para provider/modelo/URL/variable de entorno y errores visibles | Guardar desde UI aplica el cambio al agente y lo conserva tras reiniciar |
| M2 | Subagentes y jobs | Runtime, registro, ciclo de vida, resultados y eventos | Flock/jobs reales con estado y resultados | Un subagente/job ejecuta una tarea y su ciclo se ve en la UI |
| M3 | Integración herdr | Detección de host y protocolo/socket descrito en el diseño | Render y acciones dentro del host | Ejecutar como plugin sin parchear herdr |
| M4/opcional | MCP, skills y memoria | Descubrimiento y composición de contexto en frontera de step | Mostrar fuentes y herramientas activas | La capacidad configurada llega al prompt/tool registry y se refleja en UI |
| Opcional | Voz, resumen, compresión, decisión y KPIs | Implementar módulos opcionales y sus contratos | Añadir controles y visualización solo para funciones activadas | Cada módulo tiene flujo funcional verificable y no altera el core opcionalmente |

## Dependencias entre motor y UI

```text
Composer ── prompt channel ──▶ AgentLoop ──▶ Provider / Tools
                                  │
                                  ├── SessionStore (SQLite)
                                  ├── PermissionResponder ◀── Permission dialog
                                  └── EventBus ── eventos ──▶ App / transcript / panels
                                      Delta · Usage · ToolStarted/Finished · FileDiff · Error

Esc ──▶ CancellationToken ──▶ AgentLoop / Provider stream / Tools
```

La regla de arquitectura es que `codeloom-core` no dependa de Ratatui ni Crossterm. La UI presenta estado y recoge decisiones; el motor mantiene el control del turno, permisos y ejecución.

## Fuera del MVP según el diseño

El diseño base sitúa subagentes/jobs en m2 y la integración con herdr en m3. MCP, skills, memoria, voz, compresión, decisión y KPIs aparecen en specs opcionales o fases posteriores. Se registran aquí para seguimiento, pero no deben tratarse como bloqueos para cerrar el MVP mínimo.

## Referencias de seguimiento

- [Plan de implementación MVP (m0 + m1)](../plans/2026-10-13-loom-mvp.md)
- [Diseño base de CodeLoom](../specs/2026-10-13-loom-design.md)
- [Mapa de arquitectura y specs](../specs/2026-10-14-loom-architecture-overview.md)
- [Spec de estado del turno](../specs/2026-10-15-loom-turn-status-line.md)
- [Spec de archivos modificados](../specs/2026-10-15-loom-modified-files-design.md)
- [Spec del diálogo de permisos](../specs/2026-10-15-loom-permission-dialog-design.md)

**Actualización del documento:** revisar esta tabla cuando se integre o cambie un flujo de motor/UI, y actualizar fecha, estado y criterio de cierre. La compilación por sí sola no demuestra que una interacción TUI se vea o funcione correctamente en un terminal real.
