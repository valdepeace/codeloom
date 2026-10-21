# Observabilidad de agentes, Fase O2: entrar en la sesión de un subagente

Estado: propuesta del 2026-10-21 (continuación de la línea de
`2026-10-21-codeloom-agent-observability-design.md`; la fase O1 está hecha).
Plan: `docs/plans/2026-10-21-codeloom-agent-observability-o2.md`.

## Intención

Tras O1 el rail muestra las métricas de cada subagente, pero no se puede ver
qué hizo. Cada hijo tiene ya una sesión real en el almacén (el lanzador la crea
con `create_session` y persiste sus mensajes), de modo que «entrar» en un hijo
es abrir esa sesión con el mecanismo que ya existe (`SessionChoice::Resume`),
más una forma de volver al padre.

## Objetivos

1. Con el foco en el rail, seleccionar un subagente con `↑`/`↓` y abrir su
   transcript con `Enter`.
2. Volver al padre con `Alt+←` o con el comando `/parent`, restituyendo la
   sesión desde la que se entró (pila de padres, admite hijos de hijos).
3. Que se vea en el rail que se está dentro de un subagente y cómo volver.

## No objetivos

- Árbol multinivel dibujado (hoy el rail lista los hijos directos de la sesión
  abierta; al entrar en un hijo se listan los suyos, lo que ya da la navegación
  por niveles).
- Transcript vivo de un hijo en marcha: la sesión se carga del almacén en el
  momento de entrar, sin reenviar `Delta` (sigue siendo no objetivo de O1).
- Causalidad y artefactos (Fase O3).
- Bloquear la escritura en la sesión de un hijo: es una sesión ordinaria y
  puede reanudarse.

## Anti-objetivos

- Alterar el flujo de la selección de sesiones normal: abrir una sesión desde el
  selector vacía la pila de padres.
- Perder el foco del composer o añadir trabajo al bucle de dibujado.
- Cambiar de sesión con un turno en curso (misma regla que `/sessions`).

## Decisiones de diseño (mías, marcadas para revisión)

- `App.rail_selected: Option<i64>` (id del hijo resaltado) y
  `App.parent_stack: Vec<SessionId>`.
- `App::open_child(child)` apila la sesión actual y emite
  `SessionChoice::Resume(child)`; `App::go_parent()` desapila y emite
  `Resume(padre)`. Ambos se niegan con un turno en curso. Un indicador
  `nav_keep_stack` evita que `load_session` vacíe la pila durante esa
  navegación; cualquier otra carga la vacía.
- Con foco en `Rail` y al menos un subagente, `↑`/`↓` mueven la selección y
  `Enter` abre; sin subagentes, las teclas siguen actuando sobre el composer
  como hasta ahora. `Alt+←` y `/parent` valen con cualquier foco.
- El marcador de selección del rail es `›` en el color de acento; dentro de un
  hijo el rail añade bajo el título la línea `↩ /parent · Alt+←`.

## Qué se deja al implementador

Recorte de la línea del rail por anchura y nombres internos no fijados en el
plan.

## Verificación

Pruebas de `open_child`/`go_parent` (pila, negativa con turno en curso, la
carga normal vacía la pila), de las teclas del rail y del dibujado del
marcador y de la línea de retorno. Validación visual en terminal real: pendiente.

## Estado

Implementado (tareas 1 y 2 del plan):

- Navegación entre padre e hijo: `open_child`, `go_parent`, pila de padres,
  `Alt+←` y `/parent`, con las negativas previstas (turno en curso, pila vacía,
  hijo desconocido).
- Rail: `↑`/`↓` y `Enter` con el foco en `Rail`; el hijo elegido lleva el
  marcador `›` de acento solo con el foco en el rail; dentro de un hijo aparece
  bajo el título la línea `↩ /parent · Alt+←`, recortada a la anchura del rail.
- Ayuda (`F1`): lista `↑/↓ + Enter` y `Alt+←/parent`. El recuadro pasó de 19 a
  21 filas máximas para que quepan las dos líneas nuevas; en terminales de 19 o
  20 filas la ayuda muestra ahora el aviso de tamaño en lugar de recortarse.

Decisiones tomadas en la implementación:

- No existe una sección «rail» en la ayuda; las dos líneas nuevas se añaden a la
  lista general de atajos, junto a `Tab/Shift+Tab`.
- La línea de retorno ocupa la fila tras el `cwd`, no una sección propia.

Limitaciones conocidas:

- El transcript de un hijo no está vivo: se carga del almacén al entrar.
- El rail no dibuja árbol; solo lista los hijos directos de la sesión abierta.
- La validación visual en una terminal real sigue pendiente.
