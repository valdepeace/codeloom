//! Historial de prompts enviados. Puro.

/// Historial de prompts enviados, con navegación tipo shell.
#[derive(Debug)]
pub struct History {
    entries: Vec<String>,
    index: Option<usize>,
    draft: String,
    limit: usize,
}

impl Default for History {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            index: None,
            draft: String::new(),
            limit: 100,
        }
    }
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_limit(limit: usize) -> Self {
        Self {
            limit: limit.max(1),
            ..Self::default()
        }
    }

    /// Añade una entrada y resetea la navegación.
    pub fn push(&mut self, sent: impl Into<String>) {
        self.entries.push(sent.into());
        while self.entries.len() > self.limit {
            self.entries.remove(0);
        }
        self.reset();
    }

    pub fn is_browsing(&self) -> bool {
        self.index.is_some()
    }

    /// Hacia atrás (↑). Guarda `current` como borrador la primera vez.
    pub fn prev(&mut self, current: &str) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }
        match self.index {
            None => {
                self.draft = current.to_string();
                let i = self.entries.len() - 1;
                self.index = Some(i);
                Some(&self.entries[i])
            }
            Some(0) => Some(&self.entries[0]),
            Some(i) => {
                self.index = Some(i - 1);
                Some(&self.entries[i - 1])
            }
        }
    }

    /// Hacia delante (↓). Al pasar del final devuelve el borrador y termina.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<&str> {
        match self.index {
            None => None,
            Some(i) if i + 1 < self.entries.len() => {
                self.index = Some(i + 1);
                Some(&self.entries[i + 1])
            }
            Some(_) => {
                self.index = None;
                Some(&self.draft)
            }
        }
    }

    pub fn reset(&mut self) {
        self.index = None;
        self.draft.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_history_returns_none() {
        let mut h = History::new();
        assert!(!h.is_browsing());
        assert_eq!(h.prev("draft"), None);
        assert_eq!(h.next(), None);
    }

    #[test]
    fn prev_and_next_round_trip_with_draft() {
        let mut h = History::new();
        h.push("uno");
        h.push("dos");
        assert_eq!(h.prev("borrador"), Some("dos"));
        assert_eq!(h.prev("borrador"), Some("uno"));
        assert_eq!(h.prev("borrador"), Some("uno")); // tope arriba
        assert_eq!(h.next(), Some("dos"));
        assert_eq!(h.next(), Some("borrador")); // vuelve al borrador
        assert!(!h.is_browsing());
        assert_eq!(h.next(), None);
    }

    #[test]
    fn push_resets_browsing() {
        let mut h = History::new();
        h.push("uno");
        h.prev("x");
        assert!(h.is_browsing());
        h.push("dos");
        assert!(!h.is_browsing());
    }

    #[test]
    fn limit_drops_the_oldest() {
        let mut h = History::with_limit(2);
        h.push("uno");
        h.push("dos");
        h.push("tres");
        assert_eq!(h.len(), 2);
        assert_eq!(h.prev("x"), Some("tres"));
        assert_eq!(h.prev("x"), Some("dos"));
    }
}
