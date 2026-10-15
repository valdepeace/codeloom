use codeloom_core::event::LiveEvent;

#[derive(Default, Clone)]
pub struct TranscriptItem {
    pub role: &'static str,
    pub text: String,
}

pub struct App {
    pub items: Vec<TranscriptItem>,
    pub input: String,
    pub status: String,
    pub should_quit: bool,
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
            input: String::new(),
            status: "ready".into(),
            should_quit: false,
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
            _ => {}
        }
    }

    pub fn push_char(&mut self, c: char) {
        self.input.push(c);
    }
    pub fn backspace(&mut self) {
        self.input.pop();
    }
    pub fn submit(&mut self) -> Option<String> {
        if self.input.trim().is_empty() {
            return None;
        }
        let text = std::mem::take(&mut self.input);
        self.items.push(TranscriptItem {
            role: "you",
            text: text.clone(),
        });
        Some(text)
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
    fn submit_clears_input() {
        let mut app = App::new();
        app.push_char('h');
        app.push_char('i');
        assert_eq!(app.submit(), Some("hi".to_string()));
        assert!(app.input.is_empty());
        assert_eq!(app.submit(), None);
    }
}
