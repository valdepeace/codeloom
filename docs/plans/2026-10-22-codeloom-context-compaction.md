# Compactación ligera y topes de contexto: plan

**Spec:** `docs/specs/2026-10-22-codeloom-context-compaction-design.md`.

## Restricciones globales

`codeloom-core` no depende de `ratatui`/`crossterm`; errores con `thiserror`; sin red real en tests;
comentarios en español formal; ficheros en LF; formatear solo con `rustfmt --edition 2021 <ruta>`;
checks: `cargo test -p codeloom-core`, `cargo clippy -p codeloom-core -- -D warnings`.

## Tarea 1: topes de avisos y de `grep`

**Ficheros:** `crates/codeloom-core/src/tools/mod.rs`, `tools/grep.rs`, `agent/mod.rs`
(`flush_subagent_notices`), `subagent.rs` (línea del aviso `[Subagent {} completed]`).

**Produce:** `pub const MAX_NOTICE_BYTES: usize = 8_000;` y
`pub fn limit_bytes(content: &str, max: usize) -> Cow<'_, str>` en `tools/mod.rs`; `limit_output`
pasa a llamar a `limit_bytes(content, MAX_TOOL_OUTPUT_BYTES)` sin cambiar su comportamiento. El
detalle de `[Job … ]` y el resultado de `[Subagent … completed]` se pasan por
`limit_bytes(.., MAX_NOTICE_BYTES)`. `grep`: máximo 200 coincidencias, línea recortada a 300
caracteres (por frontera de carácter), aviso final `[N of TOTAL matches shown; narrow the pattern]`;
descripción de la herramienta actualizada.

- [ ] Tests en rojo: `limit_bytes_respects_the_given_cap`; `subagent_notice_result_is_capped`
  (resultado de 50 000 bytes → el aviso entregado mide ≤ 8 000 + aviso); `job_notice_detail_is_capped`;
  `grep_caps_matches_and_long_lines`.
- [ ] Implementar. Verificar. No commitear.

## Tarea 2: limpieza de resultados antiguos en la petición

**Ficheros:** nuevo `crates/codeloom-core/src/agent/clear.rs` (declarar `mod clear;` en
`agent/mod.rs`), `agent/mod.rs` (construcción de `ProviderRequest`).

**Produce:** `pub const KEEP_TOOL_RESULTS: usize = 6;`, `const CLEAR_MIN_BYTES: usize = 256;` y
`pub fn clear_old_tool_results(history: &[Message], keep: usize) -> Vec<Message>`: devuelve una copia
en la que, contando los `ContentPart::ToolResult` de más reciente a más antiguo, todos los que quedan
más allá de `keep` y cuyo `content.len() > CLEAR_MIN_BYTES` tienen su `content` sustituido por
`format!("[tool result cleared: {} bytes; run the tool again if you need it]", n)`, conservando
`call_id` e `is_error`. En `run_turn`, `messages: history.clone()` pasa a
`messages: clear_old_tool_results(&history, KEEP_TOOL_RESULTS)`. `history` y el almacén no se tocan.

- [ ] Tests en rojo (en `clear.rs`): `clears_only_results_older_than_the_last_six`,
  `small_old_results_are_kept`, `call_ids_and_order_are_preserved`,
  `original_history_is_not_modified`, y uno en `agent/mod.rs` con un proveedor espía que comprueba
  que la petición del paso 8 lleva los resultados antiguos limpiados mientras `store.messages`
  sigue completo.
- [ ] Implementar. Verificar. No commitear.

## Tarea 3: documentación

Estado en la spec; decálogo: reglas 5 (parcial: limpieza hecha, resumen pendiente), 3 y 8 (hechas).
