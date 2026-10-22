//! Fuentes de agentes: de dónde sale cada agente y cómo se lee.
//!
//! Una fuente produce entradas (`AgentEntry`) y problemas (`LoadProblem`) sin conocer la
//! precedencia: eso lo resuelve `AgentRegistry` (en `mod.rs`). Así añadir una fuente nueva,
//! como la remota de A8, no obliga a tocar el registro.

use std::path::PathBuf;

use crate::card::AgentCard;
use crate::registry::profile::{parse_profile, AgentProfile, ProfileProblem};
use crate::registry::trust::content_hash;

/// Nombre del agente por defecto, integrado. Un `default.md` de proyecto o global lo redefine.
pub const AGENTE_POR_DEFECTO: &str = "default";

/// Nivel de un perfil de fichero. El integrado no tiene nivel: es `AgentOrigin::BuiltIn`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileLevel {
    Global,
    Project,
}

/// De dónde viene un agente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentOrigin {
    BuiltIn,
    File {
        path: PathBuf,
        level: ProfileLevel,
    },
    /// Agente remoto (A8). El tipo existe para no rehacer el registro; nada lo produce aún.
    Remote {
        card_url: String,
    },
}

impl AgentOrigin {
    /// Texto estable del origen, tal como se guarda en el descriptor del subagente:
    /// `built-in`, `global:<ruta>`, `project:<ruta>` o `remote:<url>`.
    pub fn as_text(&self) -> String {
        match self {
            AgentOrigin::BuiltIn => "built-in".to_string(),
            AgentOrigin::File { path, level } => {
                let nivel = match level {
                    ProfileLevel::Global => "global",
                    ProfileLevel::Project => "project",
                };
                format!("{nivel}:{}", path.display())
            }
            AgentOrigin::Remote { card_url } => format!("remote:{card_url}"),
        }
    }
}

/// Configuración de ejecución local. No viaja en la tarjeta.
///
/// `body` es el prompt de sistema. Cadena vacía y sin modelo ni esfuerzo significa «heredar
/// del padre» (es lo que hace el `default` integrado).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalSpec {
    pub body: String,
    /// Rol de la persona (`persona.role` del perfil).
    pub persona_role: Option<String>,
    /// Objetivo de la persona (`persona.goal` del perfil).
    pub persona_goal: Option<String>,
    /// Contexto de la persona (`persona.backstory` del perfil).
    pub persona_backstory: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub max_turns: Option<u32>,
    pub max_depth: Option<u32>,
}

impl LocalSpec {
    /// Prompt de sistema que recibe la sesión hija.
    ///
    /// Con persona, antepone un bloque con una línea por cada campo presente (`Rol`,
    /// `Objetivo`, `Contexto`), una línea en blanco y el cuerpo. Sin persona devuelve el
    /// cuerpo tal cual; sin persona y con cuerpo vacío, cadena vacía (herencia del padre).
    pub fn system_prompt(&self) -> String {
        // Un campo de persona vacío cuenta como ausente: no aporta nada al prompt.
        let lineas: Vec<String> = [
            ("Rol", self.persona_role.as_deref()),
            ("Objetivo", self.persona_goal.as_deref()),
            ("Contexto", self.persona_backstory.as_deref()),
        ]
        .into_iter()
        .filter_map(|(etiqueta, valor)| {
            valor
                .filter(|v| !v.trim().is_empty())
                .map(|v| format!("{etiqueta}: {v}"))
        })
        .collect();

        if lineas.is_empty() {
            return self.body.clone();
        }
        let bloque = lineas.join("\n");
        if self.body.is_empty() {
            bloque
        } else {
            format!("{bloque}\n\n{}", self.body)
        }
    }
}

/// Cómo se ejecuta un agente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Execution {
    Local(LocalSpec),
    /// Se habla por transporte (A8). Todavía sin implementación.
    Remote,
}

/// Un agente del registro.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentEntry {
    /// Identificador único dentro del registro.
    pub name: String,
    pub origin: AgentOrigin,
    /// Lo que se anuncia (A6).
    pub card: AgentCard,
    /// Lo local, que no viaja en la tarjeta.
    pub execution: Execution,
    /// Huella del contenido del fichero (`trust::content_hash`). Solo los agentes de fichero
    /// la llevan; el integrado y los remotos, `None`. La aprobación de un agente de proyecto
    /// depende de ella: cualquier byte cambiado la invalida.
    pub content_hash: Option<String>,
}

/// Problema detectado al cargar. Nunca se silencia: viaja en la instantánea.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadProblem {
    /// Un perfil no se pudo cargar, o tiene un aviso (campo desconocido, `scope` declarado).
    Profile {
        path: PathBuf,
        problem: ProfileProblem,
    },
    /// Otro agente con el mismo nombre tiene más precedencia; `loser` queda oculto.
    Shadowed {
        name: String,
        by: AgentOrigin,
        loser: AgentOrigin,
    },
    /// Un fichero o directorio no se pudo leer.
    Unreadable { path: PathBuf, error: String },
}

/// Resultado de cargar una fuente.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceLoad {
    pub entries: Vec<AgentEntry>,
    pub problems: Vec<LoadProblem>,
}

/// Fuente de agentes. Las fuentes se pasan al registro de menor a mayor precedencia.
pub trait AgentSource: Send + Sync {
    fn id(&self) -> &str;
    fn load(&self) -> SourceLoad;
}

/// Fuente de ficheros `<nombre>.md` de un directorio. No es recursiva; un directorio
/// inexistente da cero entradas y ningún problema.
pub struct FileSource {
    dir: PathBuf,
    level: ProfileLevel,
}

impl FileSource {
    pub fn new(dir: PathBuf, level: ProfileLevel) -> Self {
        FileSource { dir, level }
    }
}

impl AgentSource for FileSource {
    fn id(&self) -> &str {
        match self.level {
            ProfileLevel::Global => "global",
            ProfileLevel::Project => "project",
        }
    }

    fn load(&self) -> SourceLoad {
        let mut entries = Vec::new();
        let mut problems = Vec::new();

        let lectura = match std::fs::read_dir(&self.dir) {
            Ok(lectura) => lectura,
            // Un directorio que no existe es normal: no hay agentes de ese nivel.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return SourceLoad { entries, problems };
            }
            Err(error) => {
                problems.push(LoadProblem::Unreadable {
                    path: self.dir.clone(),
                    error: error.to_string(),
                });
                return SourceLoad { entries, problems };
            }
        };

        // El orden de `read_dir` no está garantizado: se ordena para que la carga sea estable.
        let mut rutas = Vec::new();
        for dirent in lectura {
            match dirent {
                Ok(dirent) => rutas.push(dirent.path()),
                Err(error) => problems.push(LoadProblem::Unreadable {
                    path: self.dir.clone(),
                    error: error.to_string(),
                }),
            }
        }
        rutas.sort();

        for ruta in rutas {
            let es_perfil = ruta.extension().and_then(|e| e.to_str()) == Some("md");
            if !es_perfil || !ruta.is_file() {
                continue;
            }
            // El nombre del agente es el del fichero; si no es válido, `parse_profile` lo rechaza.
            let nombre_fichero = ruta
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let texto = match std::fs::read_to_string(&ruta) {
                Ok(texto) => texto,
                Err(error) => {
                    problems.push(LoadProblem::Unreadable {
                        path: ruta,
                        error: error.to_string(),
                    });
                    continue;
                }
            };
            match parse_profile(&nombre_fichero, &texto) {
                Err(problem) => problems.push(LoadProblem::Profile {
                    path: ruta,
                    problem,
                }),
                Ok((perfil, avisos)) => {
                    for aviso in avisos {
                        problems.push(LoadProblem::Profile {
                            path: ruta.clone(),
                            problem: aviso,
                        });
                    }
                    let origin = AgentOrigin::File {
                        path: ruta,
                        level: self.level,
                    };
                    entries.push(entrada_desde_perfil(
                        perfil,
                        origin,
                        Some(content_hash(&texto)),
                    ));
                }
            }
        }

        SourceLoad { entries, problems }
    }
}

/// Entrada del registro a partir de un perfil validado. La tarjeta se sintetiza solo con
/// `name`, `description` y `version`; persona, modelo y esfuerzo no viajan en ella.
fn entrada_desde_perfil(
    perfil: AgentProfile,
    origin: AgentOrigin,
    content_hash: Option<String>,
) -> AgentEntry {
    let version = perfil
        .version
        .unwrap_or_else(|| VERSION_POR_DEFECTO.to_string());
    AgentEntry {
        name: perfil.name.clone(),
        origin,
        card: AgentCard::local(perfil.name, perfil.description, version),
        execution: Execution::Local(LocalSpec {
            body: perfil.body,
            persona_role: perfil.persona_role,
            persona_goal: perfil.persona_goal,
            persona_backstory: perfil.persona_backstory,
            model: perfil.model,
            effort: perfil.effort,
            max_turns: perfil.max_turns,
            max_depth: perfil.max_depth,
        }),
        content_hash,
    }
}

/// Versión de la tarjeta cuando el perfil no declara ninguna.
const VERSION_POR_DEFECTO: &str = "0.0.0";

/// Descripción de la tarjeta del agente integrado.
const DESCRIPCION_POR_DEFECTO: &str =
    "Agente genérico integrado; hereda modelo y esfuerzo del agente que delega";

/// Fuente integrada: aporta el agente `default`, siempre presente.
pub struct BuiltInSource;

impl AgentSource for BuiltInSource {
    fn id(&self) -> &str {
        "built-in"
    }

    fn load(&self) -> SourceLoad {
        // Cuerpo vacío y sin modelo ni esfuerzo: la sesión hija hereda todo del padre.
        let entrada = AgentEntry {
            name: AGENTE_POR_DEFECTO.to_string(),
            origin: AgentOrigin::BuiltIn,
            card: AgentCard::local(
                AGENTE_POR_DEFECTO,
                DESCRIPCION_POR_DEFECTO,
                VERSION_POR_DEFECTO,
            ),
            execution: Execution::Local(LocalSpec {
                body: String::new(),
                persona_role: None,
                persona_goal: None,
                persona_backstory: None,
                model: None,
                effort: None,
                max_turns: None,
                max_depth: None,
            }),
            content_hash: None,
        };
        SourceLoad {
            entries: vec![entrada],
            problems: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn escribir(dir: &Path, nombre: &str, texto: &str) {
        std::fs::write(dir.join(nombre), texto).unwrap();
    }

    fn perfil(nombre: &str, descripcion: &str) -> String {
        format!("---\nname: {nombre}\ndescription: {descripcion}\n---\n\nCuerpo de {nombre}.\n")
    }

    #[test]
    fn file_source_loads_md_files_only() {
        let dir = tempfile::tempdir().unwrap();
        escribir(dir.path(), "frontend.md", &perfil("frontend", "UI"));
        escribir(dir.path(), "notas.txt", &perfil("notas", "No es un agente"));
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        escribir(
            &dir.path().join("sub"),
            "oculto.md",
            &perfil("oculto", "Subcarpeta"),
        );

        let carga = FileSource::new(dir.path().to_path_buf(), ProfileLevel::Global).load();

        let nombres: Vec<&str> = carga.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(nombres, vec!["frontend"]);
        assert!(carga.problems.is_empty());
    }

    #[test]
    fn missing_directory_yields_nothing_and_no_problem() {
        let dir = tempfile::tempdir().unwrap();
        let inexistente = dir.path().join("no-existe");

        let carga = FileSource::new(inexistente, ProfileLevel::Project).load();

        assert!(carga.entries.is_empty());
        assert!(carga.problems.is_empty());
    }

    /// `LocalSpec` sin persona ni modelo, solo con cuerpo.
    fn spec_con_cuerpo(cuerpo: &str) -> LocalSpec {
        LocalSpec {
            body: cuerpo.to_string(),
            persona_role: None,
            persona_goal: None,
            persona_backstory: None,
            model: None,
            effort: None,
            max_turns: None,
            max_depth: None,
        }
    }

    #[test]
    fn system_prompt_without_persona_is_the_body() {
        let spec = spec_con_cuerpo("Eres el agente.");
        assert_eq!(spec.system_prompt(), "Eres el agente.");
    }

    #[test]
    fn system_prompt_prepends_the_persona_lines_present() {
        let spec = LocalSpec {
            persona_role: Some("Ingeniero de frontend".to_string()),
            persona_goal: Some("Implementar y revisar la UI".to_string()),
            persona_backstory: Some("Prefiere componentes pequeños".to_string()),
            ..spec_con_cuerpo("Eres el agente de frontend.")
        };
        assert_eq!(
            spec.system_prompt(),
            "Rol: Ingeniero de frontend\nObjetivo: Implementar y revisar la UI\nContexto: Prefiere componentes pequeños\n\nEres el agente de frontend."
        );
    }

    #[test]
    fn system_prompt_skips_absent_persona_fields() {
        let spec = LocalSpec {
            persona_goal: Some("Revisar la UI".to_string()),
            ..spec_con_cuerpo("Cuerpo.")
        };
        assert_eq!(spec.system_prompt(), "Objetivo: Revisar la UI\n\nCuerpo.");
    }

    #[test]
    fn system_prompt_is_empty_for_the_builtin_default() {
        let carga = BuiltInSource.load();
        let entrada = carga
            .entries
            .iter()
            .find(|e| e.name == AGENTE_POR_DEFECTO)
            .expect("el integrado aporta `default`");
        match &entrada.execution {
            Execution::Local(spec) => assert_eq!(spec.system_prompt(), ""),
            Execution::Remote => panic!("el integrado es local"),
        }
    }

    #[test]
    fn persona_is_carried_from_the_profile_into_the_entry() {
        let dir = tempfile::tempdir().unwrap();
        escribir(
            dir.path(),
            "frontend.md",
            "---\nname: frontend\ndescription: UI\npersona:\n  role: Ingeniero de frontend\n  goal: Revisar la UI\n  backstory: Componentes pequeños\n---\n\nCuerpo.\n",
        );

        let carga = FileSource::new(dir.path().to_path_buf(), ProfileLevel::Global).load();

        match &carga.entries[0].execution {
            Execution::Local(spec) => {
                assert_eq!(spec.persona_role.as_deref(), Some("Ingeniero de frontend"));
                assert_eq!(spec.persona_goal.as_deref(), Some("Revisar la UI"));
                assert_eq!(
                    spec.persona_backstory.as_deref(),
                    Some("Componentes pequeños")
                );
            }
            Execution::Remote => panic!("un perfil de fichero es local"),
        }
    }
}
