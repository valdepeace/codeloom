//! Composer: buffer multilínea con cursor. Puro (sin ratatui).

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Default)]
pub struct Composer {
    buf: String,
    cursor: usize,
    preferred_col: Option<usize>,
}

impl Composer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Rangos de byte por línea lógica, sin incluir el salto.
    fn line_spans(&self) -> Vec<(usize, usize)> {
        let mut spans = Vec::new();
        let mut start = 0;
        for (i, _) in self.buf.match_indices('\n') {
            spans.push((start, i));
            start = i + 1;
        }
        spans.push((start, self.buf.len()));
        spans
    }

    fn snap_cursor_to_grapheme_end(&mut self) {
        if let Some((start, grapheme)) = self
            .buf
            .grapheme_indices(true)
            .find(|(start, grapheme)| *start < self.cursor && self.cursor < *start + grapheme.len())
        {
            self.cursor = start + grapheme.len();
        }
    }

    fn byte_at_column(&self, start: usize, end: usize, column: usize) -> usize {
        let mut width = 0;
        let mut byte = start;
        for (offset, grapheme) in self.buf[start..end].grapheme_indices(true) {
            let grapheme_width = UnicodeWidthStr::width(grapheme);
            let next_width = width + grapheme_width;
            if next_width > column {
                break;
            }
            width = next_width;
            byte = start + offset + grapheme.len();
            if width == column {
                break;
            }
        }
        byte
    }

    fn row_of_cursor(&self) -> usize {
        self.line_spans()
            .iter()
            .position(|(s, e)| self.cursor >= *s && self.cursor <= *e)
            .unwrap_or(0)
    }

    pub fn insert_char(&mut self, c: char) {
        if c == '\r' {
            self.newline();
            return;
        }
        self.buf.insert(self.cursor, c);
        self.cursor += c.len_utf8();
        self.snap_cursor_to_grapheme_end();
        self.preferred_col = None;
    }

    pub fn newline(&mut self) {
        self.insert_char('\n');
    }

    pub fn insert_str(&mut self, s: &str) {
        let normalized = s.replace("\r\n", "\n").replace('\r', "\n");
        self.buf.insert_str(self.cursor, &normalized);
        self.cursor += normalized.len();
        self.snap_cursor_to_grapheme_end();
        self.preferred_col = None;
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let (start, grapheme) = self
            .buf
            .grapheme_indices(true)
            .take_while(|(start, grapheme)| *start + grapheme.len() <= self.cursor)
            .last()
            .unwrap();
        let end = start + grapheme.len();
        self.buf.replace_range(start..end, "");
        self.cursor = start;
        self.preferred_col = None;
    }

    pub fn delete(&mut self) {
        if self.cursor >= self.buf.len() {
            return;
        }
        let start = self.cursor;
        let end = start + self.buf[start..].graphemes(true).next().unwrap().len();
        self.buf.replace_range(start..end, "");
        self.preferred_col = None;
    }

    pub fn move_left(&mut self) {
        self.cursor = self.buf[..self.cursor]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(start, _)| start);
        self.preferred_col = None;
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.buf.len() {
            self.cursor += self.buf[self.cursor..]
                .graphemes(true)
                .next()
                .unwrap()
                .len();
        }
        self.preferred_col = None;
    }

    pub fn move_home(&mut self) {
        let spans = self.line_spans();
        self.cursor = spans[self.row_of_cursor()].0;
        self.preferred_col = None;
    }

    pub fn move_end(&mut self) {
        let spans = self.line_spans();
        self.cursor = spans[self.row_of_cursor()].1;
        self.preferred_col = None;
    }

    pub fn move_up(&mut self) {
        let spans = self.line_spans();
        let row = self.row_of_cursor();
        if row == 0 {
            return;
        }
        let (start, _) = spans[row];
        let col = self
            .preferred_col
            .unwrap_or(UnicodeWidthStr::width(&self.buf[start..self.cursor]));
        let (s, e) = spans[row - 1];
        self.cursor = self.byte_at_column(s, e, col);
        self.preferred_col = Some(col);
    }

    pub fn move_down(&mut self) {
        let spans = self.line_spans();
        let row = self.row_of_cursor();
        if row + 1 >= spans.len() {
            return;
        }
        let (start, _) = spans[row];
        let col = self
            .preferred_col
            .unwrap_or(UnicodeWidthStr::width(&self.buf[start..self.cursor]));
        let (s, e) = spans[row + 1];
        self.cursor = self.byte_at_column(s, e, col);
        self.preferred_col = Some(col);
    }

    pub fn cursor(&self) -> usize {
        self.buf[..self.cursor].graphemes(true).count()
    }

    pub fn lines(&self) -> usize {
        self.line_spans().len()
    }

    /// (fila, columna en anchura de display) del cursor.
    pub fn cursor_row_col(&self) -> (usize, usize) {
        let spans = self.line_spans();
        let row = self.row_of_cursor();
        let (s, _) = spans[row];
        let col = UnicodeWidthStr::width(&self.buf[s..self.cursor]);
        (row, col)
    }

    pub fn set_text(&mut self, s: &str) {
        self.buf = s.replace("\r\n", "\n").replace('\r', "\n");
        self.cursor = self.buf.len();
        self.preferred_col = None;
    }

    pub fn push_char(&mut self, c: char) {
        self.insert_char(c);
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
        self.cursor = 0;
        self.preferred_col = None;
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

    #[test]
    fn inserts_in_the_middle() {
        let mut c = Composer::new();
        c.insert_str("hl");
        c.move_left();
        c.insert_char('e');
        assert_eq!(c.text(), "hel");
        assert_eq!(c.cursor(), 2);
    }

    #[test]
    fn backspace_and_delete_around_the_cursor() {
        let mut c = Composer::new();
        c.insert_str("abc");
        c.move_left();
        c.move_left(); // cursor en 1
        c.backspace();
        assert_eq!(c.text(), "bc");
        c.delete();
        assert_eq!(c.text(), "c");
    }

    #[test]
    fn home_and_end_go_to_line_bounds() {
        let mut c = Composer::new();
        c.insert_str("uno\ndos");
        c.move_home();
        assert_eq!(c.cursor(), 4);
        c.move_end();
        assert_eq!(c.cursor(), 7);
    }

    #[test]
    fn up_and_down_keep_the_column() {
        let mut c = Composer::new();
        c.insert_str("aaaa\nb");
        c.move_end(); // final de "b" (indice 6)
        c.move_up();
        assert_eq!(c.cursor_row_col(), (0, 1));
        c.move_down();
        assert_eq!(c.cursor_row_col(), (1, 1));
    }

    #[test]
    fn cursor_row_col_counts_display_width() {
        let mut c = Composer::new();
        c.insert_str("星x");
        assert_eq!(c.cursor_row_col(), (0, 3));
    }

    #[test]
    fn handles_accented_input_without_corrupting() {
        let mut c = Composer::new();
        c.insert_str("áé");
        c.move_left();
        c.insert_char('x');
        assert_eq!(c.text(), "áxé");
    }

    #[test]
    fn lines_counts_logical_lines() {
        let mut c = Composer::new();
        assert_eq!(c.lines(), 1);
        c.insert_str("a\nb\nc");
        assert_eq!(c.lines(), 3);
    }

    #[test]
    fn set_text_puts_the_cursor_at_the_end() {
        let mut c = Composer::new();
        c.insert_str("old");
        c.set_text("new\nline");
        assert_eq!(c.text(), "new\nline");
        assert_eq!(c.cursor(), 8);
    }

    #[test]
    fn cursor_and_deletion_keep_combining_and_emoji_graphemes_intact() {
        let mut c = Composer::new();
        c.insert_str("e\u{301}👩‍💻");
        assert_eq!(c.cursor(), 2);
        c.move_left();
        c.backspace();
        assert_eq!(c.text(), "👩‍💻");
        assert_eq!(c.cursor_row_col(), (0, 0));
    }

    #[test]
    fn vertical_cursor_keeps_display_column_with_wide_graphemes() {
        let mut c = Composer::new();
        c.set_text("星x\n1234");
        c.move_up();
        assert_eq!(c.cursor_row_col(), (0, 3));
    }

    #[test]
    fn normalizes_windows_line_endings_before_editing() {
        let mut c = Composer::new();
        c.insert_str("first\r\nsecond");
        assert_eq!(c.text(), "first\nsecond");
        c.set_text("first\r\nsecond");
        assert_eq!(c.text(), "first\nsecond");
        c.set_text("\r\n");
        assert_eq!(c.text(), "\n");
        c.move_end();
        c.move_up();
        c.move_down();
        c.backspace();
        assert!(c.is_empty());
    }
}
