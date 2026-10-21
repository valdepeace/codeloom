# Estados de tarea alineados con A2A — Plan de implementación

> **Para agentes:** sub-skill requerida: `superpowers:subagent-driven-development` (recomendada) o `superpowers:executing-plans`. Los pasos usan casillas `- [ ]`.

**Meta:** que los estados de jobs y subagentes tengan una proyección única y tipada hacia los estados de tarea de A2A v1.0, de modo que el futuro crate `codeloom-a2a` sea un adaptador y no una reescritura.

**Arquitectura:** un módulo puro `task.rs` en `codeloom-core` con `TaskState` (nombres y serialización de A2A) y conversiones desde `JobStatus` y `SubagentStatus`. El modelo interno NO cambia: la spec A2A §8 manda que el protocolo sea «un seam de traducción en el borde» y no dicte el modelo interno. Sin red, sin dependencias nuevas.

**Stack:** Rust, `serde` (ya en el workspace).

**Spec:** `docs/specs/2026-10-15-loom-agent-profile-a2a.md` (§5 mapeo de ciclo de vida, §8 gotchas).

## Restricciones globales

- `codeloom-core` no depende de A2A, HTTP ni red (spec A2A §6). Aquí tampoco se añade `a2a-lf`.
- No se modifican `JobStatus`, `SubagentStatus` ni el esquema SQLite (los estados se guardan como texto y `interrupted` ya existe).
- Comprobaciones obligatorias: `cargo build`, `cargo test`, `cargo fmt --all --check`, `cargo clippy -- -D warnings`.
- Entorno de build (Git Bash): `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe' CARGO_TARGET_DIR='E:/workspace/personal/loom/target' CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`. No crear otros directorios `target*` (disco casi lleno). Formatear solo con `rustfmt --edition 2021 <ruta>`.
- Comentarios en español «pijo» (registro fino y ceremonioso), sin tacos ni andalucismos.
- No hacer commit: lo decide el usuario.

## Review Focus

- `JobStatus::Stopping` (parada pedida, aún no terminada) debe seguir siendo `Working`, no `Canceled`.
- `SubagentStatus::Interrupted` (la app se cerró con el subagente en marcha) no tiene equivalente A2A exacto: se proyecta a `Failed` (decisión nuestra, anotada en la spec).
- La serialización debe ser exactamente la del protocolo: `input-required`, `auth-required`, `canceled` (una sola «l»).
- `is_terminal` debe ser `false` para `InputRequired` y `AuthRequired` (la tarea sigue viva, solo pausada).

---

### Tarea 1: `TaskState` y conversiones

**Ficheros:**
- Crear: `crates/codeloom-core/src/task.rs`
- Modificar: `crates/codeloom-core/src/lib.rs` (añadir `pub mod task;` entre `subagent` y `todo`, orden alfabético)

**Interfaces:**
- Produce: `codeloom_core::task::TaskState`, con `is_terminal(self) -> bool`, `as_str(self) -> &'static str`, `From<JobStatus>` y `From<SubagentStatus>`.
- Consume: `crate::job::JobStatus` (`Running`, `Stopping`, `Completed`, `Killed`, `Failed`) y `crate::session::SubagentStatus` (`Running`, `Completed`, `Killed`, `Failed`, `Interrupted`).

- [ ] **Paso 1: escribir los tests (en rojo)**

En `task.rs`, módulo `tests`, con estos casos y estas aserciones exactas:

```rust
#[test]
fn serializes_with_protocol_names() {
    assert_eq!(serde_json::to_string(&TaskState::InputRequired).unwrap(), "\"input-required\"");
    assert_eq!(serde_json::to_string(&TaskState::AuthRequired).unwrap(), "\"auth-required\"");
    assert_eq!(serde_json::to_string(&TaskState::Canceled).unwrap(), "\"canceled\"");
    assert_eq!(serde_json::to_string(&TaskState::Submitted).unwrap(), "\"submitted\"");
    let back: TaskState = serde_json::from_str("\"input-required\"").unwrap();
    assert_eq!(back, TaskState::InputRequired);
}

#[test]
fn as_str_matches_serialization() {
    for s in [
        TaskState::Submitted, TaskState::Working, TaskState::InputRequired,
        TaskState::AuthRequired, TaskState::Completed, TaskState::Failed,
        TaskState::Canceled, TaskState::Rejected,
    ] {
        assert_eq!(serde_json::to_string(&s).unwrap(), format!("\"{}\"", s.as_str()));
    }
}

#[test]
fn terminal_states_are_exactly_the_settled_ones() {
    for s in [TaskState::Completed, TaskState::Failed, TaskState::Canceled, TaskState::Rejected] {
        assert!(s.is_terminal());
    }
    for s in [TaskState::Submitted, TaskState::Working, TaskState::InputRequired, TaskState::AuthRequired] {
        assert!(!s.is_terminal());
    }
}

#[test]
fn job_status_projects_to_task_state() {
    assert_eq!(TaskState::from(JobStatus::Running), TaskState::Working);
    assert_eq!(TaskState::from(JobStatus::Stopping), TaskState::Working);
    assert_eq!(TaskState::from(JobStatus::Completed), TaskState::Completed);
    assert_eq!(TaskState::from(JobStatus::Failed), TaskState::Failed);
    assert_eq!(TaskState::from(JobStatus::Killed), TaskState::Canceled);
}

#[test]
fn subagent_status_projects_to_task_state() {
    assert_eq!(TaskState::from(SubagentStatus::Running), TaskState::Working);
    assert_eq!(TaskState::from(SubagentStatus::Completed), TaskState::Completed);
    assert_eq!(TaskState::from(SubagentStatus::Failed), TaskState::Failed);
    assert_eq!(TaskState::from(SubagentStatus::Killed), TaskState::Canceled);
    assert_eq!(TaskState::from(SubagentStatus::Interrupted), TaskState::Failed);
}
```

Ejecutar `cargo test -p codeloom-core task::` y confirmar que falla por compilación (el tipo no existe).

- [ ] **Paso 2: implementar `TaskState` en `crates/codeloom-core/src/task.rs`**

Firma y atributos fijados: `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]` y `#[serde(rename_all = "kebab-case")]` sobre `pub enum TaskState { Submitted, Working, InputRequired, AuthRequired, Completed, Failed, Canceled, Rejected }`. Se omite a propósito `unspecified` (es el valor cero de protobuf, no un estado real). `as_str` devuelve los mismos literales que la serialización. Las dos conversiones son un `match` exhaustivo sin comodín (para que un estado nuevo obligue a decidir su proyección). Cabecera de módulo `//!` explicando, en una frase, que es la proyección hacia A2A y que el modelo interno no cambia. Un comentario sobre `Interrupted -> Failed` que diga que A2A carece de equivalente y que se proyecta a fallo por haber quedado la tarea sin terminar.

- [ ] **Paso 3: exponer el módulo**

Añadir `pub mod task;` en `lib.rs`.

- [ ] **Paso 4: verificar**

Ejecutar: `cargo test -p codeloom-core task::` → 5 tests en verde. Después `cargo test` (workspace), `cargo clippy -- -D warnings` y `cargo fmt --all --check` → sin errores ni diferencias.

---

### Tarea 2: dejar constancia en las specs

**Ficheros:**
- Modificar: `docs/specs/2026-10-15-loom-agent-profile-a2a.md`
- Modificar: `docs/specs/2026-10-20-codeloom-todos-and-questions-design.md`

- [ ] **Paso 1: spec A2A.** Añadir al final de la §5 un subapartado «Estado de tarea (2026-10-21)» que registre: existe `codeloom_core::task::TaskState`; tabla de proyección (`Running`/`Stopping` → `working`; `Completed` → `completed`; `Failed` y `Interrupted` → `failed`; `Killed` → `canceled`); y que `Interrupted → failed` es decisión nuestra por falta de equivalente.
- [ ] **Paso 2: spec de preguntas.** Añadir una nota: `ask_user` es el análogo local de `input-required`; hoy los subagentes no la tienen (`.without("ask_user")`) y levantar esa restricción queda como trabajo posterior (ver «Fuera de este plan»).
- [ ] **Paso 3: verificar** que no se ha tocado nada más (`git diff --stat` muestra solo esas dos specs, `task.rs` y `lib.rs` respecto de este plan).

---

## Fuera de este plan (decisión pendiente del usuario)

**Que un subagente pueda usar `ask_user`** (estado A2A `input-required` real). Hoy está vetado en `InProcessSubagentLauncher::new` (`subagent.rs`) para que un hijo no bloquee la interfaz sin que se sepa quién pregunta. Levantarlo exige, como mínimo:

1. Que el diálogo indique el origen (hoy `ToolCtx` ya trae `session`, pero `PermissionService::ask_user` no lo transmite al responder).
2. Un estado visible «esperando respuesta» para ese subagente en el rail y en el transcript (probablemente un evento nuevo, no un valor persistido en SQLite).
3. Qué pasa si el usuario cancela o nunca contesta (el hijo se queda dormido; ¿tiempo máximo?).

Se trata como diseño aparte; no se implementa aquí.

## Autoauditoría

- Cobertura de la spec: solo se toca el mapeo de ciclo de vida de la §5; Agent Card, perfiles y transporte siguen siendo futuros y quedan fuera.
- Consistencia de tipos: `TaskState`, `From<JobStatus>` y `From<SubagentStatus>` tienen el mismo nombre en tests e implementación.
- Proporción: el plan cabe en una entrega pequeña (un fichero nuevo, una línea en `lib.rs` y dos notas de spec).
