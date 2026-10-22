//! Registro de agentes: perfiles `<nombre>.md` de las fuentes, precedencia y resolución.
//!
//! El registro reúne agentes de varias fuentes (`source`), aplica la precedencia y entrega
//! instantáneas inmutables (`RegistrySnapshot`). Una instantánea no cambia nunca: una
//! recarga produce otra, y quien ya tiene una (p. ej. una tarea en curso) la conserva.
//!
//! La recarga en caliente, la vigilancia y la aprobación de los agentes de proyecto llegan
//! en tareas siguientes de `docs/plans/2026-10-22-codeloom-a2a-agent-registry.md`.

pub mod profile;
pub mod source;

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

pub use profile::{is_valid_agent_name, parse_profile, AgentProfile, ProfileProblem};
pub use source::{
    AgentEntry, AgentOrigin, AgentSource, BuiltInSource, Execution, FileSource, LoadProblem,
    LocalSpec, ProfileLevel, SourceLoad, AGENTE_POR_DEFECTO,
};

/// Vista inmutable del registro en un momento dado.
#[derive(Debug, Clone, PartialEq)]
pub struct RegistrySnapshot {
    entries: BTreeMap<String, AgentEntry>,
    problems: Vec<LoadProblem>,
}

impl RegistrySnapshot {
    /// Agente con ese nombre, si existe en esta instantánea.
    pub fn get(&self, name: &str) -> Option<&AgentEntry> {
        self.entries.get(name)
    }

    /// Todos los agentes, ordenados por nombre.
    pub fn entries(&self) -> Vec<&AgentEntry> {
        self.entries.values().collect()
    }

    /// Problemas de carga de esta instantánea (avisos, sombreados, ilegibles).
    pub fn problems(&self) -> &[LoadProblem] {
        &self.problems
    }
}

/// Registro de agentes. Las fuentes se pasan de menor a mayor precedencia: la última gana.
pub struct AgentRegistry {
    sources: Vec<Box<dyn AgentSource>>,
    actual: RwLock<Arc<RegistrySnapshot>>,
}

impl AgentRegistry {
    /// Crea el registro y carga las fuentes una vez.
    pub fn new(sources: Vec<Box<dyn AgentSource>>) -> Self {
        let inicial = Arc::new(cargar(&sources));
        AgentRegistry {
            sources,
            actual: RwLock::new(inicial),
        }
    }

    /// Vuelve a cargar todas las fuentes, publica la nueva instantánea y la devuelve.
    pub fn reload(&self) -> Arc<RegistrySnapshot> {
        let nueva = Arc::new(cargar(&self.sources));
        let mut actual = self.actual.write().unwrap_or_else(|e| e.into_inner());
        *actual = Arc::clone(&nueva);
        nueva
    }

    /// Instantánea vigente. Las que ya se entregaron no cambian.
    pub fn snapshot(&self) -> Arc<RegistrySnapshot> {
        let actual = self.actual.read().unwrap_or_else(|e| e.into_inner());
        Arc::clone(&actual)
    }
}

/// Carga las fuentes en orden y aplica la precedencia: la última que aporte un nombre gana,
/// y la entrada vencida queda anotada como `Shadowed`.
fn cargar(sources: &[Box<dyn AgentSource>]) -> RegistrySnapshot {
    let mut entries: BTreeMap<String, AgentEntry> = BTreeMap::new();
    let mut problems = Vec::new();
    for source in sources {
        let carga = source.load();
        problems.extend(carga.problems);
        for entry in carga.entries {
            if let Some(vencida) = entries.remove(&entry.name) {
                problems.push(LoadProblem::Shadowed {
                    name: entry.name.clone(),
                    by: entry.origin.clone(),
                    loser: vencida.origin,
                });
            }
            entries.insert(entry.name.clone(), entry);
        }
    }
    RegistrySnapshot { entries, problems }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::AgentCard;
    use std::path::Path;

    fn escribir(dir: &Path, nombre: &str, texto: &str) {
        std::fs::write(dir.join(nombre), texto).unwrap();
    }

    fn perfil(nombre: &str, descripcion: &str) -> String {
        format!("---\nname: {nombre}\ndescription: {descripcion}\n---\n\nCuerpo de {nombre}.\n")
    }

    fn registro_con_global_y_proyecto(global: &Path, proyecto: &Path) -> AgentRegistry {
        AgentRegistry::new(vec![
            Box::new(BuiltInSource),
            Box::new(FileSource::new(global.to_path_buf(), ProfileLevel::Global)),
            Box::new(FileSource::new(
                proyecto.to_path_buf(),
                ProfileLevel::Project,
            )),
        ])
    }

    #[test]
    fn builtin_default_is_always_present() {
        let registro = AgentRegistry::new(vec![Box::new(BuiltInSource)]);
        let instantanea = registro.snapshot();

        let defecto = instantanea
            .get(AGENTE_POR_DEFECTO)
            .expect("el agente `default` debe existir siempre");
        assert_eq!(defecto.origin, AgentOrigin::BuiltIn);
        assert_eq!(
            defecto.execution,
            Execution::Local(LocalSpec {
                body: String::new(),
                model: None,
                effort: None,
                max_turns: None,
                max_depth: None,
            })
        );
        assert!(instantanea.problems().is_empty());
    }

    #[test]
    fn project_overrides_global_overrides_builtin() {
        let global = tempfile::tempdir().unwrap();
        let proyecto = tempfile::tempdir().unwrap();
        escribir(global.path(), "frontend.md", &perfil("frontend", "global"));
        escribir(
            global.path(),
            "default.md",
            &perfil("default", "por defecto global"),
        );
        escribir(
            proyecto.path(),
            "frontend.md",
            &perfil("frontend", "proyecto"),
        );

        let instantanea = registro_con_global_y_proyecto(global.path(), proyecto.path()).snapshot();

        let frontend = instantanea.get("frontend").unwrap();
        assert_eq!(frontend.card.description, "proyecto");
        assert_eq!(
            frontend.origin,
            AgentOrigin::File {
                path: proyecto.path().join("frontend.md"),
                level: ProfileLevel::Project,
            }
        );
        let defecto = instantanea.get(AGENTE_POR_DEFECTO).unwrap();
        assert_eq!(
            defecto.origin,
            AgentOrigin::File {
                path: global.path().join("default.md"),
                level: ProfileLevel::Global,
            }
        );
    }

    #[test]
    fn overridden_agent_is_reported_as_shadowed() {
        let global = tempfile::tempdir().unwrap();
        let proyecto = tempfile::tempdir().unwrap();
        escribir(global.path(), "frontend.md", &perfil("frontend", "global"));
        escribir(
            proyecto.path(),
            "frontend.md",
            &perfil("frontend", "proyecto"),
        );

        let instantanea = registro_con_global_y_proyecto(global.path(), proyecto.path()).snapshot();

        let esperado = LoadProblem::Shadowed {
            name: "frontend".to_string(),
            by: AgentOrigin::File {
                path: proyecto.path().join("frontend.md"),
                level: ProfileLevel::Project,
            },
            loser: AgentOrigin::File {
                path: global.path().join("frontend.md"),
                level: ProfileLevel::Global,
            },
        };
        assert!(instantanea.problems().contains(&esperado));
    }

    #[test]
    fn default_can_be_redefined_by_a_file() {
        let global = tempfile::tempdir().unwrap();
        let proyecto = tempfile::tempdir().unwrap();
        escribir(
            global.path(),
            "default.md",
            &perfil("default", "mi defecto"),
        );

        let instantanea = registro_con_global_y_proyecto(global.path(), proyecto.path()).snapshot();

        let defecto = instantanea.get(AGENTE_POR_DEFECTO).unwrap();
        assert_eq!(defecto.card.description, "mi defecto");
        assert!(matches!(defecto.origin, AgentOrigin::File { .. }));
        assert!(instantanea.problems().contains(&LoadProblem::Shadowed {
            name: AGENTE_POR_DEFECTO.to_string(),
            by: AgentOrigin::File {
                path: global.path().join("default.md"),
                level: ProfileLevel::Global,
            },
            loser: AgentOrigin::BuiltIn,
        }));
    }

    #[test]
    fn card_is_synthesized_without_persona_model_or_effort() {
        let global = tempfile::tempdir().unwrap();
        let proyecto = tempfile::tempdir().unwrap();
        escribir(
            global.path(),
            "frontend.md",
            "---\nname: frontend\ndescription: UI\nversion: 2\npersona:\n  role: Ingeniero\nmodel: claude-sonnet\neffort: high\n---\n\nCuerpo.\n",
        );

        let instantanea = registro_con_global_y_proyecto(global.path(), proyecto.path()).snapshot();

        let entrada = instantanea.get("frontend").unwrap();
        assert_eq!(entrada.card, AgentCard::local("frontend", "UI", "2"));
        assert!(entrada.card.skills.is_empty());
        // El modelo y el esfuerzo sí existen, pero solo en la ejecución local.
        match &entrada.execution {
            Execution::Local(spec) => {
                assert_eq!(spec.model.as_deref(), Some("claude-sonnet"));
                assert_eq!(spec.effort.as_deref(), Some("high"));
            }
            Execution::Remote => panic!("un perfil de fichero es local"),
        }
    }

    #[test]
    fn card_is_not_servable() {
        let global = tempfile::tempdir().unwrap();
        let proyecto = tempfile::tempdir().unwrap();
        escribir(global.path(), "frontend.md", &perfil("frontend", "UI"));

        let instantanea = registro_con_global_y_proyecto(global.path(), proyecto.path()).snapshot();

        assert!(!instantanea.get("frontend").unwrap().card.is_servable());
        assert!(!instantanea
            .get(AGENTE_POR_DEFECTO)
            .unwrap()
            .card
            .is_servable());
    }

    #[test]
    fn invalid_profile_is_reported_and_others_still_load() {
        let global = tempfile::tempdir().unwrap();
        let proyecto = tempfile::tempdir().unwrap();
        escribir(global.path(), "roto.md", "sin frontmatter\n");
        escribir(global.path(), "bueno.md", &perfil("bueno", "Carga bien"));

        let instantanea = registro_con_global_y_proyecto(global.path(), proyecto.path()).snapshot();

        assert!(instantanea.get("bueno").is_some());
        assert!(instantanea.get("roto").is_none());
        assert!(instantanea.problems().iter().any(|p| matches!(
            p,
            LoadProblem::Profile { path, problem: ProfileProblem::BadFrontmatter(_) }
                if path == &global.path().join("roto.md")
        )));
    }

    #[test]
    fn snapshot_is_immutable_across_reload() {
        let global = tempfile::tempdir().unwrap();
        let proyecto = tempfile::tempdir().unwrap();
        escribir(global.path(), "frontend.md", &perfil("frontend", "v1"));
        let registro = registro_con_global_y_proyecto(global.path(), proyecto.path());
        let antes = registro.snapshot();

        escribir(global.path(), "frontend.md", &perfil("frontend", "v2"));
        let despues = registro.reload();

        assert_eq!(antes.get("frontend").unwrap().card.description, "v1");
        assert_eq!(despues.get("frontend").unwrap().card.description, "v2");
        assert_eq!(
            registro
                .snapshot()
                .get("frontend")
                .unwrap()
                .card
                .description,
            "v2"
        );
    }
}
