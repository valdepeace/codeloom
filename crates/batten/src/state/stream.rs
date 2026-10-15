use super::{Line, LineKind, Transcript};

/// Buffer de deltas de una respuesta en curso. Acumula texto y lo "cierra"
/// en una `Line` del transcript cuando se hace `flush`.
#[derive(Debug, Default)]
pub struct Stream {
    buf: String,
}

impl Stream {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn push_text_delta(&mut self, s: &str) {
        self.buf.push_str(s);
    }
    pub fn push_reasoning_delta(&mut self, s: &str) {
        self.buf.push_str(s);
    }
    pub fn pending(&self) -> &str {
        &self.buf
    }
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
    pub fn flush(&mut self, out: &mut Transcript) {
        self.flush_as(out, LineKind::Assistant);
    }
    pub fn flush_as(&mut self, out: &mut Transcript, kind: LineKind) {
        if self.buf.is_empty() {
            return;
        }
        let text = std::mem::take(&mut self.buf);
        out.push(Line::new(kind, text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{LineKind, Transcript};

    #[test]
    fn accumulates_deltas_into_one_line() {
        let mut s = Stream::new();
        let mut t = Transcript::new();
        s.push_text_delta("hel");
        s.push_text_delta("lo");
        assert_eq!(s.pending(), "hello");
        assert!(t.is_empty());
        s.flush(&mut t);
        assert_eq!(t.len(), 1);
        assert_eq!(t.line(0).unwrap().plain(), "hello");
        assert_eq!(t.line(0).unwrap().kind, LineKind::Assistant);
        assert!(s.is_empty());
    }

    #[test]
    fn flush_kind_override() {
        let mut s = Stream::new();
        let mut t = Transcript::new();
        s.push_reasoning_delta("hmm");
        s.flush_as(&mut t, LineKind::Thinking);
        assert_eq!(t.line(0).unwrap().kind, LineKind::Thinking);
    }

    #[test]
    fn empty_flush_is_a_noop() {
        let mut s = Stream::new();
        let mut t = Transcript::new();
        s.flush(&mut t);
        assert!(t.is_empty());
    }

    #[test]
    fn does_not_cut_a_multibyte_char_across_deltas() {
        let full = "án 星 🚀";
        let mut s = Stream::new();
        s.push_text_delta("án ");
        s.push_text_delta("星 ");
        s.push_text_delta("🚀");
        let mut t = Transcript::new();
        s.flush(&mut t);
        assert_eq!(t.line(0).unwrap().plain(), full);
    }
}
