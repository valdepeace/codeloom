//! Estado del showcase: catálogo, selección y foco.

use crate::demo::Demo;
use crate::demos::{self, CatalogItem};
use batten::widgets::{Anim, ThemePreset};
use ratatui::crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Index,
    Demo,
    ProviderFilter,
    TextFilter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    None,
}

pub struct Showcase {
    demos: Vec<CatalogItem>,
    selected: usize,
    focus: Focus,
    quit: bool,
    theme_preset: ThemePreset,
    provider_filter: Option<&'static str>,
    query: String,
}

impl Showcase {
    pub fn new() -> Self {
        Self {
            demos: demos::catalog(),
            selected: 0,
            focus: Focus::Index,
            quit: false,
            theme_preset: ThemePreset::Batten,
            provider_filter: None,
            query: String::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.demos.len()
    }

    pub fn provider_filter(&self) -> Option<&str> {
        self.provider_filter
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn providers(&self) -> Vec<&'static str> {
        self.demos
            .iter()
            .map(|item| item.provider)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    pub fn filtered_indices(&self) -> Vec<usize> {
        let query = self.query.to_lowercase();
        self.demos
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                let provider_matches = self
                    .provider_filter
                    .is_none_or(|provider| item.provider == provider);
                let query_matches = query.is_empty()
                    || [item.provider, item.component, item.variant]
                        .iter()
                        .any(|value| value.to_lowercase().contains(&query));
                (provider_matches && query_matches).then_some(index)
            })
            .collect()
    }

    pub fn index_rows(&self) -> Vec<(Option<usize>, String)> {
        let mut rows = Vec::new();
        let mut last_component = None;
        for index in self.filtered_indices() {
            let component = self.demos[index].component;
            if last_component != Some(component) {
                rows.push((None, format!("{component} ─")));
                last_component = Some(component);
            }
            rows.push((Some(index), self.demos[index].variant.to_string()));
        }
        if rows.is_empty() {
            rows.push((None, "Sin resultados".to_string()));
        }
        rows
    }

    pub fn visible_index_rows(&self, height: u16) -> (usize, Vec<(Option<usize>, String)>) {
        let rows = self.index_rows();
        let selected_row = rows
            .iter()
            .position(|(index, _)| *index == Some(self.selected))
            .unwrap_or(0);
        let height = usize::from(height);
        let start = selected_row
            .saturating_sub(height / 2)
            .min(rows.len().saturating_sub(height));
        (start, rows.into_iter().skip(start).take(height).collect())
    }

    pub fn select(&mut self, index: usize, open: bool) {
        if index < self.demos.len() {
            self.selected = index;
            if open {
                self.focus = Focus::Demo;
            }
        }
    }

    pub fn select_provider(&mut self, provider: Option<&'static str>) {
        self.provider_filter = provider;
        self.keep_selection_visible();
    }

    pub fn cycle_provider(&mut self, backwards: bool) {
        let mut choices = vec![None];
        choices.extend(self.providers().into_iter().map(Some));
        let current = choices
            .iter()
            .position(|choice| *choice == self.provider_filter)
            .unwrap_or(0);
        let next = if backwards {
            (current + choices.len() - 1) % choices.len()
        } else {
            (current + 1) % choices.len()
        };
        self.select_provider(choices[next]);
    }

    fn keep_selection_visible(&mut self) {
        let filtered = self.filtered_indices();
        if !filtered.contains(&self.selected) {
            self.selected = filtered.first().copied().unwrap_or(0);
        }
    }

    pub fn push_query(&mut self, character: char) {
        self.query.push(character);
        self.keep_selection_visible();
    }

    pub fn pop_query(&mut self) {
        self.query.pop();
        self.keep_selection_visible();
    }

    pub fn clear_query(&mut self) {
        self.query.clear();
        self.keep_selection_visible();
    }

    pub fn component_at(&self, i: usize) -> Option<&str> {
        self.demos.get(i).map(|d| d.component)
    }

    pub fn variant_at(&self, i: usize) -> Option<&str> {
        self.demos.get(i).map(|d| d.variant)
    }

    /// Pista de teclas del demo activo.
    pub fn current_hint(&self) -> &str {
        self.demos
            .get(self.selected)
            .map(|d| d.demo.hint())
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

    pub fn theme_preset(&self) -> ThemePreset {
        self.theme_preset
    }

    fn cycle_theme(&mut self, backwards: bool) {
        let presets = ThemePreset::ALL;
        let current = presets
            .iter()
            .position(|preset| *preset == self.theme_preset)
            .unwrap_or(0);
        let next = if backwards {
            (current + presets.len() - 1) % presets.len()
        } else {
            (current + 1) % presets.len()
        };
        self.theme_preset = presets[next];
    }

    /// Mueve la selección una posición abajo (con tope) y vuelve el foco al índice.
    pub fn next(&mut self) {
        let filtered = self.filtered_indices();
        if let Some(position) = filtered.iter().position(|index| *index == self.selected) {
            if position + 1 < filtered.len() {
                self.selected = filtered[position + 1];
            }
        } else if let Some(first) = filtered.first() {
            self.selected = *first;
        }
        self.focus = Focus::Index;
    }

    /// Mueve la selección una posición arriba (con tope) y vuelve el foco al índice.
    pub fn prev(&mut self) {
        let filtered = self.filtered_indices();
        if let Some(position) = filtered.iter().position(|index| *index == self.selected) {
            self.selected = filtered[position.saturating_sub(1)];
        } else if let Some(first) = filtered.first() {
            self.selected = *first;
        }
        self.focus = Focus::Index;
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Index => Focus::Demo,
            _ => Focus::Index,
        };
    }

    pub fn set_focus(&mut self, focus: Focus) {
        self.focus = focus;
    }

    pub fn current(&mut self) -> Option<&mut Box<dyn Demo>> {
        self.demos
            .get_mut(self.selected)
            .map(|entry| &mut entry.demo)
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
            demos: demos
                .into_iter()
                .map(|demo| CatalogItem {
                    component: "Tests",
                    variant: "Test",
                    provider: "Test",
                    demo,
                })
                .collect(),
            selected: 0,
            focus: Focus::Index,
            quit: false,
            theme_preset: ThemePreset::Batten,
            provider_filter: None,
            query: String::new(),
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
    if code == KeyCode::F(7) {
        show.cycle_theme(mods.contains(KeyModifiers::SHIFT));
        return Action::None;
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
            KeyCode::Char('/') => {
                show.focus = Focus::TextFilter;
                Action::None
            }
            KeyCode::Char('p') => {
                show.focus = Focus::ProviderFilter;
                Action::None
            }
            KeyCode::Char('q') => {
                show.quit = true;
                Action::Quit
            }
            _ => Action::None,
        },
        Focus::ProviderFilter => match code {
            KeyCode::Left => {
                show.cycle_provider(true);
                Action::None
            }
            KeyCode::Right => {
                show.cycle_provider(false);
                Action::None
            }
            KeyCode::Tab | KeyCode::Enter => {
                show.focus = Focus::TextFilter;
                Action::None
            }
            KeyCode::Esc => {
                show.focus = Focus::Index;
                Action::None
            }
            _ => Action::None,
        },
        Focus::TextFilter => match code {
            KeyCode::Char(c) if !mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                show.push_query(c);
                Action::None
            }
            KeyCode::Backspace => {
                show.pop_query();
                Action::None
            }
            KeyCode::Enter | KeyCode::Tab => {
                show.focus = Focus::Index;
                Action::None
            }
            KeyCode::Esc => {
                show.clear_query();
                show.focus = Focus::Index;
                Action::None
            }
            KeyCode::Up | KeyCode::Down => {
                show.focus = Focus::Index;
                if code == KeyCode::Up {
                    show.prev();
                } else {
                    show.next();
                }
                Action::None
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
                    d.handle_key_event(other, mods);
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

    struct Rec(std::rc::Rc<std::cell::Cell<Option<KeyModifiers>>>);

    impl Demo for Rec {
        fn title(&self) -> &str {
            "rec"
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
        fn handle_key_event(&mut self, _code: KeyCode, mods: KeyModifiers) -> bool {
            self.0.set(Some(mods));
            true
        }
    }

    #[test]
    fn route_passes_modifiers_to_the_demo() {
        let seen = std::rc::Rc::new(std::cell::Cell::new(None));
        let mut show = Showcase::with_demos(vec![Box::new(Rec(seen.clone()))]);
        show.focus_demo_for_test();
        route(
            &mut show,
            KeyEventKind::Press,
            KeyCode::Char('j'),
            KeyModifiers::CONTROL,
        );
        assert_eq!(seen.get(), Some(KeyModifiers::CONTROL));
    }
}
