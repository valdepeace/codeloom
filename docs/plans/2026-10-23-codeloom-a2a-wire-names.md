# Adaptador de nombres del cable (A4): plan de implementación

> **Para quien ejecute:** una tarea cada vez; tras cada una, `cargo test -p codeloom-a2a`,
> `cargo clippy -p codeloom-a2a -- -D warnings`, `rustfmt --edition 2021 --check` sobre los
> ficheros tocados y un commit. Nada de `cargo fmt --all`.

**Objetivo:** traducir de ida y vuelta los tipos de A2A del núcleo a los nombres del cable v1.0.

**Spec:** `docs/specs/2026-10-23-codeloom-a2a-wire-names-design.md`.

## Restricciones globales

- `codeloom-core` no cambia ni depende del nuevo crate. Errores con `thiserror`.
- Sin red ni sockets. Sin dependencias de fechas: la marca de tiempo se calcula a mano.
- El crate entra en el espacio de trabajo pero no en `codeloom-cli`.

## Revisión: lo que la spec implica y ninguna tarea cubre sola

- Una `Part` con dos claves de contenido, o con ninguna, no es válida al entrar.
- `TASK_STATE_UNSPECIFIED` y las cadenas de la v0.3 (`working`) se rechazan al entrar.
- Los opcionales ausentes y las listas vacías no aparecen en el JSON de salida.
- Un campo desconocido en la entrada no rompe.
- Un `raw` que no sea base64 válido es error.

### Tarea 1: el crate y los enumerados

**Ficheros:** `Cargo.toml` (miembro nuevo), `crates/codeloom-a2a/Cargo.toml`, `src/lib.rs`,
`src/wire.rs`.

**Interfaces:** produce `WireError` (`thiserror`), `state_to_wire(TaskState) -> &'static str`,
`state_from_wire(&str) -> Result<TaskState, WireError>`, `WireRole` (serde `ROLE_USER` /
`ROLE_AGENT`) con `From<Role>` y `From<WireRole> for Role`, y `timestamp_to_wire(i64) -> String` /
`timestamp_from_wire(&str) -> Result<i64, WireError>`.

- [ ] Tests: `states_roundtrip_through_wire_names` (los ocho), `unspecified_and_v03_states_are_rejected`,
  `role_serializes_as_proto_name`, `timestamps_roundtrip` (0, 1 ms, 2026-10-23 y un año bisiesto),
  `timestamp_has_millisecond_precision_and_z`, `timestamp_before_1970_or_with_offset_is_rejected`.
- [ ] Implementar y verificar; commitear.

### Tarea 2: `Part`, `Message` y `Artifact`

**Ficheros:** `crates/codeloom-a2a/src/wire.rs`; añade `base64`.

**Interfaces:** consume la tarea 1. Produce `WirePart`, `WireMessage`, `WireArtifact`
(`Serialize`/`Deserialize`, `camelCase`, `skip_serializing_if` para ausentes y vacíos), con
`From<&core>` hacia el cable y `TryFrom<WireX>` desde él.

- [ ] Tests: `part_text_serializes_with_only_the_text_key`, `part_raw_is_base64`,
  `part_with_two_content_keys_is_rejected`, `part_without_content_is_rejected`,
  `invalid_base64_raw_is_rejected`, `message_uses_camel_case_and_omits_empty_fields`,
  `message_roundtrips`, `artifact_roundtrips`, `unknown_fields_are_ignored`.
- [ ] Implementar y verificar; commitear.

### Tarea 3: `TaskStatus` y `Task`

**Ficheros:** `crates/codeloom-a2a/src/wire.rs`.

**Interfaces:** produce `WireTaskStatus`, `WireTask`, con `From<&TaskStatus>` y
`From<&SubagentDescriptor> for WireTask`.

- [ ] Tests: `task_from_descriptor_has_id_context_status_and_artifacts`,
  `task_without_artifacts_omits_them`, `status_message_and_timestamp_are_translated`.
- [ ] Implementar, verificar y commitear.

### Tarea 4: documentos

- [ ] Sección «Estado» de la spec y roadmap (A4 [x]). Commitear.
