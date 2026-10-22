//! Registro de agentes: perfiles `<nombre>.md` de las fuentes y su resolución.
//!
//! De momento solo se lee y valida un perfil (`profile`). Las fuentes, la precedencia y la
//! recarga llegan en tareas siguientes de `docs/plans/2026-10-22-codeloom-a2a-agent-registry.md`.

pub mod profile;

pub use profile::{is_valid_agent_name, parse_profile, AgentProfile, ProfileProblem};
