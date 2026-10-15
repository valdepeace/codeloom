#[derive(Debug, Default)]
pub struct Composer {
    buf: String,
}

impl Composer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn push_char(&mut self, c: char) {
        self.buf.push(c);
    }
    pub fn newline(&mut self) {
        self.buf.push('\n');
    }
    pub fn backspace(&mut self) {
        self.buf.pop();
    }
    pub fn text(&self) -> &str {
        &self.buf
    }
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
    /// Devuelve el texto y limpia el buffer. Vacío o solo-espacios → `None`
    /// (y también limpia, para no dejar basura).
    pub fn submit(&mut self) -> Option<String> {
        if self.buf.trim().is_empty() {
            self.buf.clear();
            return None;
        }
        Some(std::mem::take(&mut self.buf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_and_submits() {
        let mut c = Composer::new();
        c.push_char('h');
        c.push_char('i');
        assert_eq!(c.text(), "hi");
        assert_eq!(c.submit(), Some("hi".to_string()));
        assert!(c.is_empty());
        assert_eq!(c.submit(), None);
    }

    #[test]
    fn whitespace_only_is_not_submitted() {
        let mut c = Composer::new();
        c.push_char(' ');
        c.newline();
        assert_eq!(c.submit(), None);
        assert_eq!(c.text(), "");
    }

    #[test]
    fn backspace_removes_last_char() {
        let mut c = Composer::new();
        c.push_char('a');
        c.push_char('b');
        c.backspace();
        assert_eq!(c.text(), "a");
        c.backspace();
        c.backspace();
        assert!(c.is_empty());
    }

    #[test]
    fn newline_inserts_newline() {
        let mut c = Composer::new();
        c.push_char('a');
        c.newline();
        c.push_char('b');
        assert_eq!(c.text(), "a\nb");
    }
}
