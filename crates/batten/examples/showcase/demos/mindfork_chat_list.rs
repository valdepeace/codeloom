//! Lista de conversaciones de Mindfork basada en la captura dark/en.

use crate::demo::Demo;
use batten::widgets::{session_list, Theme};
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::Frame;

const SESSIONS: [(&str, usize); 21] = [
    ("Gemma 4 on a 12 GB GPU", 4),
    ("Why is prompt eval slow on long chats?", 14),
    ("Grammar-constrained JSON output", 13),
    ("Sampler settings for livelier replies", 18),
    ("Speculative decoding: draft models", 12),
    ("Embeddings: why bge-m3 for the notes", 10),
    ("Refactoring a god object in Rust", 41),
    ("  Borrow-checker review of the split", 2),
    ("A tokio deadlock, found and fixed", 33),
    ("What does the DRY penalty actually do?", 9),
    ("MCP servers worth installing", 16),
    ("Terminal fonts and box-drawing glyphs", 8),
    ("Trip notes: the Dolomites in October", 26),
    ("Shrink the system prompt, keep the voice", 11),
    ("RAG chunk size for code search", 19),
    ("Reading list: attention papers", 11),
    ("Backup dry run before the update", 7),
    ("Regex or a real parser?", 23),
    ("Mermaid diagrams in the terminal", 11),
    ("Naming things: a short rant", 27),
    ("Reading GGUF metadata in Python", 9),
];

pub struct MindforkChatListDemo {
    selected: usize,
    query: String,
    searching: bool,
}

impl MindforkChatListDemo {
    pub fn new() -> Self {
        Self {
            selected: 0,
            query: String::new(),
            searching: false,
        }
    }

    fn filtered(&self) -> Vec<session_list::SessionRow<'static>> {
        SESSIONS
            .iter()
            .filter(|(title, _)| title.to_lowercase().contains(&self.query.to_lowercase()))
            .map(|(title, messages)| session_list::SessionRow {
                title,
                messages: *messages,
                active: *title == SESSIONS[0].0,
            })
            .collect()
    }
}

impl Demo for MindforkChatListDemo {
    fn title(&self) -> &str {
        "Mindfork · chat list"
    }

    fn hint(&self) -> &str {
        "↑/↓ seleccionar · / buscar títulos · Enter abrir · Esc índice"
    }

    fn fullscreen(&self) -> bool {
        true
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        let rows = self.filtered();
        session_list::render_mindfork(frame, area, &rows, self.selected, &self.query);
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => {
                self.selected = (self.selected + 1).min(self.filtered().len().saturating_sub(1))
            }
            KeyCode::Char('/') => {
                self.searching = true;
                self.query.clear();
                self.selected = 0;
            }
            KeyCode::Esc if self.searching => {
                self.searching = false;
                self.query.clear();
                self.selected = 0;
            }
            KeyCode::Backspace if self.searching => {
                self.query.pop();
                self.selected = 0;
            }
            KeyCode::Enter if self.searching => self.searching = false,
            KeyCode::Char(ch) if self.searching => {
                self.query.push(ch);
                self.selected = 0;
            }
            _ => return false,
        }
        true
    }

    fn handle_key_event(&mut self, code: KeyCode, mods: KeyModifiers) -> bool {
        if code == KeyCode::Char('f') && mods.contains(KeyModifiers::CONTROL) {
            self.searching = true;
            self.query.clear();
            self.selected = 0;
            return true;
        }
        self.handle_key(code)
    }
}
