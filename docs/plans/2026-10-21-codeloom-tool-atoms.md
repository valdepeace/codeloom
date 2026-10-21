# Piezas atómicas por herramienta — Plan de implementación

> **Para agentes:** sub-skill requerida: `superpowers:subagent-driven-development` (recomendada) o `superpowers:executing-plans`. Los pasos usan casillas `- [ ]`. Una tarea cada vez, en orden. Al terminar y verificar cada tarea se commitea (lo hace quien orquesta).

**Meta:** que cada herramienta real de CodeLoom tenga una pieza de render atómica en `batten`, usada por la app y por un catálogo en el showcase, con estilos afinables por preset.

**Arquitectura:** `ToolFacts` (tipado, sin serde) en `batten::state`; `trait ToolAtom` + registro `atom_for(name)` con pieza genérica de reserva; adaptador `tool_facts(name, &Value)` en `codeloom-tui`; tokens de estilo de pieza en el tema; catálogo en `examples/showcase` que reutiliza todo.

**Stack:** Rust, `ratatui`, `serde_json` (solo en `codeloom-tui`).

**Spec:** `docs/specs/2026-10-21-codeloom-tool-atoms-design.md` (leerla antes).

## Restricciones globales

- `batten` no depende de `codeloom-core` ni de `serde`/`serde_json`; `codeloom-core` no depende de `ratatui`/`crossterm`. Sin red en tests.
- Comprobaciones obligatorias: `cargo build`, `cargo test`, `cargo fmt --all --check`, `cargo clippy -- -D warnings` (el `examples/showcase` tiene errores de clippy preexistentes con `--all-targets`: no ampliar y no tocarlos salvo en código nuevo, que debe estar limpio).
- Entorno de build (Git Bash): `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe' CARGO_TARGET_DIR='E:/workspace/personal/loom/target' CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`. No crear otros `target*` (disco casi lleno). Formatear solo con `rustfmt --edition 2021 <ruta>` (nunca `cargo fmt --all` para escribir). Ficheros en LF (`cat <ficheros> | tr -cd '\r' | wc -c` = 0). Sin `git checkout/restore/stash/reset`.
- Comentarios en español «pijo», sin tacos ni andalucismos.
- Fluidez: el JSON se parsea solo al sincronizar la herramienta (`sync_tool`), nunca en el camino de los deltas ni en `render`.
- La pieza genérica debe reproducir `tool_lines` actual exactamente; los tests existentes de `tool_lines` y de `sync_tool` no se modifican.

## Review Focus

- Entrada JSON mal formada, campos ausentes o de tipo inesperado: el adaptador degrada a la pieza genérica, sin pánico.
- Rutas, comandos o salidas larguísimas, con saltos de línea o caracteres anchos: truncado por grafemas, sin romper el ancho.
- Diff de `edit` vacío (sin cambios) o con archivos enormes: acotado a un máximo de líneas con marca de ocultas.
- Herramienta en curso sin salida aún; con error; cancelada; esperando permiso.
- Look Classic (herramientas ancladas fuera del transcript) no cambia.

---

### Tarea 1: frontera y registro — `ToolFacts`, `ToolAtom`, pieza genérica, adaptador

**Ficheros:** crear `crates/batten/src/state/tool_atom.rs` (exportar en `state/mod.rs`); crear `crates/codeloom-tui/src/tool_facts.rs` (declarar en `lib.rs`/`main.rs` según el crate); modificar `crates/codeloom-tui/src/app.rs` (`sync_tool` y las dos llamadas a `set_summary`, ~líneas 779 y 1549).

**Interfaces — produce:**
```rust
// batten::state::tool_atom
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolFacts {
    pub primary: Option<String>,            // ruta, comando o patrón
    pub secondary: Option<String>,          // segundo dato corto (p. ej. glob de filtro, descripción)
    pub range: Option<(usize, usize)>,      // líneas inicio-fin (view)
    pub before: Option<String>,             // texto previo (edit)
    pub after: Option<String>,              // texto nuevo (edit)
    pub count: Option<usize>,               // recuento (coincidencias, tareas…)
    pub items: Vec<(String, String)>,       // resto de pares clave-valor mostrables
}
pub trait ToolAtom: Sync {
    fn lines(&self, item: &ToolItem, facts: &ToolFacts) -> Vec<Line>;
}
pub fn atom_for(name: &str) -> &'static dyn ToolAtom;   // reserva: GenericAtom
pub fn tool_lines_with(item: &ToolItem, facts: &ToolFacts) -> Vec<Line>; // atom_for(&item.name).lines(item, facts)
// ToolItem gana: pub facts: ToolFacts  (o se pasa aparte; elegir lo que menos toque y documentarlo)
// codeloom-tui::tool_facts
pub fn tool_facts(name: &str, input: &serde_json::Value) -> ToolFacts;  // Default si no entiende la entrada
```
`GenericAtom::lines` devuelve exactamente `tool_lines(item)` (el actual `summary` crudo se mantiene mientras no haya pieza propia). `sync_tool` llama a `tool_lines_with` con los hechos guardados de la herramienta; los hechos se calculan una sola vez al fijar la entrada (`set_summary` sitio de llamada) con `tool_facts`. Las herramientas sin pieza propia ven el mismo resultado que hoy.

- [ ] **Paso 1: tests en rojo** (indicar si es de compilación o de aserción): en `tool_atom.rs`: `generic_atom_matches_the_current_tool_lines` (comparar `tool_lines` con `tool_lines_with` para varios `ToolItem`: sin salida, con salida corta, plegada, expandida, con error), `unknown_tool_falls_back_to_generic`, `facts_default_is_empty`. En `tool_facts.rs`: `malformed_input_yields_default_facts` (null, número, array, objeto sin campos), `bash_input_extracts_command`, `view_input_extracts_path_and_range`, `edit_input_extracts_path_and_before_after`, `grep_input_extracts_pattern_and_path`, `glob_input_extracts_pattern`. **Las formas reales de entrada de cada herramienta se leen en `crates/codeloom-core/src/tools/`** (nombres de campo y tipos exactos); no inventarlos.
- [ ] **Paso 2: implementar.** **Paso 3: verificar** `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --all --check`; los tests existentes de `app.rs` sobre herramientas siguen verdes sin tocarlos.

---

### Tarea 2: piezas `bash`, `view`, `grep`, `glob`

**Ficheros:** `crates/batten/src/state/tool_atom.rs` (o submódulo `tool_atom/` si crece), tokens y render si hiciera falta en `widgets/transcript.rs`.

Comportamiento (la cabecera conserva `LineKind::ToolHead(status)` y el `nombre` con `ToolName`; lo que cambia es el resumen y el cuerpo):
- `bash`: resumen = `primary` (comando, primera línea, truncado a 80 grafemas); cuerpo = salida plegada como hoy.
- `view`: resumen = `primary` (ruta) + ` L{a}-{b}` si hay `range`; cuerpo plegado.
- `grep`: resumen = `primary` (patrón) + ` in {secondary}` si hay ruta; si hay `count` o ya hay salida, línea final «N matches» (contando líneas de salida no vacías si `count` es `None` y hay salida).
- `glob`: igual que `grep` con «N files».
- Estados: sin salida (en curso) no muestra recuento; error muestra la salida como hoy.
- Si falta `primary`, degrada a la pieza genérica.
- Tokens de span: nombre `ToolName`, ruta/patrón `ToolPath` o `ToolCommand` (los tokens se crean en la Tarea 5; hasta entonces usar `Accent`/`Muted` existentes y dejar un único punto de cambio en una función `style_of(kind)`).

- [ ] **Paso 1: tests en rojo:** `bash_summary_is_the_command_truncated`, `view_summary_includes_range`, `grep_shows_match_count_from_output`, `glob_shows_file_count`, `running_tool_without_output_has_no_count`, `missing_primary_falls_back_to_generic`, `long_command_with_newlines_and_wide_chars_is_truncated_by_graphemes`.
- [ ] **Paso 2: implementar.** **Paso 3: verificar** suite completa, clippy, fmt.

---

### Tarea 3: pieza `edit` con diff

**Ficheros:** `tool_atom.rs`; `crates/batten/src/state/transcript.rs` (nueva variante `LineKind::ToolDiff(DiffKind)`); `crates/batten/src/widgets/transcript.rs` (`prefix`/`render_line`/estilo de la nueva variante con `DiffInsert`/`DiffDelete`/`DiffHunk`).

Comportamiento: resumen = `primary` (ruta) + `  +{added} -{removed}`; cuerpo = líneas de `Diff::new(before, after, 2)` (`batten::state::Diff`) como `ToolDiff(kind)` con marca `+`/`-`/` `, acotadas a 12 líneas con `ToolMore` «… (N lines hidden)» si se pasa; `Ctrl+O` (expandido) las muestra todas. Sin `before`/`after` o diff vacío: degrada a la pieza genérica (salida del resultado). Respetar cómo el widget ya pinta `ToolBody` (prefijo, sangría) y que la selección de texto y el layout (`measure`) funcionen con la variante nueva.

- [ ] **Paso 1: tests en rojo:** `edit_summary_shows_path_and_counts`, `edit_body_has_diff_lines_with_kinds`, `edit_diff_is_capped_and_expandable`, `edit_without_before_after_falls_back`, `empty_diff_falls_back`, y en `widgets/transcript.rs` un test de render (`TestBackend`) que compruebe que las celdas de una línea `ToolDiff(Insert)` y `ToolDiff(Delete)` llevan los estilos `DiffInsert`/`DiffDelete` del tema.
- [ ] **Paso 2: implementar.** **Paso 3: verificar** suite completa, clippy, fmt (todos los `match` sobre `LineKind` deben quedar exhaustivos).

---

### Tarea 4: piezas `ask_user`, `subagent*`, `job_*`

**Ficheros:** `tool_atom.rs`, `tool_facts.rs` (formas de entrada reales en `crates/codeloom-core/src/tools/`, `subagent_input.rs`, `job.rs`, `question.rs`).

Comportamiento (resumen y cuerpo cortos, sin duplicar lo que ya muestran el rail y la actividad):
- `ask_user`: resumen = primera pregunta (truncada) + `  {n} options` si hay opciones.
- `subagent`: resumen = etiqueta/objetivo del hijo (truncado); en curso sin salida; con salida, cuerpo plegado.
- `subagent_control`: resumen = acción + etiqueta/id destino.
- `subagent_list` y `job_list`: resumen = `{count} items` si hay recuento o líneas de salida.
- `job_start`: resumen = comando; `job_output`/`job_kill`: resumen = id de job.
- Degradar a genérica si faltan datos.

- [ ] **Paso 1: tests en rojo:** uno por pieza con su entrada real y uno de degradación; `ask_user_shows_first_question_and_option_count`, `subagent_summary_is_the_label`, `subagent_control_summary_has_action_and_target`, `list_tools_show_item_counts`, `job_tools_show_command_or_id`, `unknown_shape_falls_back`.
- [ ] **Paso 2: implementar.** **Paso 3: verificar** suite completa, clippy, fmt.

---

### Tarea 5: tokens de estilo de pieza y valores por preset

**Ficheros:** `crates/batten/src/state/transcript.rs` (`StyleToken::ToolPath`, `ToolCommand`, `ToolCount`), `crates/batten/src/widgets/theme.rs` (campos en `styles`, valores por defecto derivados: `ToolPath`→`accent`, `ToolCommand`→`default`, `ToolCount`→`dim`; `Theme::style` los resuelve), `tool_atom.rs` (sustituye `style_of` provisional de las Tareas 2-4 por los tokens).

Cada preset puede sobrescribirlos; en esta tarea solo Crush define valores propios (cualquier valor razonable derivado de su paleta) y el resto hereda.

- [ ] **Paso 1: tests en rojo:** `new_tokens_resolve_in_every_preset` (todos los presets devuelven un `Style` para cada token nuevo), `defaults_are_derived_from_existing_styles`, `crush_overrides_tool_path`, y un test por pieza que compruebe qué token lleva su ruta/comando/recuento.
- [ ] **Paso 2: implementar.** **Paso 3: verificar** suite completa, clippy, fmt (los `match` exhaustivos sobre `StyleToken` deben compilar).

---

### Tarea 6: catálogo «CodeLoom tools» en el showcase

**Ficheros:** `crates/batten/examples/showcase/demos/codeloom_tools.rs` (nuevo), `demos/mod.rs` (registro con provider «CodeLoom»), siguiendo el patrón de una demo existente (`crush_tool_renderers.rs`, `Demo` trait en `demo.rs`).

Comportamiento: una demo por herramienta (`bash`, `view`, `edit`, `grep`, `glob`, `todo_write`, `ask_user`, `subagent`, `subagent_control`, `job_start`, `job_list`) que construye un `ToolItem` + `ToolFacts` de ejemplo, lo pasa por `tool_lines_with`, lo mete en un `Transcript` y lo pinta con `transcript::render` bajo el tema activo. Teclas de la demo: ciclar estado (en curso → éxito → error → esperando permiso) y alternar plegada/expandida (reutilizar `KeyCode` como las demos de Crush). El filtro por provider del showcase incluye «CodeLoom».

- [ ] **Paso 1: tests en rojo** (en el propio ejemplo o, si el ejemplo no admite tests, en un módulo de `batten` que construya los fixtures: p. ej. `batten::state::tool_atom::fixtures` bajo `#[cfg(any(test, feature = "fixtures"))]` y el ejemplo los consume; documentar la elección): `every_real_tool_has_a_fixture`, `each_fixture_renders_in_every_state_without_panic` (todos los presets × estados × plegado), `catalog_uses_atom_for` (la salida de la demo es la de `tool_lines_with`).
- [ ] **Paso 2: implementar.** **Paso 3: verificar** `cargo build --examples -p batten`, suite completa, clippy sobre el código nuevo, fmt.

---

### Tarea 7: documentación

- [ ] Sección «Estado» en `docs/specs/2026-10-21-codeloom-tool-atoms-design.md` (qué se implementó, decisiones al ejecutar, límites, validación visual pendiente) y entrada en `docs/specs/2026-10-14-loom-architecture-overview.md` si lista los módulos de `batten`.

## Autoauditoría

- Cobertura: objetivos 1 y 3 (T1), 2 (T2–T4, `todo_write` ya existe), 4 (T5), 5 (T6).
- Tipos coherentes: `ToolFacts`, `ToolAtom`, `atom_for`, `tool_lines_with`, `tool_facts`, `ToolDiff`, `ToolPath`/`ToolCommand`/`ToolCount` se nombran igual en todas las tareas.
- Riesgo: formas de entrada reales de cada herramienta (se leen del código de `codeloom-core`), y validación visual en terminal real pendiente.
