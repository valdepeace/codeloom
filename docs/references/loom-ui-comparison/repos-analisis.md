# Repositorios de referencia para la UI de agente

Inspección inicial del código descargado el 2026-10-18. Los clones son de solo
lectura y están fuera del repositorio de Loom:

`%TEMP%\loom-ui-references-20261018\`

Se usó `git clone --depth 1`; los SHA fijan exactamente qué snapshot se revisó.

| Proyecto | Repositorio | Snapshot | Licencia declarada | Motivo para estudiarlo |
|---|---|---|---|---|
| Martty | <https://github.com/openma-ai/Martty> | `bf66b4a3dfbcebbfe1d91e76c5af39d63f921337` | MIT | Cliente ACP en Ratatui con transcript de reasoning y herramientas, eventos en streaming y expansión individual o global. |
| Mindfork | <https://github.com/vshylov/mindfork-rs> | `5f18fceb0cdcfb4052583d26902b7e60c0fce1e4` | MIT | Chat en Ratatui; feed que conserva pensamientos y llamadas de herramientas, con estado plegable y capturas generadas desde la propia UI. |
| Pawan | <https://github.com/dirmacs/pawan> | `e15a5d22f1d06b1a1643c3d9757816d0af0ea08b` | MIT | Ejemplo más directo de tarjetas compactas de tool: el README/changelog describe tarjetas enmarcadas, 6 líneas plegadas y 40 desplegadas, además de thinking animado. |
| Crush | <https://github.com/charmbracelet/crush> | `852f410573e199fa0235cd8a8f32d6b0840373d3` | FSL-1.1-MIT; MIT futura al segundo aniversario | Aplicación Go/Bubble Tea con sidebar de contexto, modo compacto, editor dinámico y renderers especializados para thinking y herramientas. |

## Archivos para empezar

- **Martty** (`Martty/`): `Cargo.toml`, `src/transcript.rs`, `src/events.rs`,
  `src/app.rs`. Usa `ratatui 0.30.2` y `ratatui-textarea 0.9.2`. En
  `transcript.rs` están `TOOL_VIEWPORT = 4`, los tipos de celda Tool/Reasoning,
  su render y el consumo de eventos. `app.rs` gestiona el toggle de expansión y
  el atajo para expandir/plegar todo. Es la referencia más completa para el
  flujo evento → modelo de transcript → render.
- **Mindfork** (`mindfork-rs/`): `Cargo.toml`, `src/widgets/message_feed.rs`,
  `src/screens/chat/feed.rs`, `src/screens/chat/render.rs`. El widget feed
  combina contenido del asistente, thoughts y tool calls; el estado de plegado
  vive en el feed de la conversación. Su README incluye capturas hechas por la
  propia aplicación. Es buena referencia para persistencia/agrupación del feed
  y disciplina de validación visual.
- **Pawan** (`pawan/`): `Cargo.toml`, `crates/pawan-cli/src/tui/types.rs`,
  `crates/pawan-cli/src/tui/app/async_ops.rs`, `crates/pawan-cli/src/tui/render/`.
  Define `ContentBlock::ToolCall { ..., expanded }`, actualiza la tarjeta al
  completar la ejecución y muestra el indicador `thinking...` en el status bar.
  El manifest habilita Ratatui de forma opcional y también usa
  `ratatui-textarea`.

## Lectura para CodeLoom

1. Para tool calls mientras corren, seguir primero el recorrido de eventos de
   Martty y Mindfork: el transcript debe recibir inicio, actualizaciones y
   finalización, en lugar de imprimir texto crudo dentro del textarea.
2. Para la densidad visual pedida, tomar Pawan como primer caso concreto de
   tarjeta plegada y Martty para desplazamiento del transcript y expansión. Los
   números de líneas son decisiones de esos proyectos, no valores que debamos
   copiar sin probar con el ancho real de Loom.
3. Mantener el textarea para entrada multilinea y colocar thinking/actividad en
   el feed/transcript. El indicador de actividad puede ser una fila breve con
   nombre de tool, estado y resumen; el resultado completo queda disponible al
   expandir.
4. Mindfork y Martty incluyen superficies de conversación mucho más amplias que
   el MVP de Loom. Estudiar widgets y flujo de estado, sin arrastrar sus
   subsistemas de chat, almacenamiento, ACP o proveedores.

## Paletas del showcase Batten

La cobertura de componentes por aplicación, con demos Batten, estado de
importación, superficies pendientes y SHAs de referencia, está en
[`showcase-components.md`](showcase-components.md). Actualizar ese inventario
cuando se revise uno de los checkouts o se añada una demo.

Batten expone `ThemePreset` para comparar seis paletas desde el showcase: Batten,
Martty, Mindfork, Pawan, Crush/Charmtone y MastraCode. F7 y Shift+F7 recorren
las opciones; todas las demos reciben la paleta activa, y la demo `themes`
muestra una fila de muestras para cada una.

Los valores se mapean a los tokens semánticos existentes de Batten. Martty usa
su base DeepSeek bluish y sus tokens de marca/estado; Mindfork usa el canvas y
roles dark publicados en `src/shared/theme.rs`; Pawan usa su `DEFAULT` de
`crates/pawan-cli/src/tui/theme.rs`; Crush usa los colores Charmtone declarados
en `internal/ui/styles/themes.go` y sus valores de la dependencia charmtone;
MastraCode usa `darkSurface` y `darkTheme` del source map local. Se usan valores
de sus temas dark para los tokens de Batten, no se trasladó código fuente.

Son **paletas de referencia**, no perfiles 1:1: el layout, el espaciado, los
fondos específicos por componente, los marcos y el render de cada app siguen
siendo los de Batten. Los temas personalizados claro/oscuro y la selección
persistente en la aplicación CodeLoom quedan fuera de esta galería inicial.

No se copió código de los repositorios ni se ejecutaron sus binarios o pruebas.
Esta nota registra el análisis de fuentes y las rutas de inspección, no una
validación visual de sus interfaces en un terminal.
