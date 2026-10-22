//! Aprobación de los agentes de proyecto.
//!
//! Un agente de nivel `Project` llega con el repositorio y es contenido de un tercero: no se
//! usa hasta que el usuario lo aprueba. La aprobación se identifica por la terna (raíz
//! canónica del proyecto, nombre del agente, huella del contenido), así que cualquier cambio
//! de un solo byte del fichero la invalida. El almacén vive en `<data_dir>/agent-trust.json`,
//! nunca en el repositorio.
//!
//! El almacén no guarda caché: cada consulta lee el fichero. Dos instancias de `TrustStore`
//! sobre el mismo directorio ven siempre lo mismo, y aprobar o revocar se refleja en la
//! siguiente consulta sin recargar los ficheros de los agentes.

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Nombre del fichero de aprobaciones dentro del directorio de datos.
pub const ARCHIVO_CONFIANZA: &str = "agent-trust.json";

/// Errores del almacén de confianza. Una lectura corrupta no es un error: cuenta como
/// «nada aprobado» (ver `TrustStore::is_approved`).
#[derive(Debug, thiserror::Error)]
pub enum TrustError {
    #[error("no se pudo crear el directorio de datos {ruta}: {fuente}")]
    Directorio {
        ruta: PathBuf,
        #[source]
        fuente: std::io::Error,
    },
    #[error("no se pudo leer el almacén de confianza {ruta}: {fuente}")]
    Lectura {
        ruta: PathBuf,
        #[source]
        fuente: std::io::Error,
    },
    #[error("no se pudo escribir el almacén de confianza {ruta}: {fuente}")]
    Escritura {
        ruta: PathBuf,
        #[source]
        fuente: std::io::Error,
    },
}

/// Errores al aprobar o revocar un agente por nombre (`AgentRegistry::approve`).
#[derive(Debug, thiserror::Error)]
pub enum ApprovalError {
    #[error("no existe el agente `{0}`")]
    Desconocido(String),
    #[error("el agente `{0}` no necesita aprobación: solo la requieren los de proyecto")]
    NoRequerida(String),
    #[error("el registro no tiene almacén de confianza configurado")]
    SinAlmacen,
    #[error(transparent)]
    Almacen(#[from] TrustError),
}

/// Estado de aprobación de un agente en una instantánea.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Approval {
    /// Agente del propio usuario (integrado o global): no necesita aprobación.
    NotRequired,
    /// Agente de proyecto aprobado para esta raíz, nombre y huella.
    Approved,
    /// Agente de proyecto pendiente: no se puede lanzar.
    Pending,
}

/// Contenido del almacén: raíz canónica -> nombre del agente -> huella aprobada.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct Contenido {
    #[serde(default)]
    aprobaciones: BTreeMap<String, BTreeMap<String, String>>,
}

/// Almacén de aprobaciones de agentes de proyecto. Es solo una ruta: no tiene estado en
/// memoria, así que puede clonarse y compartirse sin sincronizar nada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustStore {
    ruta: PathBuf,
}

impl TrustStore {
    /// Abre el almacén en `data_dir` (lo crea si no existe). `<data_dir>/agent-trust.json`.
    pub fn open(data_dir: PathBuf) -> Result<Self, TrustError> {
        std::fs::create_dir_all(&data_dir).map_err(|fuente| TrustError::Directorio {
            ruta: data_dir.clone(),
            fuente,
        })?;
        Ok(TrustStore {
            ruta: data_dir.join(ARCHIVO_CONFIANZA),
        })
    }

    /// `true` solo si la terna (raíz canónica, nombre, huella) está aprobada. Un fichero
    /// ilegible o corrupto cuenta como «nada aprobado».
    pub fn is_approved(&self, project_root: &Path, name: &str, content_hash: &str) -> bool {
        match self.leer() {
            Ok(contenido) => contenido
                .aprobaciones
                .get(&clave_de(project_root))
                .and_then(|aprobados| aprobados.get(name))
                .is_some_and(|huella| huella == content_hash),
            Err(_) => false,
        }
    }

    /// Aprueba la terna. Escritura atómica: un corte no deja el almacén a medias.
    pub fn approve(
        &self,
        project_root: &Path,
        name: &str,
        content_hash: &str,
    ) -> Result<(), TrustError> {
        let mut contenido = self.leer()?;
        contenido
            .aprobaciones
            .entry(clave_de(project_root))
            .or_default()
            .insert(name.to_string(), content_hash.to_string());
        self.escribir(&contenido)
    }

    /// Revoca la aprobación de `name` en esa raíz. Si no existía, no hace nada.
    pub fn revoke(&self, project_root: &Path, name: &str) -> Result<(), TrustError> {
        let mut contenido = self.leer()?;
        let clave = clave_de(project_root);
        let Some(aprobados) = contenido.aprobaciones.get_mut(&clave) else {
            return Ok(());
        };
        if aprobados.remove(name).is_none() {
            return Ok(());
        }
        if aprobados.is_empty() {
            contenido.aprobaciones.remove(&clave);
        }
        self.escribir(&contenido)
    }

    /// Lee el contenido. Fichero ausente o corrupto: contenido vacío (nada aprobado). Solo
    /// un error de E/S distinto de «no existe» es un `Err`.
    fn leer(&self) -> Result<Contenido, TrustError> {
        match std::fs::read_to_string(&self.ruta) {
            Ok(texto) => Ok(serde_json::from_str(&texto).unwrap_or_default()),
            Err(fuente) if fuente.kind() == io::ErrorKind::NotFound => Ok(Contenido::default()),
            Err(fuente) if fuente.kind() == io::ErrorKind::InvalidData => Ok(Contenido::default()),
            Err(fuente) => Err(TrustError::Lectura {
                ruta: self.ruta.clone(),
                fuente,
            }),
        }
    }

    /// Escribe en un temporal hermano y lo renombra encima: el almacén nunca queda a medias.
    fn escribir(&self, contenido: &Contenido) -> Result<(), TrustError> {
        let texto = serde_json::to_string_pretty(contenido)
            .map_err(|error| error_escritura(&self.ruta, io::Error::other(error)))?;
        let temporal = self.ruta.with_extension("json.tmp");

        let resultado = (|| -> io::Result<()> {
            let mut archivo = std::fs::File::create(&temporal)?;
            archivo.write_all(texto.as_bytes())?;
            archivo.sync_all()?;
            drop(archivo);
            std::fs::rename(&temporal, &self.ruta)
        })();

        resultado.map_err(|fuente| {
            // Limpieza best effort: si falla, el temporal sobrante no altera las aprobaciones.
            let _ = std::fs::remove_file(&temporal);
            error_escritura(&self.ruta, fuente)
        })
    }
}

fn error_escritura(ruta: &Path, fuente: io::Error) -> TrustError {
    TrustError::Escritura {
        ruta: ruta.to_path_buf(),
        fuente,
    }
}

/// Clave del almacén para una raíz de proyecto: su ruta canónica en texto.
fn clave_de(project_root: &Path) -> String {
    raiz_canonica(project_root).to_string_lossy().into_owned()
}

/// Huella estable del contenido de un fichero: SHA-256 en hexadecimal minúsculas.
/// Estable entre ejecuciones, versiones y plataformas; cualquier byte distinto da otra huella.
pub fn content_hash(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Configuración de confianza de un registro: dónde se aprueba y para qué proyecto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Confianza {
    almacen: TrustStore,
    raiz: PathBuf,
    confiar_todo: bool,
}

impl Confianza {
    /// Canoniza la raíz del proyecto (si no se puede canonizar, usa la ruta tal cual).
    pub(crate) fn new(almacen: TrustStore, raiz_proyecto: &Path, confiar_todo: bool) -> Self {
        Confianza {
            almacen,
            raiz: raiz_canonica(raiz_proyecto),
            confiar_todo,
        }
    }

    /// Aprueba la terna (raíz de esta confianza, nombre, huella).
    pub(crate) fn aprobar(&self, nombre: &str, huella: &str) -> Result<(), TrustError> {
        self.almacen.approve(&self.raiz, nombre, huella)
    }

    /// Revoca la aprobación de `nombre` en la raíz de esta confianza.
    pub(crate) fn revocar(&self, nombre: &str) -> Result<(), TrustError> {
        self.almacen.revoke(&self.raiz, nombre)
    }

    /// `true` si un agente de proyecto con esa huella puede usarse.
    pub(crate) fn aprueba(&self, nombre: &str, huella: &str) -> bool {
        self.confiar_todo || self.almacen.is_approved(&self.raiz, nombre, huella)
    }
}

/// Raíz canónica del proyecto. Si canonizar falla, la ruta tal cual.
pub(crate) fn raiz_canonica(raiz: &Path) -> PathBuf {
    std::fs::canonicalize(raiz).unwrap_or_else(|_| raiz.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directorio() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn approvals_survive_reopening_the_store() {
        let datos = directorio();
        let raiz = directorio();
        let primero = TrustStore::open(datos.path().to_path_buf()).unwrap();
        primero.approve(raiz.path(), "revisor", "abc123").unwrap();

        let reabierto = TrustStore::open(datos.path().to_path_buf()).unwrap();

        assert!(reabierto.is_approved(raiz.path(), "revisor", "abc123"));
    }

    #[test]
    fn corrupt_trust_file_means_nothing_is_approved() {
        let datos = directorio();
        let raiz = directorio();
        std::fs::write(datos.path().join(ARCHIVO_CONFIANZA), "{ esto no es json").unwrap();

        let almacen = TrustStore::open(datos.path().to_path_buf()).unwrap();

        assert!(!almacen.is_approved(raiz.path(), "revisor", "abc123"));
    }

    #[test]
    fn content_hash_is_stable_and_sensitive_to_one_byte() {
        // Vector de prueba conocido de SHA-256 para «abc» (FIPS 180-2).
        assert_eq!(
            content_hash("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_ne!(content_hash("abc"), content_hash("abd"));
        assert_ne!(content_hash("abc"), content_hash("abc\n"));
    }

    #[test]
    fn approve_leaves_no_temporary_file_behind() {
        let datos = directorio();
        let raiz = directorio();
        let almacen = TrustStore::open(datos.path().to_path_buf()).unwrap();

        almacen.approve(raiz.path(), "revisor", "abc123").unwrap();

        let entradas: Vec<String> = std::fs::read_dir(datos.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(entradas, vec![ARCHIVO_CONFIANZA.to_string()]);
    }
}
