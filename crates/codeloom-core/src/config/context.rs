use std::io::Read;
use std::path::Path;

/// Tope de cada fichero de contexto del proyecto que entra al prompt de sistema. Cabe señalar que
/// dicho prompt se envía en cada petición, de suerte que un `AGENTS.md` desmedido encarecería
/// todos los turnos; se conserva el principio, donde suelen estar las normas, y se avisa del corte.
pub const MAX_CONTEXT_FILE_BYTES: usize = 16_000;

/// Lee un fichero de texto sin traer a memoria más de `max` bytes y, si no cabe, lo corta en
/// frontera de carácter y añade un marcador con el nombre y el tope.
fn read_capped(path: &Path, name: &str, max: usize) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut buf = Vec::new();
    // Un byte de más basta para saber si el fichero excede el tope.
    file.take(max as u64 + 1).read_to_end(&mut buf).ok()?;
    let excede = buf.len() > max;
    buf.truncate(max);
    let texto = match String::from_utf8(buf) {
        Ok(texto) => texto,
        Err(error) => {
            let validos = error.utf8_error().valid_up_to();
            let mut bytes = error.into_bytes();
            if excede {
                // El corte cayó dentro de un carácter: se descarta su mitad.
                bytes.truncate(validos);
            }
            String::from_utf8_lossy(&bytes).into_owned()
        }
    };
    if excede {
        Some(format!("{texto}\n[... {name} recortado a {max} bytes ...]"))
    } else {
        Some(texto)
    }
}

/// Devuelve el contenido de los archivos de contexto del proyecto, en orden, cada uno con tope
/// de `MAX_CONTEXT_FILE_BYTES`.
pub fn load_context_files(cwd: &Path) -> Vec<String> {
    const NAMES: [&str; 2] = ["AGENTS.md", "CODELOOM.md"];
    NAMES
        .iter()
        .filter_map(|n| read_capped(&cwd.join(n), n, MAX_CONTEXT_FILE_BYTES))
        .filter(|s| !s.trim().is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_agents_md() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("AGENTS.md"), "be terse").unwrap();
        let ctx = load_context_files(dir.path());
        assert_eq!(ctx, vec!["be terse".to_string()]);
    }

    #[test]
    fn empty_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_context_files(dir.path()).is_empty());
    }

    #[test]
    fn a_file_exactly_at_the_cap_is_kept_whole() {
        let dir = tempfile::tempdir().unwrap();
        let texto = "a".repeat(MAX_CONTEXT_FILE_BYTES);
        std::fs::write(dir.path().join("AGENTS.md"), &texto).unwrap();
        assert_eq!(load_context_files(dir.path()), vec![texto]);
    }

    #[test]
    fn an_oversized_file_is_cut_with_a_notice_and_keeps_its_head() {
        let dir = tempfile::tempdir().unwrap();
        let texto = format!("NORMAS\n{}", "x".repeat(MAX_CONTEXT_FILE_BYTES * 10));
        std::fs::write(dir.path().join("AGENTS.md"), texto).unwrap();
        let ctx = load_context_files(dir.path());
        assert_eq!(ctx.len(), 1);
        assert!(ctx[0].starts_with("NORMAS\n"));
        assert!(ctx[0].contains("AGENTS.md recortado a 16000 bytes"));
        assert!(ctx[0].len() < MAX_CONTEXT_FILE_BYTES + 100);
    }

    #[test]
    fn the_cut_never_splits_a_multibyte_character() {
        let dir = tempfile::tempdir().unwrap();
        // 'ñ' ocupa dos bytes y el tope es par: se desfasa un byte para que el corte caiga en medio.
        let texto = format!("a{}", "ñ".repeat(MAX_CONTEXT_FILE_BYTES));
        std::fs::write(dir.path().join("CODELOOM.md"), texto).unwrap();
        let ctx = load_context_files(dir.path());
        assert_eq!(ctx.len(), 1);
        assert!(!ctx[0].contains('\u{FFFD}'));
        assert!(ctx[0].contains("CODELOOM.md recortado"));
    }
}
