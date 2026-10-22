use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// Días que se conservan los volcados antes de borrarse al abrir.
pub const RETENTION_DAYS: u64 = 7;

/// Tope del espacio que ocupan los volcados; por encima se borran primero los más antiguos.
pub const MAX_DIR_BYTES: u64 = 256 * 1024 * 1024;

/// Longitud máxima del identificador de llamada que entra en el nombre del fichero.
const MAX_ID_CHARS: usize = 64;

const PREFIJO: &str = "tool-";
const EXTENSION: &str = ".txt";

/// Volcado a disco de la salida completa de una herramienta, cuando el modelo recibe solo un
/// recorte. Un fallo de E/S nunca se propaga: la salida recortada sigue llegando sin ruta.
#[derive(Clone, Debug)]
pub struct ToolSpill {
    dir: PathBuf,
}

impl ToolSpill {
    /// Prepara el directorio y limpia los volcados caducados. Ningún error llega al llamante:
    /// si el directorio no se puede crear, `save` devolverá `None`.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        let spill = Self { dir: dir.into() };
        let _ = fs::create_dir_all(&spill.dir);
        spill.prune();
        spill
    }

    /// Guarda `content` en `<dir>/tool-<call_id saneado>.txt` y devuelve la ruta, o `None` si
    /// no se pudo escribir.
    pub fn save(&self, call_id: &str, content: &str) -> Option<PathBuf> {
        let _ = fs::create_dir_all(&self.dir);
        // Un mismo identificador puede repetirse entre turnos (`call_0`): el sufijo evita que
        // un volcado nuevo pise a otro cuya ruta ya conoce el modelo.
        let ruta = self.dir.join(format!(
            "{PREFIJO}{}-{}{EXTENSION}",
            sanear(call_id),
            sufijo_unico()
        ));
        match fs::write(&ruta, content) {
            Ok(()) => Some(ruta),
            Err(error) => {
                tracing::warn!(error = %error, "no se pudo volcar la salida de la herramienta");
                None
            }
        }
    }

    /// Borra los volcados con más de `RETENTION_DAYS` días y, si el directorio supera
    /// `MAX_DIR_BYTES`, los más antiguos hasta quedar por debajo.
    pub fn prune(&self) {
        self.prune_with(SystemTime::now(), MAX_DIR_BYTES);
    }

    /// Como `prune`, con el instante de referencia y el tope explícitos (para los tests).
    fn prune_with(&self, now: SystemTime, max_bytes: u64) {
        let Ok(entradas) = fs::read_dir(&self.dir) else {
            return;
        };
        let caducidad = Duration::from_secs(RETENTION_DAYS * 24 * 3600);
        // Solo se consideran los ficheros que este módulo genera; el resto del directorio no se toca.
        let mut volcados: Vec<(PathBuf, u64, SystemTime)> = Vec::new();
        for entrada in entradas.flatten() {
            let nombre = entrada.file_name();
            let nombre = nombre.to_string_lossy();
            if !(nombre.starts_with(PREFIJO) && nombre.ends_with(EXTENSION)) {
                continue;
            }
            let Ok(metadatos) = entrada.metadata() else {
                continue;
            };
            if !metadatos.is_file() {
                continue;
            }
            let Ok(modificado) = metadatos.modified() else {
                continue;
            };
            let ruta = entrada.path();
            if now.duration_since(modificado).unwrap_or_default() > caducidad {
                let _ = fs::remove_file(&ruta);
                continue;
            }
            volcados.push((ruta, metadatos.len(), modificado));
        }

        let mut total: u64 = volcados.iter().map(|(_, bytes, _)| bytes).sum();
        if total <= max_bytes {
            return;
        }
        volcados.sort_by_key(|(_, _, modificado)| *modificado);
        for (ruta, bytes, _) in volcados {
            if total <= max_bytes {
                break;
            }
            if fs::remove_file(&ruta).is_ok() {
                total -= bytes;
            }
        }
    }
}

/// Sufijo distinto en cada llamada: instante en nanosegundos más un contador del proceso.
fn sufijo_unico() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static CONTADOR: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("{nanos}-{}", CONTADOR.fetch_add(1, Ordering::Relaxed))
}

/// Convierte el identificador de llamada en un nombre seguro: solo `[A-Za-z0-9_-]`, el resto a
/// `_`, como máximo `MAX_ID_CHARS` caracteres. Así nunca aparecen `..` ni separadores.
fn sanear(call_id: &str) -> String {
    call_id
        .chars()
        .take(MAX_ID_CHARS)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const OCHO_DIAS: Duration = Duration::from_secs(8 * 24 * 3600);

    #[test]
    fn save_writes_the_full_content_and_returns_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let spill = ToolSpill::new(dir.path());
        let ruta = spill
            .save("call_1", "contenido completo")
            .expect("debe guardar");
        assert_eq!(ruta.parent(), Some(dir.path()));
        // ADAPTADO: el nombre lleva ahora un sufijo único tras el identificador.
        let nombre = ruta.file_name().unwrap().to_str().unwrap();
        assert!(nombre.starts_with("tool-call_1-") && nombre.ends_with(".txt"));
        assert_eq!(
            std::fs::read_to_string(&ruta).unwrap(),
            "contenido completo"
        );
    }

    #[test]
    fn save_sanitises_the_call_id() {
        let dir = tempfile::tempdir().unwrap();
        let spill = ToolSpill::new(dir.path());
        let ruta = spill.save("../../x/y", "dato").expect("debe guardar");
        assert_eq!(ruta.parent(), Some(dir.path()));
        // ADAPTADO: el nombre lleva ahora un sufijo único tras el identificador saneado.
        let nombre = ruta.file_name().unwrap().to_str().unwrap();
        assert!(nombre.starts_with("tool-______x_y-") && nombre.ends_with(".txt"));

        let largo = "a".repeat(100);
        let ruta = spill.save(&largo, "dato").expect("debe guardar");
        let nombre = ruta.file_name().unwrap().to_str().unwrap();
        assert!(nombre.starts_with(&format!("tool-{}-", "a".repeat(64))));
        assert!(!nombre.contains(&"a".repeat(65)), "el id se corta a 64");
    }

    #[test]
    fn prune_removes_files_older_than_seven_days() {
        let dir = tempfile::tempdir().unwrap();
        let spill = ToolSpill::new(dir.path());
        let ruta = spill.save("vieja", "x").expect("debe guardar");
        let ajeno = dir.path().join("notas.txt");
        std::fs::write(&ajeno, "ajeno").unwrap();

        spill.prune_with(SystemTime::now(), MAX_DIR_BYTES);
        assert!(ruta.exists(), "un volcado reciente no se borra");

        spill.prune_with(SystemTime::now() + OCHO_DIAS, MAX_DIR_BYTES);
        assert!(!ruta.exists(), "un volcado de más de 7 días se borra");
        assert!(ajeno.exists(), "un fichero que no es volcado no se toca");
    }

    #[test]
    fn prune_keeps_the_directory_under_the_cap() {
        let dir = tempfile::tempdir().unwrap();
        let spill = ToolSpill::new(dir.path());
        let primera = spill.save("uno", &"a".repeat(100)).unwrap();
        std::thread::sleep(Duration::from_millis(20));
        let segunda = spill.save("dos", &"b".repeat(100)).unwrap();
        std::thread::sleep(Duration::from_millis(20));
        let tercera = spill.save("tres", &"c".repeat(100)).unwrap();

        // 300 bytes con un tope de 250: se borra solo el más antiguo.
        spill.prune_with(SystemTime::now(), 250);
        assert!(!primera.exists(), "se borra primero el más antiguo");
        assert!(segunda.exists());
        assert!(tercera.exists());
    }

    #[test]
    fn two_saves_with_the_same_call_id_do_not_overwrite_each_other() {
        let dir = tempfile::tempdir().unwrap();
        let spill = ToolSpill::new(dir.path());
        let primera = spill.save("call_0", "primero").unwrap();
        let segunda = spill.save("call_0", "segundo").unwrap();
        assert_ne!(primera, segunda);
        assert_eq!(std::fs::read_to_string(primera).unwrap(), "primero");
        assert_eq!(std::fs::read_to_string(segunda).unwrap(), "segundo");
    }
}
