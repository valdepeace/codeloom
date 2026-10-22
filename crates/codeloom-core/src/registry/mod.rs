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
pub mod trust;
pub mod watch;

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use crate::event::{EventBus, LiveEvent};

pub use profile::{is_valid_agent_name, parse_profile, AgentProfile, ProfileProblem};
pub use source::{
    AgentEntry, AgentOrigin, AgentSource, BuiltInSource, Execution, FileSource, LoadProblem,
    LocalSpec, ProfileLevel, SourceLoad, AGENTE_POR_DEFECTO,
};
pub use trust::{content_hash, Approval, ApprovalError, TrustError, TrustStore};
pub use watch::WatcherHandle;

use trust::Confianza;

/// Vista inmutable del registro en un momento dado.
#[derive(Debug, Clone, PartialEq)]
pub struct RegistrySnapshot {
    entries: BTreeMap<String, AgentEntry>,
    problems: Vec<LoadProblem>,
    /// Confianza configurada en el registro. `None`: no hay almacén, así que ningún agente de
    /// proyecto se aprueba.
    confianza: Option<Confianza>,
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

    /// Estado de aprobación del agente `name`. `None` si no existe en la instantánea.
    ///
    /// La consulta lee el almacén en el momento de llamarla (no al cargar la instantánea):
    /// aprobar o revocar se refleja sin recargar los ficheros.
    pub fn approval(&self, name: &str) -> Option<Approval> {
        let entrada = self.entries.get(name)?;
        Some(match &entrada.origin {
            AgentOrigin::BuiltIn => Approval::NotRequired,
            AgentOrigin::File {
                level: ProfileLevel::Global,
                ..
            } => Approval::NotRequired,
            AgentOrigin::File {
                level: ProfileLevel::Project,
                ..
            } => match (&self.confianza, &entrada.content_hash) {
                (Some(confianza), Some(huella)) if confianza.aprueba(name, huella) => {
                    Approval::Approved
                }
                _ => Approval::Pending,
            },
            // Sin transporte todavía (A8): no hay forma de confiar en él. Denegar por defecto.
            AgentOrigin::Remote { .. } => Approval::Pending,
        })
    }

    /// Sección del prompt de sistema que enumera los agentes a los que la herramienta
    /// `subagent` puede delegar. Sin ella el modelo no tiene forma de saber cuáles existen.
    ///
    /// Los agentes que aún no se pueden usar (de proyecto sin aprobar, remotos sin transporte)
    /// no se ofrecen: solo se nombran aparte, para que el modelo no los intente invocar.
    pub fn prompt_section(&self) -> String {
        let mut disponibles = Vec::new();
        let mut pendientes = Vec::new();
        for entrada in self.entries() {
            let usable = matches!(
                self.approval(&entrada.name),
                Some(Approval::NotRequired | Approval::Approved)
            );
            if !usable {
                pendientes.push(entrada.name.as_str());
                continue;
            }
            let origen = match &entrada.origin {
                AgentOrigin::BuiltIn => "built-in",
                AgentOrigin::File {
                    level: ProfileLevel::Global,
                    ..
                } => "global",
                AgentOrigin::File {
                    level: ProfileLevel::Project,
                    ..
                } => "project",
                AgentOrigin::Remote { .. } => "remote",
            };
            let descripcion = entrada.card.description.trim();
            if descripcion.is_empty() {
                disponibles.push(format!("- {} ({origen})", entrada.name));
            } else {
                disponibles.push(format!("- {} ({origen}): {descripcion}", entrada.name));
            }
        }
        let mut texto = String::from(
            "## Available agents\n\
             The `subagent` tool delegates work to one of these agents through its optional \
             `agent` argument; omit it to use `default`.\n",
        );
        texto.push_str(&disponibles.join("\n"));
        if !pendientes.is_empty() {
            texto.push_str(&format!(
                "\nNot usable yet (awaiting approval: {}).",
                pendientes.join(", ")
            ));
        }
        texto
    }
}

/// Registro de agentes. Las fuentes se pasan de menor a mayor precedencia: la última gana.
pub struct AgentRegistry {
    sources: Vec<Box<dyn AgentSource>>,
    actual: RwLock<Arc<RegistrySnapshot>>,
    /// Serializa las recargas: cada una parte de la instantánea que dejó la anterior.
    recarga: Mutex<()>,
    confianza: Option<Confianza>,
}

impl AgentRegistry {
    /// Crea el registro y carga las fuentes una vez.
    pub fn new(sources: Vec<Box<dyn AgentSource>>) -> Self {
        let inicial = Arc::new(cargar(&sources, None, None));
        AgentRegistry {
            sources,
            actual: RwLock::new(inicial),
            recarga: Mutex::new(()),
            confianza: None,
        }
    }

    /// Configura la aprobación de los agentes de proyecto y recalcula la instantánea vigente.
    ///
    /// `project_root` se canoniza. Con `trust_all_projects`, todo agente de proyecto se
    /// considera aprobado (opción `trust_project_agents` de `codeloom.toml`).
    pub fn with_trust(
        mut self,
        almacen: TrustStore,
        project_root: PathBuf,
        trust_all_projects: bool,
    ) -> Self {
        self.confianza = Some(Confianza::new(almacen, &project_root, trust_all_projects));
        // La instantánea inicial se recalcula: la aprobación depende de la confianza.
        let inicial = Arc::new(cargar(&self.sources, None, self.confianza.as_ref()));
        *self.actual.get_mut().unwrap_or_else(|e| e.into_inner()) = inicial;
        self
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

    /// Aprueba el agente de proyecto `name` tal como está cargado ahora: la huella es la de la
    /// instantánea vigente, es decir, la del contenido que la persona usuaria ha podido ver.
    /// Si el fichero cambia después, la aprobación deja de valer en la recarga siguiente.
    pub fn approve(&self, name: &str) -> Result<(), ApprovalError> {
        let (confianza, huella) = self.agente_de_proyecto(name)?;
        Ok(confianza.aprobar(name, &huella)?)
    }

    /// Revoca la aprobación del agente de proyecto `name`. Si no estaba aprobado, no hace nada.
    pub fn revoke(&self, name: &str) -> Result<(), ApprovalError> {
        let (confianza, _) = self.agente_de_proyecto(name)?;
        Ok(confianza.revocar(name)?)
    }

    /// Comprueba que `name` existe y es de proyecto; devuelve la confianza y su huella.
    fn agente_de_proyecto(&self, name: &str) -> Result<(&Confianza, String), ApprovalError> {
        let instantanea = self.snapshot();
        let entrada = instantanea
            .get(name)
            .ok_or_else(|| ApprovalError::Desconocido(name.to_string()))?;
        let huella = match (&entrada.origin, &entrada.content_hash) {
            (
                AgentOrigin::File {
                    level: ProfileLevel::Project,
                    ..
                },
                Some(huella),
            ) => huella.clone(),
            _ => return Err(ApprovalError::NoRequerida(name.to_string())),
        };
        let confianza = self.confianza.as_ref().ok_or(ApprovalError::SinAlmacen)?;
        Ok((confianza, huella))
    }

    /// Recarga bajo el cerrojo de recargas. Devuelve (anterior, nueva).
    fn recargar(&self) -> (Arc<RegistrySnapshot>, Arc<RegistrySnapshot>) {
        let _turno = self.recarga.lock().unwrap_or_else(|e| e.into_inner());
        let antes = self.snapshot();
        let despues = Arc::new(cargar(&self.sources, Some(&antes), self.confianza.as_ref()));
        let mut actual = self.actual.write().unwrap_or_else(|e| e.into_inner());
        *actual = Arc::clone(&despues);
        (antes, despues)
    }
}

/// Carga las fuentes en orden y aplica la precedencia: la última que aporte un nombre gana,
/// y la entrada vencida queda anotada como `Shadowed`.
///
/// Con `previa`, aplica la regla de «última versión válida» (ver `conservar_ultimas_validas`).
fn cargar(
    sources: &[Box<dyn AgentSource>],
    previa: Option<&RegistrySnapshot>,
    confianza: Option<&Confianza>,
) -> RegistrySnapshot {
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
    RegistrySnapshot {
        entries,
        problems,
        confianza: confianza.cloned(),
    }
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
    fn prompt_section_lists_usable_agents_and_flags_the_pending_ones() {
        let global = tempfile::tempdir().unwrap();
        let proyecto = tempfile::tempdir().unwrap();
        escribir(global.path(), "rust.md", &perfil("rust", "Experto en Rust"));
        escribir(
            proyecto.path(),
            "front.md",
            &perfil("front", "Interfaz del proyecto"),
        );

        let seccion = registro_con_global_y_proyecto(global.path(), proyecto.path())
            .snapshot()
            .prompt_section();

        assert!(seccion.contains("`subagent`"), "{seccion}");
        assert!(seccion.contains("- default (built-in)"), "{seccion}");
        assert!(
            seccion.contains("- rust (global): Experto en Rust"),
            "{seccion}"
        );
        // El de proyecto sin aprobar no se ofrece como disponible, pero se nombra.
        assert!(!seccion.contains("- front"), "{seccion}");
        assert!(
            seccion.contains("awaiting approval: front"),
            "se debe avisar del agente pendiente: {seccion}"
        );
    }

    #[test]
    fn approve_and_revoke_flip_a_project_agent_and_a_change_invalidates_it() {
        let global = tempfile::tempdir().unwrap();
        let proyecto = tempfile::tempdir().unwrap();
        let datos = tempfile::tempdir().unwrap();
        escribir(global.path(), "rust.md", &perfil("rust", "Global"));
        escribir(proyecto.path(), "front.md", &perfil("front", "Interfaz"));
        let registro = registro_con_global_y_proyecto(global.path(), proyecto.path()).with_trust(
            TrustStore::open(datos.path().to_path_buf()).unwrap(),
            proyecto.path().to_path_buf(),
            false,
        );
        assert_eq!(
            registro.snapshot().approval("front"),
            Some(Approval::Pending)
        );

        registro.approve("front").unwrap();
        assert_eq!(
            registro.snapshot().approval("front"),
            Some(Approval::Approved)
        );

        // Un cambio del fichero invalida la aprobación al recargar.
        escribir(proyecto.path(), "front.md", &perfil("front", "Otra cosa"));
        assert_eq!(registro.reload().approval("front"), Some(Approval::Pending));

        registro.approve("front").unwrap();
        registro.revoke("front").unwrap();
        assert_eq!(
            registro.snapshot().approval("front"),
            Some(Approval::Pending)
        );

        assert!(matches!(
            registro.approve("rust"),
            Err(ApprovalError::NoRequerida(_))
        ));
        assert!(matches!(
            registro.approve("nadie"),
            Err(ApprovalError::Desconocido(_))
        ));
    }

    #[test]
    fn approve_without_a_trust_store_is_an_error() {
        let proyecto = tempfile::tempdir().unwrap();
        escribir(proyecto.path(), "front.md", &perfil("front", "Interfaz"));
        let registro = AgentRegistry::new(vec![
            Box::new(BuiltInSource),
            Box::new(FileSource::new(
                proyecto.path().to_path_buf(),
                ProfileLevel::Project,
            )),
        ]);
        assert!(matches!(
            registro.approve("front"),
            Err(ApprovalError::SinAlmacen)
        ));
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

    /// Registro con confianza configurada sobre `almacen` para la raíz `proyecto`.
    fn registro_con_confianza(
        global: &Path,
        proyecto: &Path,
        almacen: &TrustStore,
        confiar_todo: bool,
    ) -> AgentRegistry {
        registro_con_global_y_proyecto(global, proyecto).with_trust(
            almacen.clone(),
            proyecto.to_path_buf(),
            confiar_todo,
        )
    }

    #[test]
    fn global_and_builtin_do_not_need_approval() {
        let global = tempfile::tempdir().unwrap();
        let proyecto = tempfile::tempdir().unwrap();
        let datos = tempfile::tempdir().unwrap();
        escribir(global.path(), "frontend.md", &perfil("frontend", "UI"));
        let almacen = TrustStore::open(datos.path().to_path_buf()).unwrap();

        let instantanea =
            registro_con_confianza(global.path(), proyecto.path(), &almacen, false).snapshot();

        assert_eq!(
            instantanea.approval(AGENTE_POR_DEFECTO),
            Some(Approval::NotRequired)
        );
        assert_eq!(
            instantanea.approval("frontend"),
            Some(Approval::NotRequired)
        );
    }

    #[test]
    fn project_agent_is_pending_until_approved() {
        let global = tempfile::tempdir().unwrap();
        let proyecto = tempfile::tempdir().unwrap();
        let datos = tempfile::tempdir().unwrap();
        escribir(proyecto.path(), "revisor.md", &perfil("revisor", "Revisa"));
        let almacen = TrustStore::open(datos.path().to_path_buf()).unwrap();
        let registro = registro_con_confianza(global.path(), proyecto.path(), &almacen, false);

        let hash = registro
            .snapshot()
            .get("revisor")
            .unwrap()
            .content_hash
            .clone()
            .expect("un agente de fichero lleva huella");
        assert_eq!(
            registro.snapshot().approval("revisor"),
            Some(Approval::Pending)
        );

        almacen.approve(proyecto.path(), "revisor", &hash).unwrap();

        // Sin recargar: la consulta refleja el estado actual del almacén.
        assert_eq!(
            registro.snapshot().approval("revisor"),
            Some(Approval::Approved)
        );
    }

    #[test]
    fn project_agent_without_trust_config_is_pending() {
        let global = tempfile::tempdir().unwrap();
        let proyecto = tempfile::tempdir().unwrap();
        escribir(proyecto.path(), "revisor.md", &perfil("revisor", "Revisa"));

        let instantanea = registro_con_global_y_proyecto(global.path(), proyecto.path()).snapshot();

        assert_eq!(
            instantanea.approval("revisor"),
            Some(Approval::Pending),
            "sin almacén configurado nada de proyecto se aprueba"
        );
    }

    #[test]
    fn approval_is_invalidated_by_a_content_change() {
        let global = tempfile::tempdir().unwrap();
        let proyecto = tempfile::tempdir().unwrap();
        let datos = tempfile::tempdir().unwrap();
        escribir(proyecto.path(), "revisor.md", &perfil("revisor", "v1"));
        let almacen = TrustStore::open(datos.path().to_path_buf()).unwrap();
        let registro = registro_con_confianza(global.path(), proyecto.path(), &almacen, false);
        let hash_v1 = registro
            .snapshot()
            .get("revisor")
            .unwrap()
            .content_hash
            .clone()
            .unwrap();
        almacen
            .approve(proyecto.path(), "revisor", &hash_v1)
            .unwrap();

        // Un `git pull` que cambia el fichero: la aprobación anterior ya no vale.
        escribir(proyecto.path(), "revisor.md", &perfil("revisor", "v2"));
        registro.reload();

        assert_eq!(
            registro.snapshot().approval("revisor"),
            Some(Approval::Pending)
        );
    }

    #[test]
    fn approval_is_per_project_root() {
        let global = tempfile::tempdir().unwrap();
        let proyecto_a = tempfile::tempdir().unwrap();
        let proyecto_b = tempfile::tempdir().unwrap();
        let datos = tempfile::tempdir().unwrap();
        // Mismo contenido en ambos: la huella coincide, pero la raíz no.
        escribir(
            proyecto_a.path(),
            "revisor.md",
            &perfil("revisor", "Revisa"),
        );
        escribir(
            proyecto_b.path(),
            "revisor.md",
            &perfil("revisor", "Revisa"),
        );
        let almacen = TrustStore::open(datos.path().to_path_buf()).unwrap();
        let registro_a = registro_con_confianza(global.path(), proyecto_a.path(), &almacen, false);
        let hash = registro_a
            .snapshot()
            .get("revisor")
            .unwrap()
            .content_hash
            .clone()
            .unwrap();
        almacen
            .approve(proyecto_a.path(), "revisor", &hash)
            .unwrap();

        let registro_b = registro_con_confianza(global.path(), proyecto_b.path(), &almacen, false);

        assert_eq!(
            registro_a.snapshot().approval("revisor"),
            Some(Approval::Approved)
        );
        assert_eq!(
            registro_b.snapshot().approval("revisor"),
            Some(Approval::Pending)
        );
    }

    #[test]
    fn revoke_returns_it_to_pending() {
        let global = tempfile::tempdir().unwrap();
        let proyecto = tempfile::tempdir().unwrap();
        let datos = tempfile::tempdir().unwrap();
        escribir(proyecto.path(), "revisor.md", &perfil("revisor", "Revisa"));
        let almacen = TrustStore::open(datos.path().to_path_buf()).unwrap();
        let registro = registro_con_confianza(global.path(), proyecto.path(), &almacen, false);
        let hash = registro
            .snapshot()
            .get("revisor")
            .unwrap()
            .content_hash
            .clone()
            .unwrap();
        almacen.approve(proyecto.path(), "revisor", &hash).unwrap();

        almacen.revoke(proyecto.path(), "revisor").unwrap();

        assert_eq!(
            registro.snapshot().approval("revisor"),
            Some(Approval::Pending)
        );
    }

    #[test]
    fn trust_all_projects_skips_the_approval() {
        let global = tempfile::tempdir().unwrap();
        let proyecto = tempfile::tempdir().unwrap();
        let datos = tempfile::tempdir().unwrap();
        escribir(proyecto.path(), "revisor.md", &perfil("revisor", "Revisa"));
        let almacen = TrustStore::open(datos.path().to_path_buf()).unwrap();

        let registro = registro_con_confianza(global.path(), proyecto.path(), &almacen, true);

        assert_eq!(
            registro.snapshot().approval("revisor"),
            Some(Approval::Approved)
        );
    }

    #[test]
    fn approval_of_an_unknown_agent_is_none() {
        let global = tempfile::tempdir().unwrap();
        let proyecto = tempfile::tempdir().unwrap();
        let datos = tempfile::tempdir().unwrap();
        let almacen = TrustStore::open(datos.path().to_path_buf()).unwrap();

        let instantanea =
            registro_con_confianza(global.path(), proyecto.path(), &almacen, false).snapshot();

        assert_eq!(instantanea.approval("no-existe"), None);
    }
}
