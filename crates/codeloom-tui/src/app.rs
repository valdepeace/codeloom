use crate::permission_flow::{dialog_for, PermissionReply};
use codeloom_core::event::LiveEvent;
use codeloom_core::permission::{Decision, PermissionRequest};
use std::collections::VecDeque;

#[derive(Default, Clone)]
pub struct TranscriptItem {
    pub role: &'static str,
    pub text: String,
}

/// Petición de permiso en pantalla, con su canal de respuesta.
pub struct PendingPermission {
    pub dialog: batten::state::PermissionDialog,
    reply: PermissionReply,
}

pub struct App {
    pub items: Vec<TranscriptItem>,
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
            items: Vec::new(),
            composer: batten::state::Composer::new(),
            history: batten::state::History::new(),
            status: "ready".into(),
            should_quit: false,
            cwd: String::new(),
            model: String::new(),
            theme: batten::widgets::Theme::default_dark(),
            changes: batten::state::FileChanges::new(),
            viewport: batten::state::Viewport::new(),
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

    pub fn apply(&mut self, ev: LiveEvent) {
        match ev {
            LiveEvent::TurnStarted => self.status = "working".into(),
            LiveEvent::TurnEnded => self.status = "ready".into(),
            LiveEvent::Delta { text } => {
                if let Some(last) = self.items.last_mut() {
                    if last.role == "assistant" {
                        last.text.push_str(&text);
                        return;
                    }
                }
                self.items.push(TranscriptItem {
                    role: "assistant",
                    text,
                });
            }
            LiveEvent::MessageAdded(m) => {
                if m.role == codeloom_core::message::Role::User {
                    self.items.push(TranscriptItem {
                        role: "you",
                        text: m.text(),
                    });
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

    /// Transcript de batten a partir de los items (una línea por línea de texto).
    pub fn transcript(&self) -> batten::state::Transcript {
        use batten::state::{Line, LineKind, Transcript};
        let mut t = Transcript::new();
        for item in &self.items {
            let kind = if item.role == "you" {
                LineKind::User
            } else {
                LineKind::Assistant
            };
            for l in item.text.lines() {
                t.push(Line::new(kind, l));
            }
        }
        t
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
        assert_eq!(app.items.len(), 1);
        assert_eq!(app.items[0].text, "ab");
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
        assert!(app.items.iter().all(|i| i.role != "you"));
        app.apply(codeloom_core::event::LiveEvent::MessageAdded(
            codeloom_core::message::Message::user("hi"),
        ));
        assert_eq!(app.items.iter().filter(|i| i.role == "you").count(), 1);
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
