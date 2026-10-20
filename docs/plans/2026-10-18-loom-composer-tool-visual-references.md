# Plan: inventariar MastraCode y Crush para CodeLoom

> Spec: `docs/specs/2026-10-18-loom-composer-tool-visual-references.md`.
>
> Alcance: inventario amplio de funciones de cara al usuario, con análisis de
> código y observaciones de ejecución separados. Sin implementar funciones.

## Restricciones

- No cambiar código de producción, dependencias ni configuración global.
- No instalar ni actualizar MastraCode, Crush o el modelo.
- REA 6.5.0 está instalado como CLI independiente. No ejecutar `rea setup`, no
  registrar el MCP ni instalar skills en ningún agente: el runtime global de
  Node es 24.8 y el CLI requiere 24.11+ (o 22.19+); para usar REA se invoca con
  el Node 24.21 compatible de Codex.
- No ejecutar herramientas con escritura en un repositorio real.
- Capturar evidencia; no afirmar validación visual basándose solo en tests o
  lectura de código.
- Guardar los artefactos bajo `docs/references/loom-ui-comparison/` y no añadir
  binarios pesados al repositorio. Si las capturas no son apropiadas para Git,
  anotar la ruta local y dejar una descripción accesible en el informe.

## Tarea 1 — Inspección por código

- [x] Confirmar versiones instaladas: MastraCode `0.47.1`, Crush
  `v0.98.2-0.20261017181040-852f410573e1+dirty`, REA `6.5.0`.
- [x] Fijar el bundle MastraCode local y el SHA de Crush estampado en el
  binario; cotejar el checkout local de Crush contra el SHA completo y separar
  los archivos UI limpios del `+dirty` ajeno a `internal/ui/`.
- [x] Comprobar que el CLI REA arranca sin cambiar configuraciones de agentes;
  usar el Node 24.21 de Codex, ya que el Node global 24.8 no cumple el engine.
- [x] Instalar REA 6.5.0 como CLI independiente y comprobar que arranca con el
  Node 24.21 de Codex, sin integrar MCP ni skills.
- [x] Analizar con REA una copia aislada de `tui-CLkPtYAM.js` de MastraCode;
  registrar la evidencia `ev_52ddd92ce1fd4c2aac95ae38bc0eff8443d6295578265484b80105fdc204654b`.
- [x] Inspeccionar con REA el build info del `crush.exe` instalado; registrar
  la evidencia `ev_1079fc964bcb01da5141a7e63054f64a28e19922422fdb37e90dc18641e333c4`.
- [x] Obtener un resumen acotado del código TUI. Para MastraCode se usó el
  source map local, que incluye TypeScript fuente; para Crush, el checkout
  `E:\workspace\personal\crush` coincide con el SHA estampado en el binario.
  REA no fue necesario para sustituir estas fuentes directas.
- [ ] Inventariar las capacidades visibles en ambas aplicaciones: inicio y
  sesiones, composer, transcript, herramientas/permisos, navegación,
  contexto/archivos, configuración/temas, estados del agente y extensiones.
- [x] Trazar en código composer/editor y layout: widget, crecimiento, wrap/scroll,
  cursor, borde y ayudas relevantes. Comparación runtime/capturas sigue
  pendiente en Tarea 2.
- [x] Trazar en código la presentación de herramientas/mensajes: estados,
  argumentos, salida, error y expansión/colapso. Comparación runtime/capturas
  sigue pendiente en Tarea 2.
- [x] Identificar temas/opciones visuales a nivel de código en ambas builds;
  `internal/ui/` de Crush está limpio en el checkout del SHA exacto pese a que
  el binario informa `+dirty`.
- [x] Resumir hechos, inferencias y diferencias con la arquitectura actual de
  Loom para el alcance visual prioritario. El inventario funcional completo y
  las etiquetas de roadmap siguen pendientes.
- [x] Registrar la conclusión provisional de que hacen falta perfiles de
  presentación con layout y tokens, no solo dos paletas. Ver
  `docs/references/loom-ui-comparison/code-findings.md`.
- [x] Anotar módulos, builds, hashes y rutas fuente para la evidencia visual.
- [x] Para Crush, inspeccionar desde el código exacto los componentes de layout,
  editor, sidebar, estilos, transcript y tools; los metadatos del binario por sí
  solos no se usan para explicar el render.

**Entrega:** matriz de UI prioritaria “Inspección por código”, con categoría,
función, evidencia/versiones, relación con Loom y límites de confianza; separada
de las observaciones visuales. El inventario funcional amplio sigue abierto.
Actualizar
`docs/references/loom-ui-comparison/feature-inventory.md` como catálogo central.

**Límite de REA:** no ejecutar `rea setup` ni registrar MCP/skills en la
configuración global en esta tarea. Si el CLI no está disponible en el runtime
compatible, seguir la inspección por fuentes y módulos instalados.

## Tarea 2 — Capturas de ejecución

- [ ] Registrar comando de inicio, versión, terminal, dimensiones, tema y
  configuración visual activa de cada aplicación.
- [x] Probar disponibilidad de `capture-process` en Windows con un escenario
  `cmd /c echo` inocuo: REA no lanza el hijo porque el adaptador Windows no
  garantiza limpieza de descendientes.
- [x] Elegir el stream VT de las sesiones PTY abiertas por el runner como
  método disponible en Windows; documentar que no genera PNG ni conserva color.
- [x] Capturar el composer vacío de ambas aplicaciones y las ayudas de
  navegación/salida mediante sesiones PTY. Guardar transcripciones y límites en
  `docs/references/loom-ui-comparison/runtime-observations.md`.
- [x] Capturar en MastraCode una llamada local de solo lectura a `view` sobre un
  README temporal; el resultado del tool aparece en el transcript.
- [ ] Capturar el composer multilínea/largo y cursor en ambas TUI.
- [ ] Recorrer flujos adicionales de forma no persistente. El `/help` local de
  MastraCode ya se registró en el inventario; falta verificar interacciones.
- [ ] Recorrer las áreas principales que cubren el inventario, sin activar
  cuentas, proveedores externos, herramientas con escritura ni cambios de
  configuración persistentes.
- [ ] Registrar composer con contenido multilínea/largo y cursor. La evidencia
  PTY actual no conserva imagen ni color.
- [ ] En un fixture temporal y aislado, mostrar en Crush una herramienta de
  solo lectura con salida breve usando solo el proveedor local; capturar estados
  y resultado. No llamar al proveedor OpenRouter observado en la config.
- [ ] Capturar un error únicamente si puede provocarse sin llamadas externas ni
  cambios persistentes.
- [ ] Repetir a tamaño reducido si es reproducible; anotar cualquier diferencia
  que impida comparación equivalente.
- [ ] Guardar las transcripciones/capturas disponibles y pasos de reproducción;
  limpiar solo el fixture temporal creado por esta tarea.

**Entrega:** transcripciones etiquetadas del stream VT (REA no puede capturar
procesos TUI en este host Windows) y sección “Captura de ejecución” del informe,
con cobertura de las áreas alcanzables y limitaciones declaradas. No llamar
capturas de pantalla a los registros textuales.

En Linux/macOS, REA `capture-process` puede aportar frames de terminal virtual,
cursor y secuencia de eventos. En este host Windows está deshabilitado por la
limpieza de procesos descendientes; capturar las aplicaciones con otro método.

## Tarea 3 — Recomendación

- [ ] Comparar explícitamente composer y visualización de herramientas usando
  las dos fuentes de evidencia.
- [ ] Completar el mapeo de cada función a: ya está en Loom, adoptar en m1,
  considerar en m2+, o descartar/no aplica.
- [ ] Recomendar un orden de trabajo que cubra el objetivo visual de composer y
  herramientas antes de ampliar al resto.
- [x] Recomendar perfiles de presentación configurables, ya que las diferencias
  abarcan la geometría y la densidad, además de la paleta.
- [x] Enumerar qué cambia: prompt/editor, áreas y proporciones de layout,
  sidebar/header/footer, fondos/superficies, espaciado y render de transcript y
  tools; estado/teclas/lógica siguen compartidos.
- [ ] Actualizar la spec con decisión, limitaciones y cuestiones pendientes.
- [ ] No implementar la selección de estilo; abrir una tarea de implementación
  posterior solo si la evidencia y la decisión lo justifican.

**Entrega:** recomendación y spec actualizada. Sin cambios de producción.

La matriz inicial está en `docs/references/loom-ui-comparison/feature-inventory.md`;
no darla por exhaustiva hasta cerrar la inspección de módulos y capturas.

## Verificación

- Revisar que cada afirmación esté etiquetada como observada por código,
  observada en captura o inferida.
- Verificar que cada artefacto runtime corresponde a las versiones anotadas.
- Confirmar que no se modificaron configuraciones globales ni archivos fuera
  del directorio temporal creado para el escenario.
