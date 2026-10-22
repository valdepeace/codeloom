# Resumen del historial con el modelo: plan de implementación

> Para quien ejecute: una tarea cada vez, con verificación y commit al terminar cada una.

**Meta:** cuando el contexto estimado alcanza el 80 % de la ventana del modelo, resumir lo antiguo
con una llamada extra y enviar al proveedor «último resumen + cola reciente», sin tocar el
registro de la sesión.

**Arquitectura:** funciones puras en `crates/codeloom-core/src/agent/compact.rs` (estimación,
corte, mensaje de resumen, historial efectivo) más un método de `AgentLoop` que hace la llamada y
persiste el resumen. El bucle `run_turn` lo invoca antes de construir cada petición.

**Spec:** `docs/specs/2026-10-23-codeloom-context-summary-design.md` (léase primero).

## Restricciones globales

- `codeloom-core` NO depende de `ratatui` ni `crossterm`. Errores con `thiserror`. Sin red real
  en tests: proveedor simulado (véase `HistorySpy` y `QueueProvider` en `agent/mod.rs`, módulo de
  pruebas).
- Comentarios de código en español formal, sin tacos ni andalucismos. No tocar identificadores
  ni mensajes de error existentes.
- Ficheros en LF. Formatear solo con `rustfmt --edition 2021 <ruta>` (nunca `cargo fmt --all`).
- Comprobaciones: `cargo build -p codeloom-core`, `cargo test -p codeloom-core`,
  `cargo clippy -p codeloom-core -p codeloom-cli -- -D warnings` (sin `--all-targets`).
- Entorno (Git Bash): `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe' CARGO_TARGET_DIR='E:/workspace/personal/loom/target' CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`.
  No crear otros directorios `target*` (el disco está casi lleno).
- **No** tocar `codeloom.toml` (modificado por el usuario). `git add` solo de rutas concretas.
- Un test existente que haya que adaptar se marca con un comentario `// ADAPTADO: <motivo>` y no
  se relaja ninguna cota.

## Cifras (decisiones propias, spec §Diseño)

`COMPACT_RATIO = 0.80`, `KEEP_TAIL_TOKENS = 10_000`, `MAX_SUMMARY_BYTES = 16_000`,
`SUMMARY_MAX_TOKENS = 4096`, `MAX_COMPACTIONS_PER_TURN = 3`. Estimación: bytes / 4.

---

### Tarea 1: ventana de contexto en el catálogo

**Ficheros:** `crates/codeloom-core/src/catalog.rs` (método + test).

**Produce:** `Catalog::context_window(&self, base_url: &str, model: &str) -> Option<u64>`, junto
a `prices`, con la misma búsqueda (proveedor por URL normalizada, luego modelo por `id`) y
devolviendo `CatalogModel::context`. `None` si falta proveedor, modelo o dato.

**Tests** (junto a los de `prices`, mismo JSON de muestra o uno mínimo):
- `context_window_returns_the_published_limit`: modelo con `limit.context = 200000` → `Some(200000)`.
- `context_window_is_none_for_unknown_model_or_provider_or_missing_limit`: tres casos → `None`.

**Commit:** `feat(core): ventana de contexto de un modelo según el catálogo`.

---

### Tarea 2: funciones puras de compactación

**Ficheros:** crear `crates/codeloom-core/src/agent/compact.rs`; en `agent/mod.rs` añadir
`pub mod compact;` junto a `pub mod stop;`.

**Produce (firmas exactas):**

```rust
pub const COMPACT_RATIO: f64 = 0.80;
pub const KEEP_TAIL_TOKENS: usize = 10_000;
pub const MAX_SUMMARY_BYTES: usize = 16_000;
pub const SUMMARY_MAX_TOKENS: u32 = 4096;
pub const MAX_COMPACTIONS_PER_TURN: usize = 3;

/// Estimación de tokens de un texto: bytes / 4, redondeando hacia arriba.
pub fn estimate_text_tokens(text: &str) -> usize;
/// Estimación de un mensaje: suma sobre sus partes (texto, razonamiento, JSON de llamadas,
/// contenido de resultados); una imagen cuenta su URL.
pub fn estimate_message_tokens(message: &Message) -> usize;

/// Cabecera de un resumen: `[Context summary: first {covers} messages]`.
pub fn summary_message(covers: usize, text: &str) -> Message;          // Role::User, un Text
/// `Some((covers, texto))` si el mensaje es un resumen; `None` si no. Tolerante: cabecera mal
/// formada → `None`.
pub fn parse_summary(message: &Message) -> Option<(usize, String)>;

/// Posiciones (en `history`) de los mensajes que forman la petición: el último resumen y,
/// detrás, los de posición >= su `covers` salvo cualquier resumen. Sin resumen: todas.
pub fn effective_indices(history: &[Message]) -> Vec<usize>;
/// Copia de esos mensajes, en orden.
pub fn effective_history(history: &[Message]) -> Vec<Message>;

/// Tamaño estimado del contexto de la próxima petición, en tokens.
pub fn estimate_context_tokens(history: &[Message], system_bytes: usize) -> usize;
/// `true` si `estimate_context_tokens >= window * COMPACT_RATIO`.
pub fn should_compact(history: &[Message], system_bytes: usize, window: u64) -> bool;

/// Posición cruda `N` desde la que se conserva la cola, o `None` si no hay nada que resumir.
pub fn choose_cut(history: &[Message]) -> Option<usize>;
```

**Algoritmos:**

- `estimate_context_tokens`: sea `idx = effective_indices(history)` y `s` la posición del último
  resumen (`None` si no hay). **Ancla:** el último elemento de `idx` que sea mensaje `Assistant`
  con `usage = Some(u)` y, si hay resumen, de posición `> s`. Con ancla `a`:
  `u.input_tokens + u.output_tokens` más la suma de `estimate_message_tokens` de los elementos de
  `idx` posteriores a `a`. Sin ancla: `system_bytes / 4` más la suma de `estimate_message_tokens`
  de todos los de `idx`.
- `choose_cut`: sea `idx = effective_indices(history)`. Recorriendo `idx` desde el final,
  acumular `estimate_message_tokens` hasta alcanzar `KEEP_TAIL_TOKENS`; el elemento en el que se
  alcanza (o el primero si nunca se alcanza) marca la posición candidata `k` dentro de `idx`.
  **Legalización:** mientras `history[idx[k]].role == Role::Tool`, `k -= 1` (la cola nunca empieza
  por un resultado: debe llevar su llamada). El elemento 0 de `idx` puede ser un resumen: no se
  puede cortar antes del índice 1 de `idx` si lo es. Si `k` queda en 0, o si en `idx[..k]` solo
  hay un resumen (nada nuevo que resumir), devolver `None`. Si no, `Some(idx[k])`.

**Tests** (módulo `tests` del mismo fichero), con constructores `Message::user`, `assistant`,
`Role::Tool`:
- `summary_round_trips`: `parse_summary(&summary_message(7, "hola\nmundo"))` es `Some((7, "hola\nmundo"))`.
- `parse_summary_rejects_ordinary_and_malformed_messages`: un mensaje de usuario normal, uno con
  `[Context summary: first x messages]` y uno con `[Context summary` sin cerrar → `None`.
- `effective_history_without_summary_is_the_whole_history`.
- `effective_history_uses_the_last_summary_and_the_messages_from_covers`: log
  `[m0,m1,m2,m3,S(covers=2),m5]` → `[S, m2, m3, m5]`.
- `effective_history_drops_older_summaries`: dos resúmenes; solo sobrevive el último.
- `estimate_uses_the_anchor_after_the_last_summary_only`: un asistente con `usage` anterior al
  resumen NO se usa como ancla (estima por bytes); uno posterior sí.
- `should_compact_respects_the_ratio`: con ventana 1000 y contexto estimado 799 → `false`; 800 → `true`.
- `choose_cut_never_starts_the_tail_with_a_tool_result`: historial largo cuyo punto de corte
  natural cae en un mensaje `Tool` → el resultado devuelto apunta al mensaje del asistente
  anterior.
- `choose_cut_is_none_when_everything_fits_in_the_tail`: historial corto → `None`.
- `choose_cut_keeps_roughly_the_tail_budget`: 100 mensajes de 4 000 bytes (1 000 tokens) →
  la cola tiene entre 10 y 12 mensajes.
- `choose_cut_skips_an_existing_summary`: historial que ya empieza por resumen y poco más → `None`.

**Commit:** `feat(core): funciones puras de la compactación con resumen`.

---

### Tarea 3: la llamada de resumen

**Ficheros:** `crates/codeloom-core/src/agent/compact.rs` (prompt), `agent/mod.rs` (método y
evento), `crates/codeloom-core/src/event/mod.rs` (variante).

**Produce:**

- `pub const SUMMARY_SYSTEM_PROMPT: &str` en inglés (los prompts de modelo del proyecto están en
  inglés): pide un resumen para que otro agente continúe el trabajo, sin llamar a herramientas,
  con estas secciones en este orden y con estos títulos: `Goal`, `User instructions and
  constraints`, `Work done`, `Current state and files touched`, `Decisions and findings`,
  `Open problems and next steps`. Pide conservar nombres de fichero, comandos y errores exactos, y
  no superar unas 1 500 palabras.
- `LiveEvent::ContextCompacted { replaced: usize, tokens_before: u64, summary_bytes: usize }`
  (derives y tratamiento en los `match` de `event/mod.rs` como las demás variantes de aviso; si
  hay un `match` exhaustivo en otras crates, añadir el brazo mínimo).
- En `impl AgentLoop`: 

```rust
/// Resume lo antiguo del historial de `session` y lo persiste como mensaje de resumen.
/// Devuelve `Ok(true)` si compactó, `Ok(false)` si no había nada que resumir o el modelo
/// devolvió un resumen vacío. Un fallo del proveedor se propaga como `Err`.
async fn compact(
    &self,
    session: SessionId,
    history: &[Message],
    tokens_before: u64,
    cancel: &CancellationToken,
) -> Result<bool>;
```

  Pasos: `cut = choose_cut(history)?` (si `None`, `Ok(false)`); mensajes a resumir = los de
  `effective_indices(history)` con posición `< cut`, pasados por
  `clear::clear_old_tool_results(.., KEEP_TOOL_RESULTS)`, más un último mensaje de usuario
  «Summarize the conversation above following your instructions.»; petición con
  `system = SUMMARY_SYSTEM_PROMPT`, `tools = vec![]`, `temperature = 0.0`,
  `max_tokens = SUMMARY_MAX_TOKENS`, `effort = None`, modelo de la configuración; recoger los
  `TextDelta` hasta `Done`, respetando `cancel`; si el texto recogido (recortado) está vacío →
  `Ok(false)`; aplicar `crate::tools::limit_bytes(&text, MAX_SUMMARY_BYTES)` (misma función que
  usan los avisos de subagente; comprobar su ruta real con `Grep`); persistir con
  `store.append_message(session, &summary_message(cut, &text))`; publicar
  `LiveEvent::MessageAdded` con ese mensaje y `LiveEvent::ContextCompacted`; `Ok(true)`.

**Tests** (módulo de pruebas de `agent/mod.rs`, con un proveedor simulado que devuelve un texto
fijo y guarda la petición recibida):
- `compact_persists_a_summary_message_and_leaves_the_log_intact`: tras `compact`, `store.messages`
  tiene los mismos mensajes de antes más uno al final, que `parse_summary` reconoce con
  `covers == cut`.
- `compact_request_has_no_tools_and_ends_with_the_summarize_instruction`.
- `compact_caps_an_oversized_summary`: el proveedor devuelve 40 000 bytes → el texto guardado
  mide como mucho `MAX_SUMMARY_BYTES` más la cabecera.
- `compact_returns_false_when_the_model_returns_nothing`: sin mensaje añadido.
- `compact_returns_false_when_there_is_nothing_to_summarize`: historial corto, sin petición.
- `compact_publishes_context_compacted`.

**Commit:** `feat(core): llamada al modelo que resume el historial antiguo`.

---

### Tarea 4: integrar en el bucle

**Ficheros:** `crates/codeloom-core/src/agent/mod.rs` (`run_turn`).

**Cambios en `run_turn`:**

1. Antes del `loop`: `let mut compactions = 0usize; let mut compaction_failed = false;`.
2. Dentro del `loop`, tras `history = self.store.messages(session).await?;` y la comprobación de
   `should_stop`, antes de construir `req`:
   - `window = self.catalog.read().unwrap().context_window(&config.provider.base_url, &config.model)`;
   - si `Some(window)`, `compactions < MAX_COMPACTIONS_PER_TURN`, `!compaction_failed` y
     `compact::should_compact(&history, system.len(), window)`: llamar a `self.compact(...)` con
     `tokens_before = estimate_context_tokens(..) as u64`. `Ok(true)` → `compactions += 1` y
     recargar `history` desde el almacén; `Ok(false)` o `Err(_)` → `compaction_failed = true`
     (el `Err` se registra con `tracing::warn!` y **no** aborta el turno; la cancelación sí
     debe respetarse: si `cancel.is_cancelled()` tras la llamada, `break`).
3. La petición pasa a usar
   `clear::clear_old_tool_results(&compact::effective_history(&history), clear::KEEP_TOOL_RESULTS)`.

   `should_stop` y el resto del bucle siguen usando `history` completo.

**Tests** (módulo de pruebas; catálogo con un modelo de ventana pequeña vía
`Catalog::from_json`, como `priced_catalog`; `HistorySpy`-like que además devuelve el resumen):
- `request_is_summary_plus_tail_once_the_window_is_reached`: historial grande, ventana pequeña →
  la primera petición de «resumen» no lleva herramientas, y la siguiente lleva `[resumen, cola…]`
  con el primer mensaje reconocible por `parse_summary`.
- `store_keeps_every_message_after_compaction`: el registro conserva todos los mensajes
  anteriores íntegros.
- `no_compaction_when_the_window_is_unknown`: catálogo vacío → ninguna petición de resumen.
- `a_failing_summary_does_not_abort_the_turn_and_is_not_retried`: el proveedor falla en la
  petición de resumen y responde bien en la normal → el turno termina; una sola petición de
  resumen.
- `compactions_per_turn_are_capped`: ventana diminuta que obligaría a compactar siempre → como
  mucho `MAX_COMPACTIONS_PER_TURN`.
- `tool_call_and_result_stay_paired_in_the_request`: ninguna petición empieza la cola por un
  mensaje `Tool`.

**Commit:** `feat(core): el bucle compacta el historial al acercarse a la ventana`.

---

### Tarea 5: mostrarlo en la TUI

**Ficheros:** `crates/codeloom-tui/src/app.rs` (junto a `parse_notice` y `Notice`).

**Cambios:** reconocer el mensaje de resumen (usar `codeloom_core::agent::compact::parse_summary`
si el módulo es público; si no, hacerlo público) y pintarlo como **aviso propio** en lugar de
texto de usuario: línea de cabecera `contexto compactado (N mensajes resumidos)` y el texto del
resumen plegado, con el mismo tratamiento que los avisos de subagente (`Notice::Subagent`) —
copiar su patrón de pintura y de plegado, sin inventar otro. El rótulo debe decir que es de
CodeLoom (no de A2A). El `LiveEvent::ContextCompacted` no necesita línea propia si el mensaje
añadido ya la produce; en tal caso el `match` solo debe ignorarlo.

**Tests:** `compaction_summary_renders_as_a_notice_not_as_user_text` (la línea no es
`LineKind::User` con el texto crudo y contiene «contexto compactado»);
`ordinary_user_message_starting_with_a_bracket_is_untouched`.

**Verificación extra:** `cargo test -p codeloom-tui` y `cargo clippy -p codeloom-tui -- -D warnings`.

**Commit:** `feat(tui): el resumen de contexto se muestra como aviso propio`.

---

### Tarea 6: documentación

**Ficheros:** `docs/specs/2026-10-22-codeloom-decalogo-de-contexto-y-coste.md` (regla 5: pasa a
«hecho salvo `/compact` manual y reintento reactivo»), esta spec (sección «Estado» con commits y
lo que queda fuera), `docs/specs/2026-10-22-codeloom-context-compaction-design.md` (puntero).
Prosa en español formal.

**Commit:** `docs: estado real de la compactación con resumen`.
