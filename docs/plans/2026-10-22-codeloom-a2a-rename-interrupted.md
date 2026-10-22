# Renombrar `SubagentStatus::Interrupted` — Plan de implementación (tarea A1)

> **Para agentes:** sub-skill requerida: `superpowers:subagent-driven-development` o `superpowers:executing-plans`. Tarea única. En esta ejecución quien supervisa commitea, así que **no commitees**.

**Meta:** que el nombre interno de «hijo que quedó sin terminar al cerrarse la aplicación» deje de coincidir con el «interrupted state» de A2A (que significa `input-required` y `auth-required`). El nuevo nombre es `Unfinished` (decisión mía, marcada para revisión; coherente con la spec A2A, que dice que la tarea «quedó sin terminar»).

**Arquitectura:** renombrado puro del modelo interno. La cadena de SQLite pasa a `"unfinished"`, pero se **sigue leyendo** la antigua `"interrupted"` para no romper bases de datos existentes. La proyección a `TaskState::Failed` no cambia (decisión nuestra ya rotulada).

**Spec:** `docs/specs/2026-10-15-loom-agent-profile-a2a.md`, secciones «Estado de tarea» y «Fuente normativa de los estados» (leerlas antes).

## Restricciones globales

- `codeloom-core` no depende de `ratatui`/`crossterm`. **Sin red en tests.**
- Comprobaciones: `cargo build`, `cargo test -p batten -p codeloom-core -p codeloom-tui -p codeloom-cli`, `cargo clippy -p codeloom-core -p codeloom-tui -- -D warnings`, `rustfmt --edition 2021 --check <ficheros tocados>`.
- Entorno de build (Git Bash): `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe' CARGO_TARGET_DIR='E:/workspace/personal/loom/target' CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`. No crear otros `target*`. Formatear solo con `rustfmt --edition 2021 <ruta>`, nunca `cargo fmt --all`. Ficheros en LF (`tr -cd '\r' < f | wc -c` debe dar 0).
- Comentarios en español «pijo», sin tacos ni andalucismos. No `git checkout/restore/stash/reset` ni push. No tocar `codeloom.toml` de la raíz.
- **No reescribir planes y specs históricos** (`docs/plans/2026-10-20-*`, `2026-10-21-*`, etc.): reflejan lo que se hizo entonces. Solo los documentos vivos citados abajo.

## Review Focus

- Una base de datos con filas `status = 'interrupted'` se sigue leyendo (como `Unfinished`) sin error.
- Lo que se escribe de nuevo usa `"unfinished"`; el ciclo escribir→leer es estable.
- `Unfinished` se sigue proyectando a `TaskState::Failed`.
- Ningún texto de interfaz cambia de comportamiento: donde hoy `Interrupted` se trataba igual que `Killed` (p. ej. el estado de actividad en `app.rs`, que lo muestra como cancelado), se conserva tal cual. Esa diferencia con la proyección A2A (`Failed`) es previa; **no la corrijas**, anótala en la spec.
- No quedan referencias a `Interrupted` del enum en `crates/` (salvo en el test que prueba la lectura de la cadena antigua).

---

### Tarea única: renombrado con compatibilidad de lectura

**Ficheros** (localizados con `grep -rn "Interrupted" crates`): `crates/codeloom-core/src/session/mod.rs` (enum), `crates/codeloom-core/src/session/sqlite.rs` (conversión a/desde cadena y su test), `crates/codeloom-core/src/task.rs` (proyección y su test), `crates/codeloom-tui/src/app.rs`, `crates/codeloom-tui/src/render.rs`, `crates/codeloom-tui/src/rail.rs` (usos y tests), y los documentos vivos: `docs/specs/2026-10-15-loom-agent-profile-a2a.md`, `docs/plans/2026-10-22-codeloom-a2a-roadmap.md`.

**Interfaces — produce:**
```rust
// session/mod.rs
pub enum SubagentStatus { Running, Completed, Killed, Failed, Unfinished }   // Interrupted → Unfinished, mismo lugar y orden
// sqlite.rs: escritura "unfinished"; lectura acepta "unfinished" y la antigua "interrupted" (ambas → Unfinished)
// task.rs: SubagentStatus::Unfinished => TaskState::Failed
```

**Comportamiento:** donde el almacén marca como no terminados los hijos que seguían `Running` al reabrir, debe escribir `Unfinished`. Actualizar los comentarios que mencionen «interrumpido» en ese sentido para que no se confundan con el «interrupted state» de A2A.

- [ ] **Paso 1: tests en rojo** (indicar si el rojo es de compilación o de aserción): `unfinished_status_roundtrips_through_sqlite`, `legacy_interrupted_string_is_read_as_unfinished` (inserta una fila con `status = 'interrupted'` directamente por SQL y comprueba que `list_subagents` la devuelve como `Unfinished`), `reopened_store_marks_running_children_unfinished` (adaptar el test existente del almacén), `unfinished_projects_to_failed` (adaptar el de `task.rs`).
- [ ] **Paso 2: implementar. Paso 3: verificar** `cargo build`, tests de los cuatro crates, clippy, rustfmt, LF, y `grep -rn "Interrupted" crates` (solo puede quedar el literal de la cadena antigua).
- [ ] **Paso 4: docs.** En la spec A2A, donde se nombra `SubagentStatus::Interrupted`, usar `Unfinished` y añadir una línea: «renombrado el 2026-10-22 para no coincidir con el “interrupted state” de A2A; la cadena antigua `interrupted` se sigue leyendo». En la hoja de ruta, marcar A1 como hecha. **Paso 5: no commitear.**
