//! Estado del showcase: catálogo, selección y foco.

use crate::demo::Demo;
use crate::demos;
use batten::widgets::Anim;
use ratatui::crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Index,
    Demo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    None,
}

pub struct Showcase {
    demos: Vec<Box<dyn Demo>>,
    selected: usize,
    focus: Focus,
    quit: bool,
}

impl Showcase {
    pub fn new() -> Self {
        Self {
            demos: demos::catalog(),
            selected: 0,
            focus: Focus::Index,
            quit: false,
        }
    }

    pub fn len(&self) -> usize {
        self.demos.len()
    }

    pub fn title_at(&self, i: usize) -> Option<&str> {
        self.demos.get(i).map(|d| d.title())
    }

    /// Pista de teclas del demo activo.
    pub fn current_hint(&self) -> &str {
        self.demos
            .get(self.selected)
            .map(|d| d.hint())
            .unwrap_or("")
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn focus(&self) -> Focus {
        self.focus
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    /// Mueve la selección una posición abajo (con tope) y vuelve el foco al índice.
    pub fn next(&mut self) {
        if self.selected + 1 < self.demos.len() {
            self.selected += 1;
        }
        self.focus = Focus::Index;
    }

    /// Mueve la selección una posición arriba (con tope) y vuelve el foco al índice.
    pub fn prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
        self.focus = Focus::Index;
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Index => Focus::Demo,
            Focus::Demo => Focus::Index,
        };
    }

    pub fn current(&mut self) -> Option<&mut Box<dyn Demo>> {
        self.demos.get_mut(self.selected)
    }

    /// Avanza la animación del demo activo.
    pub fn tick(&mut self, anim: &Anim) {
        if let Some(d) = self.current() {
            d.tick(anim);
        }
    }

    #[cfg(test)]
    pub(crate) fn focus_demo_for_test(&mut self) {
        self.focus = Focus::Demo;
    }

    #[cfg(test)]
    pub(crate) fn with_demos(demos: Vec<Box<dyn Demo>>) -> Self {
        Self {
            demos,
            selected: 0,
            focus: Focus::Index,
            quit: false,
        }
    }
}

/// Resuelve una tecla. Las globales primero; el resto, al demo si tiene el foco.
pub fn route(show: &mut Showcase, kind: KeyEventKind, code: KeyCode, mods: KeyModifiers) -> Action {
    if kind != KeyEventKind::Press {
        return Action::None;
    }
    if mods.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
        show.quit = true;
        return Action::Quit;
    }
    match show.focus {
        Focus::Index => match code {
            KeyCode::Up | KeyCode::Char('k') => {
                show.prev();
                Action::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                show.next();
                Action::None
            }
            KeyCode::Tab => {
                show.toggle_focus();
                Action::None
            }
            KeyCode::Char('q') => {
                show.quit = true;
                Action::Quit
            }
            _ => Action::None,
        },
        Focus::Demo => match code {
            KeyCode::Esc => {
                show.focus = Focus::Index;
                Action::None
            }
            KeyCode::Tab => {
                show.toggle_focus();
                Action::None
            }
            other => {
                if let Some(d) = show.current() {
                    d.handle_key(other);
                }
                Action::None
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Demo vacía para tests de navegación, independientes del catálogo real.
    struct Dummy(&'static str);

    const NAMES: [&str; 8] = ["a", "b", "c", "d", "e", "f", "g", "h"];

    impl Demo for Dummy {
        fn title(&self) -> &str {
            self.0
        }
        fn hint(&self) -> &str {
            ""
        }
        fn render(
            &mut self,
            _f: &mut ratatui::Frame,
            _a: ratatui::layout::Rect,
            _t: &batten::widgets::Theme,
        ) {
        }
    }

    fn dummy_show(n: usize) -> Showcase {
        let demos: Vec<Box<dyn Demo>> = (0..n)
            .map(|i| Box::new(Dummy(NAMES[i])) as Box<dyn Demo>)
            .collect();
        Showcase::with_demos(demos)
    }

    #[test]
    fn navigation_clamps_and_resets_focus() {
        let mut show = dummy_show(5);
        assert_eq!(show.selected(), 0);
        show.prev();
        assert_eq!(show.selected(), 0); // clamp arriba
        show.next();
        show.next();
        assert_eq!(show.selected(), 2);
        show.focus_demo_for_test();
        assert_eq!(show.focus(), Focus::Demo);
        show.next();
        assert_eq!(show.focus(), Focus::Index); // navegar resetea el foco
        for _ in 0..100 {
            show.next();
        }
        assert_eq!(show.selected(), show.len() - 1); // clamp abajo
    }

    #[test]
    fn tab_toggles_focus() {
        let mut show = Showcase::new();
        assert_eq!(show.focus(), Focus::Index);
        route(
            &mut show,
            KeyEventKind::Press,
            KeyCode::Tab,
            KeyModifiers::NONE,
        );
        assert_eq!(show.focus(), Focus::Demo);
        route(
            &mut show,
            KeyEventKind::Press,
            KeyCode::Tab,
            KeyModifiers::NONE,
        );
        assert_eq!(show.focus(), Focus::Index);
    }

    #[test]
    fn release_is_ignored() {
        let mut show = Showcase::new();
        let a = route(
            &mut show,
            KeyEventKind::Release,
            KeyCode::Char('q'),
            KeyModifiers::NONE,
        );
        assert_eq!(a, Action::None);
        assert!(!show.should_quit());
    }

    #[test]
    fn ctrl_c_quits_always() {
        let mut show = Showcase::new();
        route(
            &mut show,
            KeyEventKind::Press,
            KeyCode::Tab,
            KeyModifiers::NONE,
        ); // foco demo
        let a = route(
            &mut show,
            KeyEventKind::Press,
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        );
        assert_eq!(a, Action::Quit);
        assert!(show.should_quit());
    }

    #[test]
    fn q_quits_from_index_only() {
        let mut show = Showcase::new();
        let a = route(
            &mut show,
            KeyEventKind::Press,
            KeyCode::Char('q'),
            KeyModifiers::NONE,
        );
        assert_eq!(a, Action::Quit);
        assert!(show.should_quit());

        // Con foco en el demo, 'q' se reenvía al demo (no sale).
        let mut show = Showcase::new();
        route(
            &mut show,
            KeyEventKind::Press,
            KeyCode::Tab,
            KeyModifiers::NONE,
        );
        let a = route(
            &mut show,
            KeyEventKind::Press,
            KeyCode::Char('q'),
            KeyModifiers::NONE,
        );
        assert_eq!(a, Action::None);
        assert!(!show.should_quit());
    }

    #[test]
    fn arrows_move_selection_in_index_focus() {
        let mut show = dummy_show(3);
        route(
            &mut show,
            KeyEventKind::Press,
            KeyCode::Down,
            KeyModifiers::NONE,
        );
        assert_eq!(show.selected(), 1);
        route(
            &mut show,
            KeyEventKind::Press,
            KeyCode::Up,
            KeyModifiers::NONE,
        );
        assert_eq!(show.selected(), 0);
    }
}
