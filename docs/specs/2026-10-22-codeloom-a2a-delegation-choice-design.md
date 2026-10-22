# Elección del especialista al delegar (A12)

Estado: propuesta del 2026-10-22 (tarea A12 de `docs/plans/2026-10-22-codeloom-a2a-roadmap.md`).
Continúa `2026-10-22-codeloom-a2a-agent-registry-design.md` (A13) y
`2026-10-22-codeloom-a2a-agent-card-design.md` (A6).

## Intención

Con el registro de agentes (A13), la herramienta `subagent` acepta un argumento `agent`,
y el padre ya ve en su prompt la lista de agentes disponibles. Falta decidir **quién elige
y cómo se entera la persona usuaria**. La decisión de fondo es de quien usa el programa:
el padre propone, y **pregunta cuando duda entre varios**; cuando uno encaja con claridad,
delega sin molestar. Lo que hace el padre debe poder verse: observabilidad es el valor
central del producto.

## Qué dice A2A

No consta. El protocolo describe tarjetas y tareas, pero no fija cómo un cliente escoge a
qué agente enviar una tarea (spec del perfil, §4, y hoja de ruta, A12). Por tanto, todo lo
de esta spec es **política local de CodeLoom**, y así se rotula en la interfaz y en los
documentos. No se inventa ningún campo del protocolo.

## Decisiones

### De la persona usuaria (2026-10-22)

- **El padre pregunta, sobre todo cuando duda entre varios.** Si un solo agente encaja, o
  solo existe `default`, delega sin preguntar y lo dice en su texto, con el motivo.
- **El clasificador queda para más adelante** y se considera interesante. Esta spec debe
  dejar el hueco, sin construirlo.

### Lecturas nuestras (rotuladas como tales)

- **La pregunta usa `ask_user`**, que ya existe y ya abre el diálogo de preguntas de la
  interfaz. No se crea un mecanismo nuevo.
- **La regla va en el prompt del padre**, junto a la sección «Available agents» que ya
  genera `RegistrySnapshot::prompt_section`. Es una petición al modelo, **no una garantía**:
  un modelo puede incumplirla. Se declara así, y el comportamiento se observa en el
  transcript (la pregunta, la respuesta y el agente lanzado quedan a la vista).
- **Opción de configuración** `[agents] confirm_delegation` con tres valores:
  `"doubt"` (por defecto: pregunta si duda), `"always"` (propone siempre la asignación y
  espera), `"never"` (delega sin preguntar y lo cuenta). Cambia únicamente el texto de la
  regla del prompt; es determinista y comprobable por test.
- **Qué agente lleva cada hijo se muestra en la interfaz, siempre.** Hoy el descriptor guarda
  `agent` y `agent_origin` y nadie los pinta. El rail y la cabecera del subagente nombran el
  agente (`front`, `default`…), con el origen cuando es de proyecto o remoto. Es
  observabilidad pura, sin lógica nueva; la persona usuaria la declaró vital.
- **Hueco para el clasificador.** La regla se construye en un único lugar (el generador de
  la sección del prompt). Un enrutador futuro podría preseleccionar un agente y el padre
  preguntaría solo con confianza baja; esta spec **no** lo implementa. Se conserva la
  consigna «regla antes que clasificador».

## Qué se construye

1. Tipo `ConfirmDelegation { Doubt, Always, Never }` en `codeloom-core::config`, leído de
   `[agents] confirm_delegation`; valor ausente = `Doubt`; valor desconocido = error de
   configuración con mensaje claro.
2. `RegistrySnapshot::prompt_section` recibe el modo y añade la regla correspondiente.
   Redacción de la regla (inglés, como el resto del prompt): en `Doubt`, «if more than one
   agent could fit a task and you are not sure, ask the user with `ask_user`, naming the
   agent you would pick and why, before calling `subagent`; if one clearly fits, delegate
   and say which agent you chose and why»; en `Always`, propone la asignación de todas las
   tareas y espera la respuesta; en `Never`, delega y cuenta la elección.
3. `AgentLoop::with_delegation_confirm(mode)` y cableado en `codeloom-cli/src/main.rs`.
4. La TUI muestra el agente de cada hijo en el rail y en su detalle, con el origen cuando no
   es `built-in`.

## No objetivos

- Clasificador o enrutador automático (queda anotado como A12b).
- Reglas por etiquetas (`tags` de las skills): las skills de la tarjeta sintetizada van
  vacías todavía.
- Garantizar por código que el padre pregunta. Si hiciera falta, sería un cambio posterior:
  que `subagent` rechace el lanzamiento sin una aprobación registrada.
- Persistir el «motivo» de la elección. Vive en el texto del padre y en la respuesta de la
  persona usuaria, no en un campo.
- Efectos de `model` y `effort` por perfil: ya los resuelve A13.

## Anti-objetivos

- Que el padre pregunte por cada subagente trivial: sería ruido. En `Doubt`, un único
  candidato claro o solo `default` no genera pregunta.
- Presentar la elección del padre como algo del protocolo A2A.
- Interferir con la fluidez del composer: nada de esto toca el bucle de entrada.

## Qué se deja al implementador

Redacción exacta de las tres reglas dentro de lo fijado arriba, el formato visual del agente
en el rail (siempre con trazabilidad: el estado A2A del hijo no se altera) y el nombre de los
tests.

## Verificación

- Configuración: ausente = `Doubt`; los tres valores se leen; uno desconocido da error.
- `prompt_section` contiene la regla propia de cada modo y no las de los otros.
- Extremo a extremo con el proveedor espía: el prompt de sistema que recibe el modelo lleva
  la regla del modo configurado.
- TUI: un hijo con agente `front` lo muestra en el rail y en su cabecera; uno con `default`
  también (decisión de la persona usuaria, 2026-10-22: la observabilidad es vital, así que el
  agente se muestra siempre que se conozca); uno sin agente conocido no muestra nada.
- Declarado: el cumplimiento de la regla por el modelo **no se puede probar** sin un modelo
  real; solo se comprueba que llega al prompt.

## Preguntas abiertas

- ¿El modo `Always` debería valer por sesión (conmutable con un comando) en vez de por
  configuración? Aquí queda por configuración.
- Cuándo y cómo entra el clasificador (A12b) y con qué señal de confianza.

## Estado (2026-10-22)

**Implementado.** Plan: `docs/plans/2026-10-22-codeloom-a2a-delegation-choice.md`. Commits:
`7cd4aab` (modo `[agents] confirm_delegation` en la configuración), `1c12360` (la regla de
elección en el prompt del padre, `with_delegation_confirm` y cableado en `main.rs`) y
`c71f53e` (etiqueta del agente de cada hijo en el rail y en la cabecera de actividad).

**Cómo se usa.** Sin tocar nada, el modo es `doubt`: el padre delega sin preguntar cuando un
agente encaja con claridad y pregunta con `ask_user` cuando duda entre varios. Para cambiarlo,
en `codeloom.toml`:

```toml
[agents]
confirm_delegation = "always"   # "doubt" (por defecto), "always" o "never"
```

**Decisiones tomadas al implementar** (además de las de arriba):

- La etiqueta se muestra **siempre** que el agente se conozca, incluido `default`, por la
  decisión de la persona usuaria sobre la observabilidad (corrige el borrador inicial, que no
  la mostraba para `default`). Formato: `[front]`, y `[front·project]` o `[front·remote]` cuando
  el origen es de proyecto o remoto. En el rail va tenue antes del nombre; en la cabecera de
  actividad, tras el nombre, y **el estado A2A entre paréntesis sigue siendo lo último**.
- Un nombre de agente vacío se trata como desconocido, para no pintar `[]`.
- En la aplicación real el registro siempre está cableado, así que una delegación sin `agent`
  queda registrada como `default` (origen `built-in`) y su etiqueta se ve.

**Limitaciones reales:**

- **La regla del prompt no es una garantía.** Un modelo puede no preguntar cuando debía, o
  preguntar de más. Solo se comprueba por test que la regla llega al prompt; su cumplimiento no
  se puede probar sin un modelo real. Lo que sí queda a la vista es lo ocurrido: la pregunta, la
  respuesta y el agente de cada hijo.
- No se muestra el **motivo** de la elección como dato: vive en el texto del padre.
- Un hijo creado antes de A13, o lanzado sin registro, no tiene agente y no muestra etiqueta.
- La etiqueta puede recortarse en un rail muy estrecho, como el resto de sus líneas. No se ha
  visto en una terminal real.
- El modo es global y fijo por configuración; no hay comando para cambiarlo en la sesión.

**Pendiente:** A12b (clasificador o enrutador) y la pregunta abierta sobre un comando de sesión.

### Adenda (2026-10-22): el transcript es el diario de toda llamada

Se detectó en uso real que, con el look clásico (`ToolStyle::Pinned`, el de `theme = "batten"`),
**ninguna llamada a herramienta dejaba rastro en la conversación**: solo la última se veía en una
tarjeta fija junto al composer, y los subagentes y jobs tampoco aparecían. La persona usuaria lo
declaró inaceptable: el transcript es el diario de toda iteración, y al ser varios actores debe
verse quién es quién y qué hace.

Cambio: con el look compacto, cada llamada a una herramienta y cada elemento de actividad
(subagente o job) deja **una línea de cabecera** en su sitio cronológico, y esa línea corta el
texto del asistente, de modo que lo que diga después va debajo de la llamada. El cuerpo largo
sigue en la tarjeta fija y en el inspector (Ctrl+T). La llamada a `subagent` nombra el agente
elegido en su propia cabecera: `subagent  [rust] leer fichero`. Se reescribieron seis tests que
defendían la regla anterior («el look clásico no deja rastro»), por decisión expresa.

**Resuelto después:** el aviso de fin de un subagente o de un job (`[Subagent … completed]`)
entraba en el transcript como mensaje de **usuario** y aparecía rotulado `You`, como si lo
hubiera escrito la persona usuaria. Ahora, con el look clásico, se conserva entero pero como
línea de **sistema** (`LineKind::System`), sin la etiqueta `You`; su cabecera nombra al actor.
**Sigue abierto:** esa línea de sistema no lleva etiqueta propia (`Subagent`) ni color
distintivo; solo deja de atribuirse a la persona usuaria.
