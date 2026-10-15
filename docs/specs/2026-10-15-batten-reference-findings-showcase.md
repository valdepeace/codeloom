# batten — Hallazgos de referencias y composición CodeLoom en el showcase

> Estado: **propuesta para análisis**, no aprobación de dependencias ni plan ejecutado.
>
> Fecha: 2026-10-15.
>
> Evidencia: lectura de código de cuatro clones superficiales en `.references/`.
> No se han compilado, ejecutado ni validado visualmente esas aplicaciones.

## 1. Objetivo y relación con el diseño existente

Convertir el showcase de batten en un lugar donde analizar y validar las
primitivas que componen CodeLoom, tanto aisladas como juntas, sin proveedores,
red, herramientas reales ni motor de agentes.

Esta spec complementa, sin sustituir sus decisiones aprobadas:

- [Diseño de batten](2026-10-14-batten-design.md).
- [Showcase navegable](2026-10-15-batten-showcase-design.md).
- [Primitivas](2026-10-15-batten-primitives-design.md).
- [Composer editable](2026-10-15-batten-composer-editing-design.md).
- [Selección](2026-10-15-batten-selection.md).
- [Archivos modificados](2026-10-15-loom-modified-files-design.md).
- [Estado del turno](2026-10-15-loom-turn-status-line.md).

Las recomendaciones siguientes son propuestas. En particular, introducir wrap
en el composer modifica la decisión aprobada de recorte horizontal: requiere
resolver esa decisión antes de implementarlo. Tampoco se da por aprobada una
migración a otro editor.

## 2. Estado observado en nuestro checkout

- `batten/src/lib.rs` separa estado puro de widgets ratatui con tema.
- El catálogo de `examples/showcase/demos/mod.rs` ya contiene demos de
  transcript, composer, herramientas, sidebar, cambios, temas, efectos y otros.
- `demos/chat.rs` compone transcript, una herramienta, sidebar y composer.
  Simula la finalización de una herramienta y añade una respuesta completa
  tras 25 ticks; no demuestra todavía una respuesta que crece por deltas.
- `state/composer.rs` ya permite cursor, edición y movimiento multilínea.
  No es un buffer exclusivamente append, aunque una spec anterior lo describa así.
- `widgets/transcript.rs` selecciona líneas lógicas mediante `Viewport` y
  después aplica wrap con `Paragraph`. Una línea lógica puede consumir varias
  filas visuales: scroll y scrollbar deben contabilizar lo que se pinta.
- `Viewport` mantiene desplazamiento relativo al final. Al crecer el contenido
  mientras el usuario lee arriba, ese modelo puede desplazar su ventana;
  falta definir y verificar el anclaje esperado durante streaming.
- `FileChanges` muestra rutas y contadores, no el contenido de un diff.
- `tachyonfx`, ANSI, enlaces y perfil de terminal ya están integrados mediante
  features. No hace falta volver a introducirlos.
- `Demo::handle_key` recibe `KeyCode`, no modificadores ni eventos de ratón.
  Demostrar Ctrl+J, Shift+selección o pegado exige revisar el transporte de
  eventos del ejemplo; no implica acoplar el estado de batten a crossterm.

## 3. Referencias y aportación concreta

Los enlaces relativos apuntan a clones locales, que no forman parte de la
documentación publicable. Los enlaces remotos permiten localizar el proyecto;
antes de incorporar código se debe registrar el commit exacto utilizado.

| Referencia | Archivos inspeccionados | Aportación | Uso propuesto |
|---|---|---|---|
| [Martty](https://github.com/openma-ai/martty) | `.references/martty/src/input/composer.rs`, `tests/unit/input__composer__tests.rs`, `tests/unit/app__scroll_tests.rs` | Editor sobre `ratatui-textarea`; mapa de grafemas a celdas, wrap, tabs, ratón; pruebas de selección y undo | Referencia del composer y sus casos de prueba; evaluar el editor antes de ampliar el nuestro |
| [Claude Code Rust](https://github.com/srothgan/claude-code-rust) | `.references/claude-code-rust/src/ui/wrap.rs`, `markdown.rs`, `diff.rs` | Wrap con estilos, filas físicas, prefijos colgantes, adaptación de Markdown y render de diff | Referencia principal para transcript; adaptar solo lo necesario |
| [herdr-reviewr](https://github.com/persiyanov/herdr-reviewr) | `.references/herdr-reviewr/src/selection.rs`, `diff.rs` | Selección como datos, extracción de texto, filas de diff, números de origen y contexto plegado | Referencia para selección; diff en una fase posterior |
| [ratatui-markdown](https://github.com/celestia-island/ratatui-markdown) | `.references/ratatui-markdown/Cargo.toml`, `LICENSE`, `src/scroll/follow_scroll.rs` | Biblioteca de Markdown/árboles/scroll y offset manual absoluto | Comparar API y comportamiento; no adoptar automáticamente |

### 3.1 Límites de reutilización

- Martty y herdr-reviewr declaran MIT; Claude Code Rust declara Apache-2.0.
  Al incorporar código se conservarán los avisos y atribuciones correspondientes.
- El checkout de ratatui-markdown declara Ratatui `^0.30`, no 0.29 como indicaba
  la documentación consultada inicialmente. Declara `SySL-1.0`; su archivo
  LICENSE contiene requisitos específicos de avisos y divulgación de generación
  por IA. La adopción queda pendiente de revisión de condiciones.
- El composer de Martty usa `ratatui-textarea` 0.9.2. No se ha clonado ni
  auditado esa dependencia: no se infiere su licencia o adecuación desde Martty.
- `wrap_lines_to_physical_rows` de Claude Code Rust renderiza en un buffer
  intermedio. Copiar esa estrategia para todo el historial en cada delta podría
  elevar memoria y CPU; no se ha medido. No copiar el módulo entero por defecto.
- La selección de reviewr depende de sus tipos de diff y navegación. Su modelo
  conceptual encaja, pero el archivo no es una primitiva independiente lista
  para pegar en batten.

## 4. Decisiones propuestas para analizar

### P0 — Transcript, wrap y anclaje

Una única geometría debe alimentar pintado, scroll, scrollbar y futura selección.
Calcular el total y la ventana en filas visuales al ancho efectivo, incluyendo
el espacio de scrollbar. Evitar aplicar un segundo wrap incoherente al resultado.

Al llegar contenido: seguir el final solo si estaba activo el seguimiento;
si el usuario lee arriba, conservar su posición en el documento. Al volver al
final, reanudar seguimiento. Un resize debe conservar una referencia al contenido
visible, ajustando el offset si ya no cabe, sin saltar automáticamente al final.

La implementación mínima se decide contrastando Ratatui con los helpers de
referencia. No se prescribe caché ni virtualización sin necesidad medida.

### P1 — Composer y eventos del showcase

Primero demostrar correctamente el contrato ya aprobado: edición en medio,
multilínea, historial con borrador, Enter para enviar y Ctrl+J para salto.
Separar foco composer/transcript para que las flechas no compitan.

Después comparar dos alternativas: conservar `state::Composer` o adoptar un
editor especializado en la capa visual. Evaluar undo, selección, grafemas,
wrap y hit-testing, junto al coste de adaptar consumidores y pruebas.
Un `TextArea` ligado a ratatui no debe entrar en `state` puro.

Revisar `Demo`, `Showcase`, bucle de eventos y demos afectados para transportar
modificadores; añadir ratón o pegado solo al implementar su interacción.
No crear un sistema genérico de eventos anticipadamente.

### P2 — Selección y Markdown

Comenzar por selección de bloques/líneas, como propone la spec existente.
Mantener ancla y extremo en coordenadas del documento, independientes del scroll.
El widget resalta; el estado devuelve texto; la app decide cómo copiarlo.
El showcase puede mostrar el texto extraído sin escribir al portapapeles real.

Evaluar Markdown básico para respuestas: títulos, listas, enlaces y código.
Claude Code Rust demuestra una adaptación sobre `tui-markdown`; esa dependencia
también requiere evaluación antes de adoptarla. El texto incompleto durante
streaming debe seguir visible; un fallo de presentación no debe perder el original.
Mermaid, imágenes y árboles quedan fuera de esta primera composición.

### P3 — Diff

Mantener `FileChanges` como resumen. Si se necesita inspeccionar contenido,
crear una primitiva distinta con filas de contexto/altas/bajas y números de línea.
La app suministra datos: batten no ejecuta Git ni lee archivos para obtenerlos.
Plegado y énfasis por palabras se añaden solo después de validar el diff básico.

## 5. Composición CodeLoom en el showcase

Evolucionar la demo `chat` existente, manteniendo el catálogo y las demos
aisladas. La composición muestra:

1. Identidad y estado del turno mediante las primitivas existentes.
2. Transcript con respuesta progresiva y herramientas visibles en su contexto.
3. Composer editable y foco explícito.
4. Sidebar con información de sesión; resumen de cambios cuando los haya.
5. Flock simulado en la ubicación definida por el diseño de CodeLoom cuando
   se valide esa representación; no se implementa orquestación real.

La composición reutiliza los widgets públicos: nada de renderizadores paralelos
exclusivos del showcase. Los adaptadores de escenario permanecen en el ejemplo.
No convertir esta demo en una segunda aplicación CodeLoom.

### Escenarios reproducibles

| Escenario | Acción | Resultado esperado |
|---|---|---|
| Turno normal | Escribir y enviar | User → herramienta pending/running/success → respuesta por deltas → idle |
| Lectura durante streaming | Subir en transcript mientras crece | Ventana anclada; volver al final reanuda seguimiento |
| Texto difícil | Cargar URL larga, tabs, CJK, acentos combinados y emoji compuesto | Wrap consistente; cursor/selección sin cortar grafemas al habilitar ese soporte |
| Edición e historial | Editar en medio, salto, enviar, recuperar historial | Texto correcto y borrador conservado según spec del composer |
| Permiso | Simular herramienta awaiting-permission y resolverla | Acción explícita; herramienta solo continúa tras decisión simulada |
| Error y cancelación | Activar escenarios separados | Estado terminal coherente; composer utilizable y transcript conservado |
| Cambios | Completar herramienta con fixture de archivos | Contadores y rutas coherentes; diff solo cuando exista su primitiva |
| Resize | Pasar de ancho a estrecho durante lectura/edición | Degradación del layout sin panic, pérdida de texto ni foco invisible |

Fixtures pequeñas, deterministas y claramente simuladas. Permitir reiniciar
el escenario y pausar/avanzar sus pasos para inspección; reutilizar `tick` para
streaming automático. Las teclas concretas se fijan al revisar el routing, sin
interferir con escritura, navegación del catálogo ni salida.

## 6. Alcance de archivos y consumidores

- Primitivas: `state/viewport.rs`, `state/composer.rs`, `state/history.rs`,
  `state/transcript.rs`, `widgets/transcript.rs`, `widgets/composer.rs` y
  scrollbar; módulos/exports solo cuando la solución lo requiera.
- Showcase: `demo.rs`, `app.rs`, `main.rs`, catálogo y demos `chat`,
  `composer`, `transcript`; selección/Markdown/diff cuando se aprueben.
- Consumidores: revisar `codeloom-tui` al modificar firmas o comportamiento
  público. No migrar su layout como efecto colateral de mejorar una demo.
- Configuración: `Cargo.toml`/lockfile solo si se decide añadir dependencia.
- Pruebas y fixtures de los comportamientos afectados; actualizar la spec
  correspondiente cuando se cambie deliberadamente una decisión aprobada.

## 7. Verificación y condición de cierre

Cada fase termina al demostrar su comportamiento aislado y en `chat`, sin
arrastrar la siguiente. Comprobaciones del repositorio:

```bash
cargo build
cargo test
cargo fmt --check
cargo clippy -- -D warnings
```

Para inspección interactiva: `cargo run -p batten --example showcase`.
Los tests de buffer/estado no sustituyen la inspección de teclado, resize,
cursor y streaming en un terminal real. Registrar terminal, tamaño, escenario
y resultado; no declarar validación visual sin realizarla.

Pruebas prioritarias: scroll por filas con estilos, crecimiento con lectura
manual, vuelta al final, resize, Unicode y routing de foco/modificadores.
Conservar los casos existentes de área vacía y tamaños mínimos.

Cierre de la primera entrega: demo `chat` con streaming por deltas, scroll
coherente y composer conforme al contrato aprobado. Selección, Markdown,
diff y ratón pueden entregarse después, sin bloquear ese resultado.

## 8. Preguntas abiertas

- ¿Se mantiene el composer recortado o se aprueba wrap visual?
- ¿Compensa adaptar `ratatui-textarea` preservando estado puro y API existente?
- ¿Qué mecanismo mínimo de geometría permite anclar transcript y selección?
- ¿Markdown pertenece a batten como capacidad opcional o lo suministra la app?
- ¿Cuándo necesita la composición un visor de diff además del resumen?
- ¿Qué representación de flock cabe en la primera demo sin anticipar m2?

Fuera de alcance: red, proveedores, persistencia, MCP, subagentes reales,
sandbox, instalación de aplicaciones de referencia y copia masiva de sus módulos.
