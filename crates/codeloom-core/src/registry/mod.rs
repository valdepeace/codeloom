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
pub mod watch;

use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};

use crate::event::{EventBus, LiveEvent};

pub use profile::{is_valid_agent_name, parse_profile, AgentProfile, ProfileProblem};
pub use source::{
    AgentEntry, AgentOrigin, AgentSource, BuiltInSource, Execution, FileSource, LoadProblem,
    LocalSpec, ProfileLevel, SourceLoad, AGENTE_POR_DEFECTO,
};
pub use watch::WatcherHandle;

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
    /// Serializa las recargas: cada una parte de la instantánea que dejó la anterior.
    recarga: Mutex<()>,
}

impl AgentRegistry {
    /// Crea el registro y carga las fuentes una vez.
    pub fn new(sources: Vec<Box<dyn AgentSource>>) -> Self {
        let inicial = Arc::new(cargar(&sources, None));
        AgentRegistry {
            sources,
            actual: RwLock::new(inicial),
            recarga: Mutex::new(()),
        }
    }

    /// Vuelve a cargar todas las fuentes, publica la nueva instantánea y la devuelve.
    pub fn reload(&self) -> Arc<RegistrySnapshot> {
        self.recargar().1
    }

    /// Recarga y publica `AgentRegistryChanged` en `bus`, solo si hubo diferencias.
    pub fn reload_and_publish(&self, bus: &EventBus) -> Arc<RegistrySnapshot> {
        let (antes, despues) = self.recargar();
        if let Some(evento) = diferencias(&antes, &despues) {
            bus.publish(evento);
        }
        despues
    }

    /// Instantánea vigente. Las que ya se entregaron no cambian.
    pub fn snapshot(&self) -> Arc<RegistrySnapshot> {
        let actual = self.actual.read().unwrap_or_else(|e| e.into_inner());
        Arc::clone(&actual)
    }

    /// Recarga bajo el cerrojo de recargas. Devuelve (anterior, nueva).
    fn recargar(&self) -> (Arc<RegistrySnapshot>, Arc<RegistrySnapshot>) {
        let _turno = self.recarga.lock().unwrap_or_else(|e| e.into_inner());
        let antes = self.snapshot();
        let despues = Arc::new(cargar(&self.sources, Some(&antes)));
        let mut actual = self.actual.write().unwrap_or_else(|e| e.into_inner());
        *actual = Arc::clone(&despues);
        (antes, despues)
    }
}

/// Carga las fuentes en orden y aplica la precedencia: la última que aporte un nombre gana,
/// y la entrada vencida queda anotada como `Shadowed`.
///
/// Con `previa`, aplica la regla de «última versión válida» (ver `conservar_ultimas_validas`).
fn cargar(sources: &[Box<dyn AgentSource>], previa: Option<&RegistrySnapshot>) -> RegistrySnapshot {
    let mut entries: BTreeMap<String, AgentEntry> = BTreeMap::new();
    let mut problems = Vec::new();
    for source in sources {
        let mut carga = source.load();
        if let Some(previa) = previa {
            conservar_ultimas_validas(&mut carga, previa);
        }
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

/// Regla de «última versión válida». Si un fichero que cargaba antes falla ahora (o su
/// directorio es ilegible), su entrada anterior se reinyecta en esta carga y el problema se
/// conserva. Un fichero borrado no produce problema: su entrada desaparece.
fn conservar_ultimas_validas(carga: &mut SourceLoad, previa: &RegistrySnapshot) {
    let fallidas: HashSet<&Path> = carga
        .problems
        .iter()
        .filter_map(|problema| match problema {
            LoadProblem::Profile { path, .. } | LoadProblem::Unreadable { path, .. } => {
                Some(path.as_path())
            }
            LoadProblem::Shadowed { .. } => None,
        })
        .collect();

    let mut retenidas = Vec::new();
    for entrada in previa.entries.values() {
        let AgentOrigin::File { path, .. } = &entrada.origin else {
            continue;
        };
        let afectada = fallidas.contains(path.as_path())
            || path.parent().is_some_and(|dir| fallidas.contains(dir));
        let presente = carga
            .entries
            .iter()
            .any(|nueva| matches!(&nueva.origin, AgentOrigin::File { path: p, .. } if p == path));
        if afectada && !presente {
            retenidas.push(entrada.clone());
        }
    }
    carga.entries.extend(retenidas);
}

/// Diferencias entre dos instantáneas. `None` si no cambió nada visible: ni entradas ni
/// problemas.
fn diferencias(antes: &RegistrySnapshot, despues: &RegistrySnapshot) -> Option<LiveEvent> {
    let added: Vec<String> = despues
        .entries
        .keys()
        .filter(|nombre| !antes.entries.contains_key(*nombre))
        .cloned()
        .collect();
    let removed: Vec<String> = antes
        .entries
        .keys()
        .filter(|nombre| !despues.entries.contains_key(*nombre))
        .cloned()
        .collect();
    let changed: Vec<String> = despues
        .entries
        .iter()
        .filter(|(nombre, nueva)| {
            antes
                .entries
                .get(*nombre)
                .is_some_and(|previa| previa != *nueva)
        })
        .map(|(nombre, _)| nombre.clone())
        .collect();

    let cambio_de_problemas = antes.problems != despues.problems;
    if added.is_empty() && removed.is_empty() && changed.is_empty() && !cambio_de_problemas {
        return None;
    }
    Some(LiveEvent::AgentRegistryChanged {
        added,
        removed,
        changed,
        problems: despues.problems.len(),
    })
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
                persona_role: None,
                persona_goal: None,
                persona_backstory: None,
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

    /// Bus con un suscriptor y el evento que llegue, si llega alguno.
    fn ultimo_evento(rx: &mut tokio::sync::broadcast::Receiver<LiveEvent>) -> Option<LiveEvent> {
        rx.try_recv().ok()
    }

    #[test]
    fn new_file_is_added_and_published() {
        let global = tempfile::tempdir().unwrap();
        let proyecto = tempfile::tempdir().unwrap();
        let registro = registro_con_global_y_proyecto(global.path(), proyecto.path());
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();

        escribir(global.path(), "frontend.md", &perfil("frontend", "UI"));
        registro.reload_and_publish(&bus);

        assert!(registro.snapshot().get("frontend").is_some());
        match ultimo_evento(&mut rx) {
            Some(LiveEvent::AgentRegistryChanged {
                added,
                removed,
                changed,
                problems,
            }) => {
                assert_eq!(added, vec!["frontend".to_string()]);
                assert!(removed.is_empty());
                assert!(changed.is_empty());
                assert_eq!(problems, 0);
            }
            otro => panic!("se esperaba AgentRegistryChanged, llegó {otro:?}"),
        }
    }

    #[test]
    fn edited_file_is_reported_as_changed() {
        let global = tempfile::tempdir().unwrap();
        let proyecto = tempfile::tempdir().unwrap();
        escribir(global.path(), "frontend.md", &perfil("frontend", "v1"));
        let registro = registro_con_global_y_proyecto(global.path(), proyecto.path());
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();

        escribir(global.path(), "frontend.md", &perfil("frontend", "v2"));
        registro.reload_and_publish(&bus);

        match ultimo_evento(&mut rx) {
            Some(LiveEvent::AgentRegistryChanged {
                added,
                removed,
                changed,
                ..
            }) => {
                assert!(added.is_empty());
                assert!(removed.is_empty());
                assert_eq!(changed, vec!["frontend".to_string()]);
            }
            otro => panic!("se esperaba AgentRegistryChanged, llegó {otro:?}"),
        }
    }

    #[test]
    fn deleted_file_is_removed() {
        let global = tempfile::tempdir().unwrap();
        let proyecto = tempfile::tempdir().unwrap();
        escribir(global.path(), "frontend.md", &perfil("frontend", "UI"));
        let registro = registro_con_global_y_proyecto(global.path(), proyecto.path());
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();

        std::fs::remove_file(global.path().join("frontend.md")).unwrap();
        registro.reload_and_publish(&bus);

        assert!(registro.snapshot().get("frontend").is_none());
        match ultimo_evento(&mut rx) {
            Some(LiveEvent::AgentRegistryChanged { removed, .. }) => {
                assert_eq!(removed, vec!["frontend".to_string()]);
            }
            otro => panic!("se esperaba AgentRegistryChanged, llegó {otro:?}"),
        }
    }

    #[test]
    fn broken_edit_keeps_the_last_valid_version_and_reports_the_problem() {
        let global = tempfile::tempdir().unwrap();
        let proyecto = tempfile::tempdir().unwrap();
        escribir(global.path(), "frontend.md", &perfil("frontend", "buena"));
        let registro = registro_con_global_y_proyecto(global.path(), proyecto.path());
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();

        // Guardado a medias: el frontmatter queda sin cerrar.
        escribir(
            global.path(),
            "frontend.md",
            "---\nname: frontend\ndescription: ",
        );
        registro.reload_and_publish(&bus);

        let instantanea = registro.snapshot();
        let entrada = instantanea
            .get("frontend")
            .expect("la última versión válida debe conservarse");
        assert_eq!(entrada.card.description, "buena");
        assert!(instantanea.problems().iter().any(|p| matches!(
            p,
            LoadProblem::Profile { path, .. } if path == &global.path().join("frontend.md")
        )));
        match ultimo_evento(&mut rx) {
            Some(LiveEvent::AgentRegistryChanged {
                added,
                removed,
                changed,
                problems,
            }) => {
                assert!(added.is_empty() && removed.is_empty() && changed.is_empty());
                assert_eq!(problems, 1);
            }
            otro => panic!("se esperaba AgentRegistryChanged por el problema, llegó {otro:?}"),
        }
    }

    #[test]
    fn unchanged_reload_publishes_nothing() {
        let global = tempfile::tempdir().unwrap();
        let proyecto = tempfile::tempdir().unwrap();
        escribir(global.path(), "frontend.md", &perfil("frontend", "UI"));
        let registro = registro_con_global_y_proyecto(global.path(), proyecto.path());
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();

        registro.reload_and_publish(&bus);

        assert!(ultimo_evento(&mut rx).is_none());
    }
}
