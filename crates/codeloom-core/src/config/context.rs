use std::path::Path;

/// Devuelve el contenido de los archivos de contexto del proyecto, en orden.
pub fn load_context_files(cwd: &Path) -> Vec<String> {
    const NAMES: [&str; 2] = ["AGENTS.md", "CODELOOM.md"];
    NAMES
        .iter()
        .filter_map(|n| {
            let p = cwd.join(n);
            std::fs::read_to_string(p).ok()
        })
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
}
