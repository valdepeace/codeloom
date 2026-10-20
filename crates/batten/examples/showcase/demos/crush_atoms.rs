//! Autocomplete, adjuntos y pills de Crush.

use crate::demo::Demo;
use batten::widgets::Theme;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

#[derive(Clone, Copy)]
pub enum CrushAtomKind {
    Completions,
    Attachments,
    Pills,
}

pub struct CrushAtomDemo {
    kind: CrushAtomKind,
    selected: usize,
    expanded: bool,
    deleting: bool,
}

impl CrushAtomDemo {
    pub fn new(kind: CrushAtomKind) -> Self {
        Self {
            kind,
            selected: 0,
            expanded: false,
            deleting: false,
        }
    }
}

impl Demo for CrushAtomDemo {
    fn title(&self) -> &str {
        match self.kind {
            CrushAtomKind::Completions => "Crush · completions",
            CrushAtomKind::Attachments => "Crush · attachment chips",
            CrushAtomKind::Pills => "Crush · todo / queue pills",
        }
    }

    fn hint(&self) -> &str {
        "↑↓: seleccionar · e: expandir pills · d: mostrar/quitar controles de adjunto"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let accent = theme.styles.accent;
        let muted = theme.styles.muted;
        let selected = theme.styles.selection;
        let lines = match self.kind {
            CrushAtomKind::Completions => {
                let entries = [
                    ("@", "src/model/ui.rs", "file"),
                    ("@", "src/widgets/composer.rs", "file"),
                    ("/", "model", "command"),
                    ("!", "cargo test", "shell"),
                ];
                let mut rows = vec![Line::from(Span::styled("Suggestions", accent))];
                rows.extend(entries.iter().enumerate().map(|(i, (kind, value, note))| {
                    let style = if i == self.selected % entries.len() {
                        selected.add_modifier(Modifier::BOLD)
                    } else {
                        muted
                    };
                    Line::from(vec![
                        Span::styled(format!(" {kind} "), accent),
                        Span::styled(*value, style),
                        Span::styled(format!("   {note}"), muted),
                    ])
                }));
                rows
            }
            CrushAtomKind::Attachments => {
                let remove = if self.deleting { " ×" } else { "" };
                vec![Line::from(vec![
                    Span::styled("▧ ", accent),
                    Span::styled(format!("session.rs{remove}  "), theme.styles.default),
                    Span::styled("▧ ", accent),
                    Span::styled(format!("design.md{remove}  "), theme.styles.default),
                    Span::styled("+2 more", muted),
                ])]
            }
            CrushAtomKind::Pills => {
                let mut rows = vec![Line::from(vec![
                    Span::styled("▰ TODO ", accent.add_modifier(Modifier::BOLD)),
                    Span::styled("2 active", theme.styles.warning),
                    Span::raw("     "),
                    Span::styled("▰ QUEUE ", accent.add_modifier(Modifier::BOLD)),
                    Span::styled("3 prompts", muted),
                    Span::styled("   ctrl+t open", muted),
                ])];
                if self.expanded {
                    rows.extend([
                        Line::from(Span::styled(
                            "   ◉ Add a migration test",
                            theme.styles.warning,
                        )),
                        Line::from(Span::styled("   ○ Verify older session data", muted)),
                        Line::from(Span::styled("   queued: inspect session loader", muted)),
                    ]);
                }
                rows
            }
        };
        let block = if matches!(self.kind, CrushAtomKind::Completions) {
            Block::default()
                .title(" completion popup ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme.styles.border)
        } else {
            Block::default()
        };
        frame.render_widget(Paragraph::new(lines).block(block), area);
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match (self.kind, code) {
            (CrushAtomKind::Completions, KeyCode::Up) => {
                self.selected = self.selected.saturating_sub(1);
            }
            (CrushAtomKind::Completions, KeyCode::Down) => self.selected = (self.selected + 1) % 4,
            (CrushAtomKind::Pills, KeyCode::Char('e')) => self.expanded = !self.expanded,
            (CrushAtomKind::Attachments, KeyCode::Char('d')) => self.deleting = !self.deleting,
            _ => return false,
        }
        true
    }
}
