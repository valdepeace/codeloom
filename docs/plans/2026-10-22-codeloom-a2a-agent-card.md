# La tarjeta del agente — Plan de implementación

> **Para agentes:** una tarea cada vez, en orden. Quien supervisa commitea, así que **no commitees**.

**Meta:** tipos de `AgentCard` del proto en `codeloom-core`, con un constructor para agentes locales.

**Arquitectura:** módulo nuevo `card.rs`, aditivo, sin tocar nada existente. Reutiliza `serde`.

**Spec:** `docs/specs/2026-10-22-codeloom-a2a-agent-card-design.md` (**leerla entera antes**). Manda: **no inventar nada que el proto no contemple**; lo omitido (seguridad, firmas, icono) se dice, no se improvisa.

## Restricciones globales

- `codeloom-core` no depende de `ratatui`/`crossterm`. **Sin red en tests.**
- Comprobaciones: `cargo build`, `cargo test -p codeloom-core`, `cargo clippy -p codeloom-core -- -D warnings`, `rustfmt --edition 2021 --check <ficheros tocados>`.
- Entorno de build (Git Bash): `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe' CARGO_TARGET_DIR='E:/workspace/personal/loom/target' CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`. No crear otros `target*`. Formatear solo con `rustfmt --edition 2021 <ruta>`. Ficheros en LF.
- Comentarios en español «pijo», sin tacos ni andalucismos. No `git checkout/restore/stash/reset`. No tocar `codeloom.toml`.

## Review Focus

- `streaming: None` (no declarado) y `Some(false)` son distintos tras ida y vuelta serde.
- Una tarjeta local no es servible; una con interfaces sí lo es.
- Una skill sin `examples` ni modos se serializa y deserializa sin error (campos opcionales con `#[serde(default)]`).

---

### Tarea 1: tipos de la tarjeta

**Ficheros:** crear `crates/codeloom-core/src/card.rs`; declarar `pub mod card;` en `lib.rs`.

**Interfaces — produce** (todos `Debug, Clone, PartialEq, Serialize, Deserialize`, con `#[serde(default)]` en lo opcional):
```rust
pub struct AgentProvider { pub url: String, pub organization: String }
pub struct AgentExtension { pub uri: String, pub description: String, pub required: bool, pub params: Option<serde_json::Value> }
pub struct AgentCapabilities { pub streaming: Option<bool>, pub push_notifications: Option<bool>,
                               pub extensions: Vec<AgentExtension>, pub extended_agent_card: Option<bool> }
pub struct AgentInterface { pub url: String, pub protocol_binding: String, pub tenant: Option<String>, pub protocol_version: String }
pub struct AgentSkill { pub id: String, pub name: String, pub description: String, pub tags: Vec<String>,
                        pub examples: Vec<String>, pub input_modes: Vec<String>, pub output_modes: Vec<String> }
pub struct AgentCard { pub name: String, pub description: String, pub supported_interfaces: Vec<AgentInterface>,
                       pub provider: Option<AgentProvider>, pub version: String, pub documentation_url: Option<String>,
                       pub capabilities: AgentCapabilities, pub default_input_modes: Vec<String>,
                       pub default_output_modes: Vec<String>, pub skills: Vec<AgentSkill> }
impl AgentSkill { pub fn new(id, name, description, tags: Vec<String>) -> Self }   // el resto vacío
impl AgentCard {
    pub fn local(name: impl Into<String>, description: impl Into<String>, version: impl Into<String>) -> Self
        // interfaces vacías; capacidades sin declarar; modos ["text/plain"] de entrada y salida; sin skills
    pub fn with_skill(self, skill: AgentSkill) -> Self
    pub fn is_servable(&self) -> bool   // true si hay al menos una interfaz
}
```
Cada tipo lleva un comentario que nombra el mensaje del proto del que procede y deja constancia de lo omitido (seguridad, firmas, icono) en `AgentCard`.

- [ ] **Paso 1: tests en rojo (por aserción: deja los cuerpos en `todo!()`):** `card_roundtrips_through_serde`, `local_card_has_declared_defaults`, `local_card_is_not_servable`, `card_with_an_interface_is_servable`, `with_skill_appends_a_skill_with_its_tags`, `capabilities_distinguish_undeclared_from_false`, `skill_without_optional_fields_roundtrips`.
- [ ] **Paso 2: implementar. Paso 3: verificar** `cargo test -p codeloom-core`, clippy, rustfmt, LF. **Paso 4: no commitear.**

---

### Tarea 2: documentación

**Ficheros:** `docs/specs/2026-10-22-codeloom-a2a-agent-card-design.md`, `docs/specs/2026-10-15-loom-agent-profile-a2a.md` (§4), `docs/plans/2026-10-22-codeloom-a2a-roadmap.md`.

- [ ] **Paso 1:** `## Estado (fecha)` en la spec de la tarjeta (implementado, decisiones, limitaciones reales). En la spec del perfil §4, nota fechada: los tipos de la tarjeta ya existen en `card.rs`; el cargador de `AgentProfile` **no** existe aún. En la hoja de ruta, A6 [x] con commits, y apuntar como pendiente nuevo el cargador de `AgentProfile` desde fichero (prerrequisito de A12) y la entrada de seguridad/firmas de la tarjeta (A8).
- [ ] **Paso 2: verificar** LF. **Paso 3: no commitear.**
