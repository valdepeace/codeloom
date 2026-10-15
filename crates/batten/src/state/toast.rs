//! Avisos temporales (toasts): cola FIFO con caducidad por frames contados.

use super::tone::Tone;
use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq)]
pub struct Toast {
    pub tone: Tone,
    pub text: String,
    /// Frames que le quedan visible.
    pub ttl: u32,
}

/// Cola de avisos; solo se muestra el primero.
#[derive(Debug, Default)]
pub struct Toasts {
    queue: VecDeque<Toast>,
}

impl Toasts {
    pub fn new() -> Self {
        Self::default()
    }

    /// Encola un aviso. Con `ttl_frames == 0` no hace nada.
    pub fn push(&mut self, tone: Tone, text: impl Into<String>, ttl_frames: u32) {
        if ttl_frames == 0 {
            return;
        }
        self.queue.push_back(Toast {
            tone,
            text: text.into(),
            ttl: ttl_frames,
        });
    }

    pub fn current(&self) -> Option<&Toast> {
        self.queue.front()
    }

    /// Consume un frame del aviso visible; al caducar lo saca de la cola.
    /// Devuelve `true` si cambió lo que se ve.
    pub fn tick(&mut self) -> bool {
        let Some(t) = self.queue.front_mut() else {
            return false;
        };
        t.ttl -= 1;
        if t.ttl == 0 {
            self.queue.pop_front();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifo_with_exact_expiry() {
        let mut t = Toasts::new();
        t.push(Tone::Success, "uno", 2);
        t.push(Tone::Error, "dos", 1);
        assert_eq!(t.current().unwrap().text, "uno");
        assert!(!t.tick());
        assert!(t.tick());
        assert_eq!(t.current().unwrap().text, "dos");
        assert!(t.tick());
        assert!(t.current().is_none());
    }

    #[test]
    fn zero_ttl_is_ignored_and_empty_tick_is_noop() {
        let mut t = Toasts::new();
        t.push(Tone::Info, "nada", 0);
        assert!(t.current().is_none());
        assert!(!t.tick());
    }
}
