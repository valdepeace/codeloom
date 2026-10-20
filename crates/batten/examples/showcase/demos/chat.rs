//! Demo de la composición completa con un guion determinista: turno normal
//! (tool + respuesta por deltas) y turno con permiso.

use crate::demo::Demo;
use crate::prompt_keys::{apply, Outcome};
use batten::state::{
    Composer, EntryId, Format, History, Line, LineKind, PermissionBody, PermissionChoice,
    PermissionDialog, Sidebar, ToolItem, Transcript, Viewport,
};
use batten::widgets::{composer, permission, sidebar, tool_item, transcript, Anim, Theme};
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::Frame;
use std::collections::VecDeque;

/// Ticks entre pasos del guion (~150 ms con el reloj de 50 ms).
const TICKS_PER_STEP: u32 = 3;

const ANSWER: &str = "## Listo\n\nHe leído el fichero. Lo que hay:\n\n\
- el transcript cuenta **filas visuales**\n\
- `PgUp` y `PgDn` no mueven tu lectura\n\
- la selección copia *lo que ves*\n\n\
```rust\nfn main() {\n    println!(\"hola\");\n}\n```\n\n\
Prueba también F6 para un turno con permiso.";

const PERMISSION_ANSWER: &str = "Hecho: target/ limpio.";

#[derive(Debug, Clone, PartialEq)]
enum Step {
    ToolStart(&'static str),
    ToolDone,
    Thinking,
    AwaitPermission(&'static str),
    Delta(String),
    Flush,
}

fn answer_steps(text: &str) -> Vec<Step> {
    std::iter::once(Step::Thinking)
        .chain(
            text.split_inclusive(' ')
                .map(|w| Step::Delta(w.to_string())),
        )
        .chain([Step::Flush])
        .collect()
}

pub struct ChatDemo {
    transcript: Transcript,
    viewport: Viewport,
    layout: transcript::Layout,
    composer: Composer,
    history: History,
    sidebar: Sidebar,
    tool: Option<ToolItem>,
    answer: Option<EntryId>,
    script: VecDeque<Step>,
    dialog: Option<PermissionDialog>,
    anim: Anim,
    thinking: bool,
    paused: bool,
    wait: u32,
    focus_transcript: bool,
}

impl ChatDemo {
    pub fn new() -> Self {
        let mut sidebar = Sidebar::with_logo("batten");
        sidebar.push_section("estado", vec!["demo".into()]);
        sidebar.push_section("modelo", vec!["simulado".into()]);
        Self {
            transcript: Transcript::new(),
            viewport: Viewport::new(),
            layout: transcript::Layout::new(),
            composer: Composer::new(),
            history: History::new(),
            sidebar,
            tool: None,
            answer: None,
            script: VecDeque::new(),
            dialog: None,
            anim: Anim::new(),
            thinking: false,
            paused: false,
            wait: 0,
            focus_transcript: false,
        }
    }

    /// Turno normal: tool `view` y respuesta palabra a palabra.
    fn start_turn(&mut self, prompt: String) {
        self.transcript.push(Line::new(LineKind::User, prompt));
        self.viewport.follow_bottom();
        self.script
            .extend([Step::ToolStart("view"), Step::ToolDone]);
        self.script.extend(answer_steps(ANSWER));
    }

    /// Turno con permiso: `bash cargo clean` espera la decisión del usuario.
    fn start_permission_turn(&mut self) {
        self.transcript
            .push(Line::new(LineKind::User, "limpia el build"));
        self.viewport.follow_bottom();
        self.script.extend([
            Step::ToolStart("bash"),
            Step::AwaitPermission("cargo clean"),
            Step::ToolDone,
        ]);
        self.script.extend(answer_steps(PERMISSION_ANSWER));
    }

    /// Ejecuta el siguiente paso. No avanza mientras haya un diálogo abierto.
    fn step(&mut self) {
        if self.dialog.is_some() {
            return;
        }
        let Some(step) = self.script.pop_front() else {
            return;
        };
        match step {
            Step::ToolStart(name) => {
                let mut t = ToolItem::new("demo", name);
                t.start();
                t.set_summary("leyendo README.md");
                self.tool = Some(t);
            }
            Step::ToolDone => {
                if let Some(t) = self.tool.as_mut() {
                    t.set_result(false);
                    t.set_output(
                        "24 líneas · Rust · Ratatui\nTranscript con Markdown\nComposer multilínea",
                    );
                }
            }
            Step::Thinking => self.thinking = true,
            Step::AwaitPermission(cmd) => {
                if let Some(t) = self.tool.as_mut() {
                    t.await_permission();
                }
                self.dialog = Some(PermissionDialog::new(
                    "bash",
                    "",
                    PermissionBody::Text(cmd.into()),
                ));
            }
            Step::Delta(s) => {
                self.thinking = false;
                match self.answer {
                    Some(id) => self.transcript.append(id, &s),
                    None => {
                        let id =
                            self.transcript
                                .push_entry(LineKind::Assistant, Format::Markdown, s);
                        self.answer = Some(id);
                    }
                }
            }
            Step::Flush => {
                self.answer = None;
                self.thinking = false;
            }
        }
    }

    /// Cierra el diálogo: permitir sigue el guion; denegar cancela y lo descarta.
    fn resolve(&mut self, choice: PermissionChoice) {
        self.dialog = None;
        if choice == PermissionChoice::Deny {
            if let Some(t) = self.tool.as_mut() {
                t.cancel();
            }
            self.script.clear();
            self.transcript
                .push(Line::new(LineKind::System, "permiso denegado"));
        } else if let Some(t) = self.tool.as_mut() {
            t.resolve_awaiting();
        }
    }
}

impl Demo for ChatDemo {
    fn title(&self) -> &str {
        "chat"
    }

    fn hint(&self) -> &str {
        "Enter: turno · x: expandir salida · Ctrl+J salto · F6 permiso · F5 foco · F2 reinicia · F3 pausa · F4 paso"
    }

    fn tick(&mut self, anim: &Anim) {
        self.anim = *anim;
        if self.paused {
            return;
        }
        self.wait += 1;
        if self.wait >= TICKS_PER_STEP {
            self.wait = 0;
            self.step();
        }
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let wide = area.width >= 60;
        let (body, side) = if wide {
            let cols = Layout::horizontal([Constraint::Min(1), Constraint::Length(28)]).split(area);
            (cols[0], Some(cols[1]))
        } else {
            (area, None)
        };
        let tool_height = if self.tool.as_ref().is_some_and(|tool| tool.expanded) {
            6
        } else {
            3
        };
        let rows = Layout::vertical([
            Constraint::Length(tool_height),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(body);
        if let Some(t) = &self.tool {
            tool_item::render(f, rows[0], t, &self.anim, theme);
        }
        if self.thinking {
            batten::widgets::render_thinking(f, rows[1], "thinking…", &self.anim, theme);
        }
        transcript::render(
            f,
            rows[2],
            &self.transcript,
            &mut self.viewport,
            &mut self.layout,
            None,
            theme,
        );
        composer::render_focused(f, rows[3], &self.composer, !self.focus_transcript, theme);
        if let Some(s) = side {
            sidebar::render(f, s, &self.sidebar, theme);
        }
        if let Some(d) = &self.dialog {
            permission::render(f, area, d, theme);
        }
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        self.handle_key_event(code, KeyModifiers::NONE)
    }

    fn handle_key_event(&mut self, code: KeyCode, mods: KeyModifiers) -> bool {
        if let Some(d) = self.dialog.as_mut() {
            let choice = match code {
                KeyCode::Left => {
                    d.prev();
                    None
                }
                KeyCode::Right => {
                    d.next();
                    None
                }
                KeyCode::Up => {
                    d.scroll_by(-1);
                    None
                }
                KeyCode::Down => {
                    d.scroll_by(1);
                    None
                }
                KeyCode::Enter => Some(d.choice()),
                KeyCode::Char('a') => Some(PermissionChoice::Allow),
                KeyCode::Char('s') => Some(PermissionChoice::AllowSession),
                KeyCode::Char('d') => Some(PermissionChoice::Deny),
                _ => None,
            };
            if let Some(c) = choice {
                self.resolve(c);
            }
            return true;
        }
        if code == KeyCode::F(5) {
            self.focus_transcript = !self.focus_transcript;
            return true;
        }
        if self.focus_transcript {
            let vp = &mut self.viewport;
            match code {
                KeyCode::Up | KeyCode::Char('k') => vp.scroll_up(1),
                KeyCode::Down | KeyCode::Char('j') => vp.scroll_down(1),
                KeyCode::Char('u') => vp.half_page_up(),
                KeyCode::Char('d') => vp.half_page_down(),
                KeyCode::PageUp | KeyCode::Char('b') => vp.page_up(),
                KeyCode::PageDown | KeyCode::Char('f') | KeyCode::Char(' ') => vp.page_down(),
                KeyCode::Home | KeyCode::Char('g') => vp.to_top(),
                KeyCode::End | KeyCode::Char('G') => vp.follow_bottom(),
                _ => return false,
            }
            return true;
        }
        match code {
            KeyCode::F(6) => self.start_permission_turn(),
            KeyCode::Char('x') if self.tool.is_some() => {
                if let Some(tool) = self.tool.as_mut() {
                    tool.toggle_expanded();
                }
            }
            KeyCode::F(2) => *self = Self::new(),
            KeyCode::F(3) => self.paused = !self.paused,
            KeyCode::F(4) => self.step(),
            KeyCode::PageUp => self.viewport.scroll_up(5),
            KeyCode::PageDown => self.viewport.scroll_down(5),
            _ => match apply(&mut self.composer, &mut self.history, code, mods) {
                Outcome::Submitted(text) => self.start_turn(text),
                Outcome::Edited => {}
                Outcome::Ignored => return false,
            },
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use batten::state::{markdown, ToolStatus};

    fn ticks(d: &mut ChatDemo, n: u32) {
        let a = Anim::new();
        for _ in 0..n {
            d.tick(&a);
        }
    }

    fn type_and_send(d: &mut ChatDemo, text: &str) {
        for c in text.chars() {
            d.handle_key(KeyCode::Char(c));
        }
        d.handle_key(KeyCode::Enter);
    }

    #[test]
    fn enter_starts_a_turn_that_streams_and_flushes() {
        let mut d = ChatDemo::new();
        type_and_send(&mut d, "hi");
        assert_eq!(d.transcript.line(0).unwrap().plain(), "hi");
        ticks(&mut d, TICKS_PER_STEP * 5); // tool start, tool done, 3 deltas
        assert!(d.answer.is_some());
        assert!(d.transcript.len() >= 2);
        ticks(&mut d, TICKS_PER_STEP * 200);
        assert!(d.answer.is_none());
        assert_eq!(
            d.transcript.len(),
            1 + markdown::parse(ANSWER, LineKind::Assistant).len()
        );
        assert_eq!(d.transcript.line(1).unwrap().plain(), "Listo");
        assert_eq!(d.tool.as_ref().unwrap().status(), ToolStatus::Success);
    }

    #[test]
    fn composer_contract_in_the_chat() {
        let mut d = ChatDemo::new();
        d.handle_key(KeyCode::Char('a'));
        d.handle_key_event(KeyCode::Char('j'), KeyModifiers::CONTROL);
        d.handle_key(KeyCode::Char('b'));
        d.handle_key(KeyCode::Enter);
        assert_eq!(d.transcript.line(0).unwrap().plain(), "a\nb");
        d.handle_key(KeyCode::Up);
        assert_eq!(d.composer.text(), "a\nb");
    }

    #[test]
    fn f5_focuses_the_transcript_and_letters_scroll() {
        let mut d = ChatDemo::new();
        d.viewport.set_geometry(100, 10);
        d.handle_key(KeyCode::F(5));
        d.handle_key(KeyCode::Char('k'));
        assert_eq!(d.composer.text(), "");
        assert!(!d.viewport.is_following());
        d.handle_key(KeyCode::F(5));
        d.handle_key(KeyCode::Char('k'));
        assert_eq!(d.composer.text(), "k");
    }

    #[test]
    fn permission_turn_blocks_until_allowed() {
        let mut d = ChatDemo::new();
        d.handle_key(KeyCode::F(6));
        ticks(&mut d, TICKS_PER_STEP * 2); // tool start, await permission
        assert!(d.dialog.is_some());
        let pending = d.script.len();
        ticks(&mut d, TICKS_PER_STEP * 10);
        assert_eq!(
            d.script.len(),
            pending,
            "el guion no avanza con el diálogo abierto"
        );
        d.handle_key(KeyCode::Char('a'));
        assert!(d.dialog.is_none());
        ticks(&mut d, TICKS_PER_STEP * 20);
        assert_eq!(d.tool.as_ref().unwrap().status(), ToolStatus::Success);
        assert_eq!(d.transcript.line(1).unwrap().plain(), PERMISSION_ANSWER);
    }

    #[test]
    fn denying_cancels_and_drops_the_script() {
        let mut d = ChatDemo::new();
        d.handle_key(KeyCode::F(6));
        ticks(&mut d, TICKS_PER_STEP * 2);
        d.handle_key(KeyCode::Char('d'));
        assert_eq!(d.tool.as_ref().unwrap().status(), ToolStatus::Canceled);
        assert!(d.script.is_empty());
        let last = d.transcript.line(d.transcript.len() - 1).unwrap();
        assert_eq!(last.kind, LineKind::System);
        assert_eq!(last.plain(), "permiso denegado");
    }

    #[test]
    fn pause_stops_and_f4_steps() {
        let mut d = ChatDemo::new();
        type_and_send(&mut d, "x");
        d.handle_key(KeyCode::F(3));
        ticks(&mut d, TICKS_PER_STEP * 10);
        assert!(d.tool.is_none());
        d.handle_key(KeyCode::F(4));
        assert!(d.tool.is_some());
    }

    #[test]
    fn f2_resets_the_scenario() {
        let mut d = ChatDemo::new();
        type_and_send(&mut d, "x");
        d.handle_key(KeyCode::F(2));
        assert!(d.transcript.is_empty());
        assert!(d.script.is_empty());
    }
}
