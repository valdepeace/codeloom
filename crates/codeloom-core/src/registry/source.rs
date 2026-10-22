//! Fuentes de agentes: de dónde sale cada agente y cómo se lee.
//!
//! Una fuente produce entradas (`AgentEntry`) y problemas (`LoadProblem`) sin conocer la
//! precedencia: eso lo resuelve `AgentRegistry` (en `mod.rs`). Así añadir una fuente nueva,
//! como la remota de A8, no obliga a tocar el registro.

use std::path::PathBuf;

use crate::card::AgentCard;
use crate::registry::profile::{parse_profile, AgentProfile, ProfileProblem};

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

/// Configuración de ejecución local. No viaja en la tarjeta.
///
/// `body` es el prompt de sistema. Cadena vacía y sin modelo ni esfuerzo significa «heredar
/// del padre» (es lo que hace el `default` integrado).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalSpec {
    pub body: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub max_turns: Option<u32>,
    pub max_depth: Option<u32>,
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
                    entries.push(entrada_desde_perfil(perfil, origin));
                }
            }
        }

        SourceLoad { entries, problems }
    }
}

/// Entrada del registro a partir de un perfil validado. La tarjeta se sintetiza solo con
/// `name`, `description` y `version`; persona, modelo y esfuerzo no viajan en ella.
fn entrada_desde_perfil(perfil: AgentProfile, origin: AgentOrigin) -> AgentEntry {
    let version = perfil
        .version
        .unwrap_or_else(|| VERSION_POR_DEFECTO.to_string());
    AgentEntry {
        name: perfil.name.clone(),
        origin,
        card: AgentCard::local(perfil.name, perfil.description, version),
        execution: Execution::Local(LocalSpec {
            body: perfil.body,
            model: perfil.model,
            effort: perfil.effort,
            max_turns: perfil.max_turns,
            max_depth: perfil.max_depth,
        }),
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
                model: None,
                effort: None,
                max_turns: None,
                max_depth: None,
            }),
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
}
