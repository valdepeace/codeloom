# Loom — Ingeniería inversa de MastraCode y Crush

> Estado: inspección de código prioritaria completada para composer, transcript,
> herramientas y temas. Recomendación provisional: dos perfiles de presentación;
> implementación y capturas comparables siguen pendientes.
>
> Fecha: 2026-10-18.
>
> Relacionado: `docs/specs/2026-10-15-batten-composer-editing-design.md`,
> `docs/specs/2026-10-15-batten-reference-findings-showcase.md` y
> `docs/plans/2026-10-17-loom-composer-visual.md`.

## 1. Objetivo

Crear un inventario completo y verificable de las características que MastraCode
y Crush exponen al usuario, para identificar cuáles pueden inspirar CodeLoom.
El composer y la presentación de herramientas son prioritarios, dentro de un
recorrido más amplio de la interfaz y sus interacciones. Usar dos fuentes de
evidencia separadas:

1. **Inspección de código:** localizar los componentes, estados, dimensiones,
   estilos y configuración que producen la interfaz.
2. **Ejecución observada:** iniciar las versiones instaladas y registrar
   transcripciones/capturas del mismo escenario cuando el host lo permita.

El resultado incluirá una matriz de funciones, evidencia de código y capturas
de ejecución, relación con Loom y una priorización. Servirá para decidir qué
comportamientos o estilos adaptar a Loom y cuáles diferir. No implica copiar
código ni implementar automáticamente todas las funciones inventariadas.

## 2. Estado y evidencia inicial

### 2.1 Evidencia por código ya disponible

- El composer de Loom vive en `batten::state::Composer` y
  `batten::widgets::composer`; Loom decide su espacio en
  `crates/codeloom-tui/src/render.rs`. Batten ya recibe el tema para sus
  widgets. La configuración general de `codeloom-core` aún no tiene una opción
  de estilo de interfaz.
- La instalación local de MastraCode inspeccionada usa `pi-tui` `Editor`. Su
  adaptador admite entrada multilínea, submit con Enter, nueva línea con
  Shift+Enter o `\\`+Enter, wrap/scroll del editor y pistas de teclado debajo
  de algunos prompts. El adaptador elimina parte del borde del editor al
  incrustarlo en otro contenedor.
- Crush basa su entrada en `Bubbles textarea.Model`, con altura dinámica entre
  3 y 15 filas, márgenes para adjuntos/footer y layout que reduce el chat al
  crecer. El layout no compacto coloca chat y sidebar; el modo compacto cambia
  a cabecera. La sidebar reúne proyecto, modelo/contexto, archivos, LSP, MCP y
  Skills en contenido desplazable.
- Se inspeccionó el checkout local `E:\workspace\personal\crush` en el mismo
  commit completo que `vcs.revision` del binario. Aunque el binario indica
  `vcs.modified=true`, `internal/ui/` está limpio respecto al commit.
- En Crush, los renderers de chat diferencian thinking, respuesta, error y
  refusal; las herramientas separan permiso, ejecución, éxito, error y
  cancelación, con renderers por familia y opciones de expansión. Su tema
  central se basa en tokens, incluye Charmtone/Gruvbox y admite archivos JSON
  que extienden una paleta.
- La fuente empaquetada de MastraCode muestra un editor `pi-tui` custom con
  prompt dinámico/sombreado, editor multilínea con wrap/scroll, varios roles de
  tema light/dark y herramientas con vistas compactas/expandidas específicas.
  Detalle trazable en `docs/references/loom-ui-comparison/code-findings.md`.
- Estos datos describen implementación y comportamiento previsto; no prueban
  por sí solos el aspecto efectivo en un terminal, el tema aplicado ni el
  render de cada herramienta.
- REA 6.5.0 se instaló como CLI independiente. Con el runtime Node 24.21 de
  Codex se analizó una copia aislada del bundle `tui-CLkPtYAM.js` de MastraCode.
  Evidencia: `ev_52ddd92ce1fd4c2aac95ae38bc0eff8443d6295578265484b80105fdc204654b`,
  SHA-256 del artefacto `0c2de11834e3a3c4f5f67acb1c8cdc05ef38505da51d8e4b1c4dff684d0ff034`.
  Es evidencia estática de ese único bundle: no cubre el producto entero ni
  demuestra comportamiento runtime. El análisis del paquete completo no
  terminó dentro de un tiempo razonable.
- REA inspeccionó también el binario instalado `crush.exe` sin ejecutarlo:
  evidencia `ev_1079fc964bcb01da5141a7e63054f64a28e19922422fdb37e90dc18641e333c4`,
  SHA-256 `b94842f68ee90c9ae37ff860683d84712f4d4efba34ca439c6bc74253f4b6794`.
  Confirma Go `1.27.0`, arquitectura amd64 y dependencias como Bubble Tea v2,
  Bubbles v2, Lipgloss v2 y Glamour v2; no reconstruye el comportamiento de
  widgets ni la vista de la aplicación.

### 2.2 Evidencia de ejecución pendiente

Hay un registro PTY inicial de ambas aplicaciones en
`docs/references/loom-ui-comparison/runtime-observations.md`: composers vacíos
y ayudas de salida/atajos, más una llamada `view` de solo lectura en MastraCode.
Es texto del stream VT, no imagen; tampoco se fijó la misma geometría del
terminal. Queda por observar una herramienta de Crush con proveedor local,
además de edición multilínea, estados de permiso/error, temas y terminal
estrecho.

### 2.3 Uso y límites de REA

REA ya se instaló como CLI independiente y su analizador JavaScript se ejecutó
con el runtime Node 24.21 incluido por Codex. La instalación global de Node es
24.8, inferior al requisito declarado por REA; invocar `rea` con ese Node
produce aviso de motor no compatible. El análisis exitoso del bundle se hizo
con el runtime compatible sin cambiar Node activo.

Para MastraCode, REA puede mapear módulos y relaciones de sus bundles
JavaScript. Para Crush, escrito en Go, el análisis de binario aporta
principalmente metadatos de compilación y módulos; los widgets y su lógica se
deben investigar en su código Go fuente. REA no sustituye esa lectura ni las
capturas de terminal.

REA dispone de `capture-process` para terminal virtual, frames renderizados,
cursor, salida y reproducción de eventos en hosts compatibles. En Windows su
adaptador está deshabilitado porque no puede verificar la limpieza de procesos
descendientes. El intento con un `cmd /c echo` inocuo terminó antes de lanzar
el hijo. Para las builds Windows instaladas, las capturas runtime deberán usar
un terminal/PTY compatible fuera de REA y documentar el método; REA sigue
siendo útil para análisis estático. En sistemas compatibles, `capture-process`
es una alternativa reproducible, pero no genera PNG del terminal físico. No
ejecutar `rea setup` ni registrar MCP/skills en la configuración global; invocar
el CLI con el runtime compatible ya disponible.

## 3. Alcance e inventario de funciones

- Auditar versiones concretas de ambos productos, código, documentación y
  configuración disponible.
- Catalogar las capacidades visibles y los flujos principales: entrada y
  edición, conversación/transcript, herramientas y permisos, navegación y
  sesiones, contexto/archivos, configuración, estado del agente y extensiones.
- Capturar flujos representativos en ejecución, incluyendo composer y estados
  de herramientas.
- Mapear cada función a CodeLoom: existente, adaptable, posterior (m2+) o fuera
  de encaje; anotar evidencia, valor visual/funcional, coste/riesgo y prioridad.
- Recomendar qué adoptar primero y cómo exponer variantes visuales en la
  configuración de Loom, si la evidencia lo justifica.

## 4. Fuera de alcance

- Implementar las funciones inventariadas o perfiles de estilo.
- Copiar código fuente o dependencias de MastraCode/Crush.
- Modificar configuraciones globales de esos programas o del modelo local.
- Usar servicios externos, ejecutar cambios sobre repositorios reales o aceptar
  comandos sugeridos por un modelo.
- Tratar una captura aislada como prueba de compatibilidad entre todos los
  terminales.

## 5. Método de comparación

### A. Inspección de código

Para cada aplicación anotar versión/commit, licencia del proyecto, toolkit y
archivos fuente o módulos inspeccionados. Recorrer sus capacidades por áreas
(§3) y seguir las funciones principales desde controles/modelos de estado hasta
render y configuración. En las herramientas, incluir estados, argumentos,
salida, error, permisos y plegado/expansión.

Separar explícitamente:

- comportamiento definido por código;
- estilos/tokens configurables;
- aspecto que solo se puede confirmar en ejecución;
- ideas transferibles a Loom, sin copiar implementación.

Se puede usar REA para orientar la búsqueda de módulos en la instalación
JavaScript de MastraCode. Contrastar cada resultado con el bundle o el código
fuente correspondiente. En Crush, priorizar sus componentes Go desde código
fuente; no interpretar metadatos del binario como explicación del layout.

### B. Captura de ejecución

Usar las versiones instaladas localmente y registrar sus versiones y ajustes
visuales activos. En este host Windows, usar el stream VT de una sesión PTY para
transcripciones: REA `capture-process` está deshabilitado y no se consiguió
screenshot físico. En hosts compatibles, REA puede guardar frames de terminal
virtual, cursor y secuencia de entradas. Para no convertir la captura en un recorrido ilimitado,
elegir escenarios que cubran las familias de funciones: inicio/sesión,
composer vacío y extendido, conversación, herramienta de solo lectura,
resultado/error/permiso si se puede reproducir con seguridad, navegación y
configuración visual. Guardar capturas con el mismo tamaño de terminal cuando
sea posible; registrar resolución, tema, comando de inicio y pasos de
reproducción. El inventario estático puede ser más amplio que el conjunto de
capturas.

No ejecutar una herramienta que escriba o modifique archivos. Si alguna
aplicación requiere un proveedor/modelo para mostrar la herramienta, usar solo
el endpoint local ya configurado y un fixture desechable. Si no es posible
obtener el mismo estado de forma segura, dejar ese punto como no comparable en
vez de inventar una captura.

## 6. Recomendación provisional basada en código

Ofrecer dos perfiles seleccionables inspirados en MastraCode y Crush. La razón
es visible en el código: difieren en estructura, composición, densidad,
tratamiento de mensajes/herramientas y tokens; dos paletas sobre el render actual
no reproducirían ambas interfaces. La matriz y trazabilidad están en
`docs/references/loom-ui-comparison/code-findings.md`.

El perfil afecta layout, prompt/editor, header/sidebar/footer, superficies,
espaciado y render de transcript/tools. `App`, estado, permisos, herramientas y
atajos permanecen compartidos. El estilo actual de Loom puede continuar como
perfil propio posterior.

Esta conclusión es de inspección estática. Antes de fijar valores finales de
colores, columnas, espaciado y estados del editor, comparar capturas reales a
igual resolución; las capacidades del terminal y el estado de la sesión forman
parte del resultado visible. Aún no implementar el selector ni declarar
equivalencia 1:1.

## 7. Criterios pendientes de esta investigación

- [ ] Evidencia de código y observaciones runtime están en secciones separadas.
- [ ] Versiones, entorno y pasos para reproducir cada observación están anotados.
- [ ] Existe un inventario de las funciones visibles de ambos productos,
  agrupadas por área y sustentadas en fuentes.
- [ ] Se comparan composer y estados/resultados de herramientas en código y
  observaciones runtime equivalentes.
- [ ] Cada función está mapeada a Loom y priorizada; las funciones futuras de
  Loom se marcan m2+ o fuera de alcance del MVP.
- [ ] Se distinguen observaciones, inferencias y aspectos no verificados.
- [x] Se presenta una recomendación concreta sobre perfiles configurables, sin
  implementarlos en esta tarea.
