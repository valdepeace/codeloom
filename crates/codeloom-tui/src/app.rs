use crate::permission_flow::{dialog_for, PermissionReply};
use codeloom_core::event::LiveEvent;
use codeloom_core::permission::{Decision, PermissionRequest};
use std::collections::VecDeque;

/// Dónde va el teclado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Focus {
    #[default]
    Composer,
    Transcript,
}

/// Petición de permiso en pantalla, con su canal de respuesta.
pub struct PendingPermission {
    pub dialog: batten::state::PermissionDialog,
    reply: PermissionReply,
}

pub struct App {
    /// Transcript persistente: se actualiza con los eventos, no por frame.
    pub transcript: batten::state::Transcript,
    /// Entrada de la respuesta en curso (los deltas se añaden a ella).
    assistant: Option<batten::state::EntryId>,
    pub composer: batten::state::Composer,
    pub history: batten::state::History,
    pub status: String,
    pub should_quit: bool,
    pub cwd: String,
    pub model: String,
    pub theme: batten::widgets::Theme,
    pub changes: batten::state::FileChanges,
    /// Scroll del transcript (sigue el final salvo que el usuario suba).
    pub viewport: batten::state::Viewport,
    pub layout: batten::widgets::transcript::Layout,
    pub focus: Focus,
    /// Líneas seleccionadas del transcript, si hay.
    pub selection: Option<batten::state::Selection>,
    /// Texto pendiente de enviar al portapapeles (lo consume el bucle).
    pub clipboard: Option<String>,
    /// Prompts enviados que el agente aún no ha empezado.
    pub queued: usize,
    /// Diálogo de permiso abierto, si hay.
    pub permission: Option<PendingPermission>,
    /// Peticiones esperando a que se cierre la actual.
    permission_queue: VecDeque<(PermissionRequest, PermissionReply)>,
    /// `true` cuando se acaba de abrir un diálogo y falta lanzar su efecto.
    pub fx_pending: bool,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self {
            transcript: batten::state::Transcript::new(),
            assistant: None,
            composer: batten::state::Composer::new(),
            history: batten::state::History::new(),
            status: "ready".into(),
            should_quit: false,
            cwd: String::new(),
            model: String::new(),
            theme: batten::widgets::Theme::default_dark(),
            changes: batten::state::FileChanges::new(),
            viewport: batten::state::Viewport::new(),
            layout: batten::widgets::transcript::Layout::new(),
            focus: Focus::Composer,
            selection: None,
            clipboard: None,
            queued: 0,
            permission: None,
            permission_queue: VecDeque::new(),
            fx_pending: false,
        }
    }

    pub fn with_context(cwd: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            cwd: cwd.into(),
            model: model.into(),
            ..Self::new()
        }
    }

    /// Alterna el foco entre composer y transcript.
    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Composer => Focus::Transcript,
            Focus::Transcript => Focus::Composer,
        };
    }

    /// Encola una petición; si no hay diálogo abierto, la muestra ya.
    pub fn enqueue_permission(&mut self, req: PermissionRequest, reply: PermissionReply) {
        self.permission_queue.push_back((req, reply));
        self.open_next_permission();
    }

    /// Responde al diálogo abierto y pasa al siguiente de la cola.
    pub fn resolve_permission(&mut self, decision: Decision) {
        if let Some(p) = self.permission.take() {
            let _ = p.reply.send(decision);
        }
        self.open_next_permission();
    }

    fn open_next_permission(&mut self) {
        if self.permission.is_some() {
            return;
        }
        if let Some((req, reply)) = self.permission_queue.pop_front() {
            self.permission = Some(PendingPermission {
                dialog: dialog_for(&req),
                reply,
            });
            self.fx_pending = true;
        }
    }

    /// Mensaje del usuario: entrada nueva; la siguiente respuesta empieza otra.
    pub fn push_user(&mut self, text: &str) {
        use batten::state::{Format, LineKind};
        self.transcript
            .push_entry(LineKind::User, Format::Plain, text);
        self.assistant = None;
    }

    /// Delta de la respuesta: se añade a la entrada en curso (markdown).
    pub fn push_assistant(&mut self, delta: &str) {
        use batten::state::{Format, LineKind};
        match self.assistant {
            Some(id) => self.transcript.append(id, delta),
            None => {
                let id = self
                    .transcript
                    .push_entry(LineKind::Assistant, Format::Markdown, delta);
                self.assistant = Some(id);
            }
        }
    }

    pub fn apply(&mut self, ev: LiveEvent) {
        match ev {
            LiveEvent::TurnStarted => {
                self.status = "working".into();
                self.queued = self.queued.saturating_sub(1);
            }
            LiveEvent::TurnEnded => {
                self.status = "ready".into();
                self.assistant = None;
            }
            LiveEvent::Delta { text } => self.push_assistant(&text),
            LiveEvent::MessageAdded(m) => {
                if m.role == codeloom_core::message::Role::User {
                    self.push_user(&m.text());
                }
            }
            LiveEvent::Error { message } => self.status = format!("error: {message}"),
            LiveEvent::FileDiff {
                path,
                added,
                removed,
            } => self.changes.upsert(batten::state::FileChange {
                path,
                added,
                removed,
            }),
            _ => {}
        }
    }

    /// Texto pegado (bracketed paste): entra de una vez en el composer y
    /// nunca envía. Se ignora con un diálogo abierto o sin foco en el composer.
    pub fn paste(&mut self, text: &str) {
        if self.permission.is_some() || self.focus != Focus::Composer {
            return;
        }
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        self.composer.insert_str(&text);
        self.history.reset();
    }

    pub fn push_char(&mut self, c: char) {
        self.composer.insert_char(c);
        self.history.reset();
    }
    pub fn newline(&mut self) {
        self.composer.newline();
        self.history.reset();
    }
    pub fn backspace(&mut self) {
        self.composer.backspace();
        self.history.reset();
    }
    pub fn delete(&mut self) {
        self.composer.delete();
        self.history.reset();
    }
    pub fn move_left(&mut self) {
        self.composer.move_left();
    }
    pub fn move_right(&mut self) {
        self.composer.move_right();
    }
    pub fn move_home(&mut self) {
        self.composer.move_home();
    }
    pub fn move_end(&mut self) {
        self.composer.move_end();
    }

    /// ↑: cursor si hay varias líneas; si no, historial.
    pub fn up(&mut self) {
        if !self.history.is_browsing() && self.composer.lines() > 1 {
            self.composer.move_up();
            return;
        }
        let current = self.composer.text().to_string();
        if let Some(prev) = self.history.prev(&current) {
            let prev = prev.to_string();
            self.composer.set_text(&prev);
        }
    }

    /// ↓: historial si se está navegando; si no, cursor.
    pub fn down(&mut self) {
        if !self.history.is_browsing() {
            self.composer.move_down();
            return;
        }
        if let Some(next) = self.history.next() {
            let next = next.to_string();
            self.composer.set_text(&next);
        }
    }

    pub fn submit(&mut self) -> Option<String> {
        let text = self.composer.submit()?;
        self.history.push(text.clone());
        Some(text)
    }

    /// Construye el sidebar de batten con la marca y las secciones de sesión.
    pub fn sidebar(&self) -> batten::state::Sidebar {
        let mut sb = batten::state::Sidebar::with_logo("CodeLoom");
        sb.push_section("estado", vec![self.status.clone()]);
        sb.push_section("cwd", vec![self.cwd.clone()]);
        sb.push_section("modelo", vec![self.model.clone()]);
        sb.push_section("agente", vec!["steps    —".into()]);
        sb.push_section("subagentes / jobs", vec!["(m2)".into()]);
        sb
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deltas_accumulate_into_one_item() {
        let mut app = App::new();
        app.apply(codeloom_core::event::LiveEvent::Delta { text: "a".into() });
        app.apply(codeloom_core::event::LiveEvent::Delta { text: "b".into() });
        assert_eq!(app.transcript.entry_count(), 1);
        assert_eq!(app.transcript.line(0).unwrap().plain(), "ab");
    }

    #[test]
    fn submit_clears_composer() {
        let mut app = App::new();
        app.push_char('h');
        app.push_char('i');
        assert_eq!(app.submit(), Some("hi".to_string()));
        assert!(app.composer.is_empty());
        assert_eq!(app.submit(), None);
    }

    #[test]
    fn user_line_comes_from_the_event_not_from_submit() {
        let mut app = App::new();
        app.push_char('h');
        app.push_char('i');
        assert_eq!(app.submit(), Some("hi".to_string()));
        assert!(app.transcript.is_empty());
        app.apply(codeloom_core::event::LiveEvent::MessageAdded(
            codeloom_core::message::Message::user("hi"),
        ));
        assert_eq!(
            app.transcript.line(0).unwrap().kind,
            batten::state::LineKind::User
        );
    }

    #[test]
    fn a_user_message_starts_a_new_answer_entry() {
        let mut app = App::new();
        app.push_assistant("uno");
        app.push_user("pregunta");
        app.push_assistant("dos");
        assert_eq!(app.transcript.entry_count(), 3);
        assert_eq!(app.transcript.line(2).unwrap().plain(), "dos");
    }

    #[test]
    fn with_context_stores_cwd_and_model() {
        let app = App::with_context("/tmp/proj", "mistral-nemo");
        assert_eq!(app.cwd, "/tmp/proj");
        assert_eq!(app.model, "mistral-nemo");
        assert_eq!(app.status, "ready");
    }

    #[test]
    fn sidebar_has_logo_and_session_sections() {
        let app = App::with_context("/proj", "mistral-nemo");
        let sb = app.sidebar();
        assert_eq!(sb.logo.as_ref().unwrap().name, "CodeLoom");
        let titles: Vec<&str> = sb.sections.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(
            titles,
            ["estado", "cwd", "modelo", "agente", "subagentes / jobs"]
        );
        assert_eq!(sb.sections[1].lines, vec!["/proj".to_string()]);
    }

    #[test]
    fn paste_inserts_normalized_text_without_sending() {
        let mut app = App::new();
        app.paste("a\r\nb\rc");
        assert_eq!(app.composer.text(), "a\nb\nc");
    }

    #[test]
    fn paste_is_ignored_without_composer_focus() {
        let mut app = App::new();
        app.toggle_focus();
        app.paste("x");
        assert_eq!(app.composer.text(), "");
    }

    #[test]
    fn queue_counter_drops_on_turn_start_and_never_underflows() {
        let mut app = App::new();
        app.queued = 2;
        app.apply(codeloom_core::event::LiveEvent::TurnStarted);
        assert_eq!(app.queued, 1);
        app.apply(codeloom_core::event::LiveEvent::TurnStarted);
        app.apply(codeloom_core::event::LiveEvent::TurnStarted);
        assert_eq!(app.queued, 0);
    }

    #[test]
    fn applies_file_diffs() {
        let mut app = App::new();
        app.apply(codeloom_core::event::LiveEvent::FileDiff {
            path: "src/main.rs".into(),
            added: 3,
            removed: 1,
        });
        assert_eq!(app.changes.len(), 1);
        assert_eq!(app.changes.totals(), (3, 1));
    }
}
