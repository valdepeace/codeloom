//! Planificador de frames: la entrada se pinta ya; el resto (streaming,
//! animaciones) como mucho cada `min_interval`. Sin peticiones no se pinta.

use std::time::{Duration, Instant};

/// Intervalo mínimo entre frames de fondo (~60 fps).
pub const MIN_FRAME: Duration = Duration::from_millis(16);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    /// Teclado, pegado, resize, diálogos: se pinta en cuanto se procesa.
    Input,
    /// Eventos del agente y animaciones: se agrupan.
    Background,
}

#[derive(Debug)]
pub struct FrameScheduler {
    pending: Option<Urgency>,
    last: Option<Instant>,
    min_interval: Duration,
}

impl FrameScheduler {
    pub fn new(min_interval: Duration) -> Self {
        Self {
            pending: None,
            last: None,
            min_interval,
        }
    }

    /// Pide un frame; `Input` gana a `Background`.
    pub fn request(&mut self, urgency: Urgency) {
        self.pending = match (self.pending, urgency) {
            (Some(Urgency::Input), _) | (_, Urgency::Input) => Some(Urgency::Input),
            _ => Some(Urgency::Background),
        };
    }

    pub fn pending(&self) -> bool {
        self.pending.is_some()
    }

    /// ¿Hay que pintar ya?
    pub fn due(&self, now: Instant) -> bool {
        match self.pending {
            None => false,
            Some(Urgency::Input) => true,
            Some(Urgency::Background) => self
                .last
                .is_none_or(|last| now.duration_since(last) >= self.min_interval),
        }
    }

    /// Cuándo vence un frame de fondo pendiente (para esperar sin girar).
    pub fn deadline(&self) -> Option<Instant> {
        match self.pending {
            Some(Urgency::Background) => self.last.map(|last| last + self.min_interval),
            _ => None,
        }
    }

    /// Se acaba de pintar.
    pub fn drawn(&mut self, now: Instant) {
        self.pending = None;
        self.last = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sched() -> (FrameScheduler, Instant) {
        (FrameScheduler::new(MIN_FRAME), Instant::now())
    }

    #[test]
    fn nothing_pending_is_never_due() {
        let (s, t0) = sched();
        assert!(!s.pending());
        assert!(!s.due(t0));
        assert_eq!(s.deadline(), None);
    }

    #[test]
    fn input_is_due_immediately_even_right_after_a_frame() {
        let (mut s, t0) = sched();
        s.drawn(t0);
        s.request(Urgency::Input);
        assert!(s.due(t0));
    }

    #[test]
    fn background_waits_for_the_interval() {
        let (mut s, t0) = sched();
        s.drawn(t0);
        s.request(Urgency::Background);
        assert!(!s.due(t0 + Duration::from_millis(5)));
        assert_eq!(s.deadline(), Some(t0 + MIN_FRAME));
        assert!(s.due(t0 + MIN_FRAME));
    }

    #[test]
    fn first_background_frame_is_due() {
        let (mut s, t0) = sched();
        s.request(Urgency::Background);
        assert!(s.due(t0));
    }

    #[test]
    fn input_upgrades_a_pending_background() {
        let (mut s, t0) = sched();
        s.drawn(t0);
        s.request(Urgency::Background);
        s.request(Urgency::Input);
        assert!(s.due(t0));
        assert_eq!(s.deadline(), None);
        s.request(Urgency::Background);
        assert!(s.due(t0), "Background no rebaja un Input pendiente");
    }

    #[test]
    fn drawn_clears_the_request() {
        let (mut s, t0) = sched();
        s.request(Urgency::Input);
        s.drawn(t0);
        assert!(!s.pending());
    }
}
