//! Ficheros tocados en la sesión con sus líneas añadidas/quitadas. Puro.

/// Un fichero tocado: ruta y líneas añadidas/quitadas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    pub added: usize,
    pub removed: usize,
}

/// Lista de ficheros tocados, sin duplicados por `path` (orden de primera
/// aparición).
#[derive(Debug, Default, Clone)]
pub struct FileChanges {
    files: Vec<FileChange>,
}

impl FileChanges {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserta o reemplaza por `path`.
    pub fn upsert(&mut self, change: FileChange) {
        match self.files.iter_mut().find(|c| c.path == change.path) {
            Some(slot) => *slot = change,
            None => self.files.push(change),
        }
    }

    /// Quita y devuelve el último fichero.
    pub fn pop(&mut self) -> Option<FileChange> {
        self.files.pop()
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &FileChange> {
        self.files.iter()
    }

    /// Suma de (added, removed) de todos los ficheros.
    pub fn totals(&self) -> (usize, usize) {
        self.files
            .iter()
            .fold((0, 0), |(a, r), c| (a + c.added, r + c.removed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(path: &str, added: usize, removed: usize) -> FileChange {
        FileChange {
            path: path.to_string(),
            added,
            removed,
        }
    }

    #[test]
    fn upsert_appends_new_paths() {
        let mut c = FileChanges::new();
        assert!(c.is_empty());
        c.upsert(change("a.rs", 3, 1));
        c.upsert(change("b.rs", 1, 0));
        assert_eq!(c.len(), 2);
        assert_eq!(
            c.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            ["a.rs", "b.rs"]
        );
    }

    #[test]
    fn upsert_replaces_by_path_and_keeps_order() {
        let mut c = FileChanges::new();
        c.upsert(change("a.rs", 3, 1));
        c.upsert(change("b.rs", 1, 0));
        c.upsert(change("a.rs", 10, 2));
        assert_eq!(c.len(), 2);
        assert_eq!(c.iter().next().unwrap().path, "a.rs");
        assert_eq!(c.iter().next().unwrap().added, 10);
        assert_eq!(c.iter().next().unwrap().removed, 2);
    }

    #[test]
    fn totals_sum_every_file() {
        let mut c = FileChanges::new();
        c.upsert(change("a.rs", 3, 1));
        c.upsert(change("b.rs", 1, 4));
        assert_eq!(c.totals(), (4, 5));
    }

    #[test]
    fn pop_removes_the_last() {
        let mut c = FileChanges::new();
        c.upsert(change("a.rs", 1, 1));
        c.upsert(change("b.rs", 1, 1));
        assert_eq!(c.pop().unwrap().path, "b.rs");
        assert_eq!(c.len(), 1);
        assert!(c.pop().is_some());
        assert!(c.pop().is_none());
    }
}
