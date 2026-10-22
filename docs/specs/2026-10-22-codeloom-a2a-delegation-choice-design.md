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
- **Qué agente lleva cada hijo se muestra en la interfaz.** Hoy el descriptor guarda
  `agent` y `agent_origin` y nadie los pinta. El rail y el detalle del subagente nombran el
  agente (`front`, `default`…). Es observabilidad pura, sin lógica nueva.
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
- TUI: un hijo con agente `front` lo muestra en el rail; uno con `default` y origen
  `built-in` no añade ruido.
- Declarado: el cumplimiento de la regla por el modelo **no se puede probar** sin un modelo
  real; solo se comprueba que llega al prompt.

## Preguntas abiertas

- ¿El modo `Always` debería valer por sesión (conmutable con un comando) en vez de por
  configuración? Aquí queda por configuración.
- Cuándo y cómo entra el clasificador (A12b) y con qué señal de confianza.
