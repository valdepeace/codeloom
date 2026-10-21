# Lista de tareas y preguntas al usuario (`todo_write`, `ask_user`)

Estado: aprobada por el usuario en conversación el 2026-10-20. Plan:
`docs/plans/2026-10-20-codeloom-todos-and-questions.md`.

## Intención

Hoy, cuando el modelo responde con varias preguntas o con una lista de tareas,
todo llega como markdown: se lee, pero no es interactivo ni persiste. Se añaden
dos herramientas del core con presentación propia en la TUI:

- `todo_write`: el modelo mantiene una lista de tareas visible y viva.
- `ask_user`: el modelo plantea de 1 a 4 preguntas con opciones y espera las
  respuestas del usuario en un diálogo.

## Objetivos

1. `todo_write` reemplaza la lista completa en cada llamada (elementos
   `{content, status}` con estado `pending`, `in_progress` o `completed`;
   máximo 50; lista vacía permitida para borrarla).
2. La lista se ve como un elemento del transcript que se actualiza en su sitio
   (look Crush) y como sección TODOS del rail, con progreso `hechas/total`.
   Se restaura al reabrir la sesión releyendo la última llamada a `todo_write`
   del historial, sin tabla nueva.
3. `ask_user` abre un diálogo modal con las preguntas; cada una admite
   selección única o múltiple y siempre una opción final «Other…» de texto
   libre. Bloquea la herramienta (no el proceso) hasta que el usuario contesta
   o cancela.
4. Las respuestas vuelven al modelo como resultado de la herramienta, una línea
   por pregunta. Esc cancela y el modelo recibe un error indicando que el
   usuario no contestó.

## No objetivos

- Editar la lista de tareas a mano desde la TUI.
- Mostrar tareas en el look Classic más allá de la tarjeta de la herramienta.
- Preguntas encadenadas entre subagentes.
- Persistir respuestas parciales de un diálogo si se cierra la aplicación.

## Anti-objetivos

- Que un subagente pueda bloquear la interfaz del padre (sus preguntas van al
  padre, no a la pantalla; véase la nota sobre subagentes).
- Que el diálogo robe teclas al de permisos: el de permisos tiene prioridad.
- Que una lista de tareas enorme o una respuesta libre larguísima rompan el
  layout o ralenticen el dibujado.

## Decisiones de diseño

- **Sin cambios en `ToolCtx`.** Las preguntas viajan por el responder de
  permisos: `PermissionResponder` gana `ask(&[Question]) -> Option<Vec<Answer>>`
  con implementación por defecto `None`, y `PermissionService::ask_user` lo
  expone a las herramientas. Los responders no interactivos (`AllowAll`,
  `DenyAll`) devuelven `None`.
- **Un evento por lista.** `LiveEvent::TodosChanged { session, items }`; la TUI
  solo acepta los de la sesión abierta, de modo que cada subagente tiene su
  lista y el padre no ve la del hijo.
- **Un solo elemento de tareas en el transcript**, creado en la primera llamada
  y actualizado en su sitio (decisión mía; el rail muestra siempre el estado
  actual). La herramienta `todo_write` no genera su propia tarjeta en el
  transcript.
- **Máquina del diálogo pura en `batten`** (`QuestionDialog`, sin dependencia
  del core): la TUI traduce teclas y datos. Se reutiliza la mecánica de cola y
  `oneshot` del diálogo de permisos.
- **`ask_user` es `Exclusive`**: no hay dos diálogos de preguntas a la vez.
- Límites: 4 preguntas, de 2 a 6 opciones cada una, 50 tareas.

## Qué se deja al implementador

Distribución exacta del diálogo, textos de ayuda y nombres internos que el plan
no fije.

## Verificación

Tests de parseo y límites, de la máquina del diálogo (flujos único, múltiple,
«Other…», navegación, cancelación), de eventos y restauración de sesión, y de
render en varios tamaños. Validación visual en terminal real: pendiente.

## Estado (2026-10-21)

Implementado en el árbol de trabajo, sin commitear: `todo_write` y `ask_user`
en el core, `todo_lines` y `QuestionDialog` en `batten`, y su integración en la
TUI. 504 tests, clippy `-D warnings` y rustfmt limpios.

Decisiones tomadas al implementar:

- Con un diálogo de preguntas abierto, Esc lo cancela y no cuenta para la
  salida por doble Esc (Ctrl+C sigue saliendo). Con permisos abiertos, nada
  cambia.
- El pegado y el ratón se ignoran mientras hay un diálogo de preguntas.
- Cancelar el turno activo cierra también las preguntas pendientes.
- En selección única, elegir una opción descarta el texto de «Other…» y
  viceversa.
- El diálogo de preguntas no usa el efecto de aparición (`fx_pending`) de los
  permisos.

### Nota sobre subagentes (2026-10-21)

`ask_user` es el análogo local del estado A2A `input-required` (véase la §5 de
la spec de agentes en `2026-10-15-loom-agent-profile-a2a.md`). Los subagentes
**sí** disponen ya de la herramienta: su pregunta no llega a la pantalla, sino
al buzón de su padre, cuyo LLM responde con `subagent_control` (acción
`answer`) o escala al usuario con su propio `ask_user`. El diseño completo está
en `2026-10-21-codeloom-subagent-input-required-design.md`; cabe señalar que el
anti-objetivo de arriba se mantiene, pues el hijo nunca bloquea la interfaz.
