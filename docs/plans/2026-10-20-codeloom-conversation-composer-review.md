# Revisión pendiente: transcript, composer y actividad del agente

## Objetivo

Revisar y cerrar una representación coherente de la conversación de CodeLoom,
tomando como referencias de implementación las demos locales de Crush y Martty.
Debe ser fácil seguir mensajes, llamadas a tools, jobs y subagentes durante un
turno, además de editar y enviar el prompt.

## Hallazgos del código actual

- El transcript distingue mensajes de usuario y asistente, y renderiza líneas
  con formato. Su layout virtualiza y ajusta el texto al ancho disponible.
- La TUI recibe `ToolStarted`, `ToolFinished` y mensajes con `ToolCall`/
  `ToolResult`. Conserva hasta ocho tools para el inspector; en la conversación
  visible dibuja solo `app.tools.back()` junto al composer. Así, el histórico de
  tools no queda integrado en orden dentro del transcript.
- La tarjeta de tool ya tiene estados, resumen, salida y plegado. La demo Crush
  recorre pendiente, ejecución, permiso, éxito, error y cancelación. La demo
  Martty muestra solicitud, salida y etiqueta del agente.
- Jobs y subagentes tienen estado en el core y la TUI; el encabezado presenta
  recuentos. Los avisos de finalización llegan como texto (`[Job ...] ...`), no
  como elementos estructurados de actividad dentro del transcript.
- El composer actual es multilínea y mantiene `Enter` para enviar y `Ctrl+J`
  para insertar salto. La altura se calcula con líneas lógicas; aún debe
  comprobarse que el cursor/contenido sigan visibles cuando el texto excede el
  ancho o el alto disponible.
- La presentación mezcla decisiones propias, una sugerencia de comandos con
  patrón Martty y una tarjeta de tool con patrón Crush. No se ha validado como
  una dirección visual unificada.

## Dirección propuesta

1. Usar el flujo de conversación simple de Crush como referencia para leer
   mensajes en orden.
2. Adaptar el patrón de tool de Martty para mostrar solicitud, estado,
   resultado plegable y agente asociado cuando esté disponible; conservar los
   estados de permiso y error que ya cubre la tarjeta de Crush.
3. Representar jobs y subagentes como actividad del mismo flujo, con estado y
   resultado. Evitar depender solo del contador del encabezado y de avisos de
   texto.
4. Elegir una sola referencia de composer antes de cambiar su aspecto. Resolver
   primero crecimiento, wrap/scroll y visibilidad del cursor; mantener el
   contrato actual de teclas.

Esta dirección es una recomendación basada en lectura de código, no una decisión
visual aprobada ni una especificación 1:1.

## Siguiente trabajo

- Dibujar el flujo completo del modelo actual: mensajes, tool call/result,
  permisos, job/subagente, errores y restauración de sesión. Identificar qué
  datos ya están disponibles y cuáles no llegan a la UI.
- Proponer el cambio mínimo para que las tools queden en orden cronológico con
  el transcript, sin duplicar estado ni perder el inspector existente.
- Definir el elemento de actividad para jobs/subagentes y cómo mostrar progreso,
  finalización, fallo y resultado.
- Inspeccionar el composer en terminal real con prompt vacío, multilínea,
  texto largo y terminal estrecho; contrastarlo con la referencia escogida.
- Solo después de acordar esa dirección, implementar la sección completa y
  verificar estados normal, vacío/cargando y error.

## Referencias locales

- Implementación TUI: `crates/codeloom-tui/src/render.rs`,
  `crates/codeloom-tui/src/app.rs`.
- Widgets: `crates/batten/src/widgets/transcript.rs`,
  `crates/batten/src/widgets/composer.rs`,
  `crates/batten/src/widgets/tool_item.rs`.
- Demos: `crates/batten/examples/showcase/demos/crush_chat.rs`,
  `crates/batten/examples/showcase/demos/crush_tool_call.rs`,
  `crates/batten/examples/showcase/demos/martty_tool_call.rs`,
  `crates/batten/examples/showcase/demos/crush_composer.rs`,
  `crates/batten/examples/showcase/demos/martty_composer.rs`.
- Inventario existente: `docs/references/loom-ui-comparison/code-findings.md` y
  `docs/specs/2026-10-18-loom-composer-tool-visual-references.md`.

## Límites de esta revisión

Inspección estática del código y demos locales. No se ejecutaron Crush ni
Martty, no se capturó CodeLoom en una terminal real y no se modificó ni probó
código de producción en esta revisión.
