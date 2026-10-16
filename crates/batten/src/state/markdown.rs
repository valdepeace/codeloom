//! Markdown → líneas del transcript con `SpanSpec` y tokens del tema.
//! Puro (sin ratatui): el widget pinta los tokens como cualquier otra línea,
//! y la selección copia el texto tal como se ve.

use super::transcript::{Line, LineKind, SpanSpec, StyleToken};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

/// Convierte `src` en líneas de tipo `kind`. Markdown incompleto (streaming)
/// no pierde texto: lo que no cierra se queda como texto normal.
pub fn parse(src: &str, kind: LineKind) -> Vec<Line> {
    let opts = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS | Options::ENABLE_TABLES;
    let mut b = Builder::new(kind);
    for ev in Parser::new_ext(src, opts) {
        b.event(ev);
    }
    b.flush();
    b.out
}

struct Builder {
    kind: LineKind,
    out: Vec<Line>,
    text: String,
    spans: Vec<SpanSpec>,
    started: bool,
    blank_pending: bool,
    styles: Vec<StyleToken>,
    lists: Vec<Option<u64>>,
    item_start: bool,
    quote: usize,
    code: bool,
    links: Vec<String>,
}

impl Builder {
    fn new(kind: LineKind) -> Self {
        Self {
            kind,
            out: Vec::new(),
            text: String::new(),
            spans: Vec::new(),
            started: false,
            blank_pending: false,
            styles: Vec::new(),
            lists: Vec::new(),
            item_start: false,
            quote: 0,
            code: false,
            links: Vec::new(),
        }
    }

    /// Añade `s` a la línea en curso, con `token` si lo hay.
    fn raw(&mut self, s: &str, token: Option<StyleToken>) {
        let start = self.text.len();
        self.text.push_str(s);
        if let Some(token) = token {
            if !s.is_empty() {
                self.spans.push(SpanSpec {
                    start,
                    len: s.len(),
                    token,
                });
            }
        }
    }

    /// Abre la línea si hace falta: blanco entre bloques, barras de cita,
    /// sangría de lista y de bloque de código.
    fn start_line(&mut self) {
        if self.started {
            return;
        }
        if self.blank_pending && !self.out.is_empty() {
            self.out.push(Line::new(self.kind, ""));
        }
        self.blank_pending = false;
        self.started = true;
        for _ in 0..self.quote {
            self.raw("│ ", Some(StyleToken::Muted));
        }
        let depth = self.lists.len();
        if depth > 0 {
            let levels = depth - 1 + usize::from(!self.item_start);
            self.raw(&"  ".repeat(levels), None);
        }
        if self.code {
            self.raw("  ", None);
        }
    }

    /// Texto con el estilo en línea vigente.
    fn content(&mut self, s: &str) {
        self.start_line();
        let token = self.styles.last().copied();
        self.raw(s, token);
    }

    /// Cierra la línea en curso (si hay).
    fn flush(&mut self) {
        if !self.started {
            return;
        }
        self.out.push(Line {
            text: std::mem::take(&mut self.text),
            kind: self.kind,
            spans: std::mem::take(&mut self.spans),
        });
        self.started = false;
    }

    /// Fin de bloque: cierra la línea y, fuera de listas y citas, deja un
    /// blanco pendiente antes del siguiente bloque.
    fn block_end(&mut self) {
        self.flush();
        if self.lists.is_empty() && self.quote == 0 {
            self.blank_pending = true;
        }
    }

    fn code_text(&mut self, s: &str) {
        for seg in s.split_inclusive('\n') {
            let line = seg.strip_suffix('\n').unwrap_or(seg);
            self.start_line();
            self.raw(line, Some(StyleToken::Code));
            if seg.ends_with('\n') {
                self.flush();
            }
        }
    }

    fn event(&mut self, ev: Event) {
        match ev {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(s) if self.code => self.code_text(&s),
            Event::Text(s) | Event::Html(s) | Event::InlineHtml(s) => self.content(&s),
            Event::Code(s) => {
                self.start_line();
                self.raw(&s, Some(StyleToken::Code));
            }
            Event::SoftBreak => self.content(" "),
            Event::HardBreak => self.flush(),
            Event::Rule => {
                self.flush();
                self.start_line();
                self.raw("───", Some(StyleToken::Muted));
                self.block_end();
            }
            Event::TaskListMarker(done) => {
                self.start_line();
                self.raw(if done { "[x] " } else { "[ ] " }, Some(StyleToken::Muted));
            }
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Heading { .. } => self.styles.push(StyleToken::Heading),
            Tag::BlockQuote(_) => {
                self.flush();
                self.quote += 1;
            }
            Tag::CodeBlock(_) => {
                self.flush();
                self.code = true;
            }
            Tag::List(first) => {
                self.flush();
                self.lists.push(first);
            }
            Tag::Item => {
                self.flush();
                self.item_start = true;
                self.start_line();
                self.item_start = false;
                let marker = match self.lists.last_mut() {
                    Some(Some(n)) => {
                        let m = format!("{n}. ");
                        *n += 1;
                        m
                    }
                    _ => "• ".to_string(),
                };
                self.raw(&marker, Some(StyleToken::Muted));
            }
            Tag::Emphasis => self.styles.push(StyleToken::Emphasis),
            Tag::Strong => self.styles.push(StyleToken::Strong),
            Tag::Strikethrough => self.styles.push(StyleToken::Dim),
            Tag::Link { dest_url, .. } => {
                self.styles.push(StyleToken::Accent);
                self.links.push(dest_url.to_string());
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.block_end(),
            TagEnd::Heading(_) => {
                self.styles.pop();
                self.block_end();
            }
            TagEnd::BlockQuote(_) => {
                self.flush();
                self.quote = self.quote.saturating_sub(1);
                if self.quote == 0 && self.lists.is_empty() {
                    self.blank_pending = true;
                }
            }
            TagEnd::CodeBlock => {
                self.flush();
                self.code = false;
                self.block_end();
            }
            TagEnd::List(_) => {
                self.flush();
                self.lists.pop();
                if self.lists.is_empty() {
                    self.block_end();
                }
            }
            TagEnd::Item => self.flush(),
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                self.styles.pop();
            }
            TagEnd::Link => {
                self.styles.pop();
                // limitación: la URL va entre paréntesis; con OSC 8 sería clicable.
                if let Some(url) = self.links.pop() {
                    if !url.is_empty() {
                        self.start_line();
                        self.raw(&format!(" ({url})"), Some(StyleToken::Dim));
                    }
                }
            }
            // limitación: tablas como texto plano con separadores.
            TagEnd::TableCell => self.content(" │ "),
            TagEnd::TableHead | TagEnd::TableRow => self.flush(),
            TagEnd::Table => self.block_end(),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(lines: &[Line]) -> Vec<&str> {
        lines.iter().map(|l| l.plain()).collect()
    }

    fn md(src: &str) -> Vec<Line> {
        parse(src, LineKind::Assistant)
    }

    #[test]
    fn plain_paragraph_has_no_spans() {
        let l = md("hola mundo");
        assert_eq!(texts(&l), vec!["hola mundo"]);
        assert!(l[0].spans.is_empty());
        assert_eq!(l[0].kind, LineKind::Assistant);
    }

    #[test]
    fn heading_then_paragraph_with_blank_between() {
        let l = md("# Título\n\ntexto");
        assert_eq!(texts(&l), vec!["Título", "", "texto"]);
        assert_eq!(
            l[0].spans,
            vec![SpanSpec {
                start: 0,
                len: "Título".len(),
                token: StyleToken::Heading
            }]
        );
    }

    #[test]
    fn inline_styles_become_spans() {
        let l = md("a **b** `c` *d*");
        assert_eq!(texts(&l), vec!["a b c d"]);
        let got: Vec<(usize, usize, StyleToken)> = l[0]
            .spans
            .iter()
            .map(|s| (s.start, s.len, s.token))
            .collect();
        assert_eq!(
            got,
            vec![
                (2, 1, StyleToken::Strong),
                (4, 1, StyleToken::Code),
                (6, 1, StyleToken::Emphasis)
            ]
        );
    }

    #[test]
    fn bullet_and_numbered_lists() {
        let l = md("- uno\n- dos\n\n1. a\n2. b");
        assert_eq!(texts(&l), vec!["• uno", "• dos", "", "1. a", "2. b"]);
    }

    #[test]
    fn code_block_is_indented_and_styled() {
        let l = md("```rust\nfn x() {}\n```\n");
        assert_eq!(texts(&l), vec!["  fn x() {}"]);
        assert_eq!(l[0].spans[0].start, 2);
        assert_eq!(l[0].spans[0].token, StyleToken::Code);
    }

    #[test]
    fn quote_and_link() {
        assert_eq!(texts(&md("> cita")), vec!["│ cita"]);
        let l = md("[docs](https://x.y)");
        assert_eq!(texts(&l), vec!["docs (https://x.y)"]);
        assert_eq!(l[0].spans[0].token, StyleToken::Accent);
    }

    #[test]
    fn incomplete_markdown_keeps_the_text() {
        assert_eq!(texts(&md("texto **sin cerrar")), vec!["texto **sin cerrar"]);
        assert_eq!(texts(&md("```\ncod")), vec!["  cod"]);
    }

    #[test]
    fn no_trailing_blank_lines() {
        let l = md("uno\n\ndos\n\n");
        assert_eq!(texts(&l), vec!["uno", "", "dos"]);
    }
}
