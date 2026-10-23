//! Estado del showcase: catálogo, selección y foco.

use crate::demo::Demo;
use crate::demos::{self, CatalogItem};
use crate::matrix::{self, Cell, Matrix};
use batten::widgets::{Anim, ThemePreset};
use ratatui::crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Index,
    Demo,
    Matrix,
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
    /// Matriz de componentes por tema, derivada una vez del catálogo.
    matrix: Matrix,
    /// Celda bajo el cursor de la matriz: (fila, columna), siempre dentro de los límites.
    matrix_cursor: (usize, usize),
    /// `true` si el demo activo se abrió desde la matriz; `Esc` vuelve entonces a ella.
    matrix_return: bool,
}

impl Showcase {
    pub fn new() -> Self {
        let demos = demos::catalog();
        let matrix = matrix::build(&demos);
        Self {
            demos,
            selected: 0,
            focus: Focus::Index,
            quit: false,
            theme_preset: ThemePreset::Batten,
            provider_filter: None,
            query: String::new(),
            matrix,
            matrix_cursor: (0, 0),
            matrix_return: false,
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
        self.matrix_return = false;
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
        self.matrix_return = false;
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Index => Focus::Demo,
            _ => Focus::Index,
        };
        // Cualquier salida de Focus::Demo por Tab deja el demo sin procedencia de la matriz.
        self.matrix_return = false;
    }

    pub fn set_focus(&mut self, focus: Focus) {
        self.focus = focus;
        if focus != Focus::Demo {
            self.matrix_return = false;
        }
    }

    /// Abre la matriz desde el índice.
    pub fn open_matrix(&mut self) {
        self.focus = Focus::Matrix;
    }

    /// Sale del demo con `Esc`: vuelve a la matriz si el demo se abrió desde ella, si no al índice.
    pub fn leave_demo(&mut self) {
        self.focus = if self.matrix_return {
            Focus::Matrix
        } else {
            Focus::Index
        };
        self.matrix_return = false;
    }

    /// Matriz de componentes por tema.
    // Lo leerá el dibujo de la matriz (Tarea 3); hasta entonces solo lo ejercitan los tests.
    #[allow(dead_code)]
    pub fn matrix(&self) -> &Matrix {
        &self.matrix
    }

    /// Posición (fila, columna) del cursor de la matriz.
    // Lo leerá el dibujo de la matriz (Tarea 3); hasta entonces solo lo ejercitan los tests.
    #[allow(dead_code)]
    pub fn matrix_cursor(&self) -> (usize, usize) {
        self.matrix_cursor
    }

    /// Celda bajo el cursor de la matriz, o `None` si la matriz no tiene filas.
    // Lo leerá el dibujo de la matriz (Tarea 3); hasta entonces solo lo ejercitan los tests.
    #[allow(dead_code)]
    pub fn matrix_cell(&self) -> Option<Cell> {
        let (row, column) = self.matrix_cursor;
        self.matrix
            .rows
            .get(row)
            .and_then(|row| row.cells.get(column))
            .copied()
    }

    /// Desplaza el cursor de la matriz con tope en los bordes; no hace nada sin filas.
    pub fn matrix_move(&mut self, down: isize, right: isize) {
        let rows = self.matrix.rows.len();
        if rows == 0 {
            return;
        }
        let (row, column) = self.matrix_cursor;
        self.matrix_cursor = (
            step(row, down, rows),
            step(column, right, matrix::COLUMNS.len()),
        );
    }

    /// Abre el demo de la celda bajo el cursor con el tema de su columna. Las celdas
    /// `Missing` no hacen nada; la columna CodeLoom no cambia el tema activo.
    pub fn open_matrix_cell(&mut self) {
        let (row, column) = self.matrix_cursor;
        let Some(component) = self.matrix.rows.get(row).map(|row| row.component) else {
            return;
        };
        let Some(index) = matrix::target(&self.demos, component, column) else {
            return;
        };
        self.select(index, true);
        self.matrix_return = true;
        if let Some((_, Some(preset))) = matrix::COLUMNS.get(column) {
            self.theme_preset = *preset;
        }
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
        let demos: Vec<CatalogItem> = demos
            .into_iter()
            .map(|demo| CatalogItem {
                component: "Tests",
                variant: "Test",
                provider: "Test",
                demo,
            })
            .collect();
        let matrix = matrix::build(&demos);
        Self {
            demos,
            selected: 0,
            focus: Focus::Index,
            quit: false,
            theme_preset: ThemePreset::Batten,
            provider_filter: None,
            query: String::new(),
            matrix,
            matrix_cursor: (0, 0),
            matrix_return: false,
        }
    }
}

/// Avanza una posición dentro de `0..len` sin dar la vuelta.
fn step(position: usize, delta: isize, len: usize) -> usize {
    (position as isize + delta).clamp(0, len as isize - 1) as usize
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
            KeyCode::Char('m') => {
                show.open_matrix();
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
        Focus::Matrix => match code {
            KeyCode::Up | KeyCode::Char('k') => {
                show.matrix_move(-1, 0);
                Action::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                show.matrix_move(1, 0);
                Action::None
            }
            KeyCode::Left | KeyCode::Char('h') => {
                show.matrix_move(0, -1);
                Action::None
            }
            KeyCode::Right | KeyCode::Char('l') => {
                show.matrix_move(0, 1);
                Action::None
            }
            KeyCode::Enter => {
                show.open_matrix_cell();
                Action::None
            }
            KeyCode::Esc | KeyCode::Char('m') => {
                show.set_focus(Focus::Index);
                Action::None
            }
            _ => Action::None,
        },
        Focus::Demo => match code {
            KeyCode::Esc => {
                show.leave_demo();
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

    fn press(show: &mut Showcase, code: KeyCode) {
        route(show, KeyEventKind::Press, code, KeyModifiers::NONE);
    }

    /// Índice de columna de la matriz por nombre.
    fn column_index(name: &str) -> usize {
        matrix::COLUMNS
            .iter()
            .position(|(column, _)| *column == name)
            .unwrap()
    }

    /// Coloca el cursor de la matriz en la celda (componente, columna), buscando por nombre.
    fn place(show: &mut Showcase, component: &str, column: &str) {
        let row = show
            .matrix()
            .rows
            .iter()
            .position(|row| row.component == component)
            .unwrap_or_else(|| panic!("sin fila para {component}"));
        show.matrix_cursor = (row, column_index(column));
    }

    /// Primer demo del catálogo con ese componente y esa variante (por prefijo de proveedor).
    fn first_demo(show: &Showcase, component: &str, provider_prefix: &str) -> usize {
        show.demos
            .iter()
            .position(|item| {
                item.component == component && item.variant.starts_with(provider_prefix)
            })
            .unwrap()
    }

    /// Componente de la primera fila cuya celda en esa columna cumple `pred`.
    fn component_where(
        show: &Showcase,
        column: usize,
        pred: impl Fn(Cell) -> bool,
    ) -> &'static str {
        show.matrix()
            .rows
            .iter()
            .find(|row| pred(row.cells[column]))
            .map(|row| row.component)
            .expect("ninguna fila cumple la condición en el catálogo real")
    }

    #[test]
    fn m_opens_the_matrix_and_esc_closes_it() {
        let mut show = Showcase::new();
        press(&mut show, KeyCode::Char('m'));
        assert_eq!(show.focus(), Focus::Matrix);
        press(&mut show, KeyCode::Esc);
        assert_eq!(show.focus(), Focus::Index);
        press(&mut show, KeyCode::Char('m'));
        press(&mut show, KeyCode::Char('m'));
        assert_eq!(show.focus(), Focus::Index);
    }

    #[test]
    fn m_is_a_character_in_the_text_filter() {
        let mut show = Showcase::new();
        press(&mut show, KeyCode::Char('/'));
        press(&mut show, KeyCode::Char('m'));
        assert_eq!(show.focus(), Focus::TextFilter);
        assert_eq!(show.query(), "m");
    }

    #[test]
    fn cursor_stops_at_the_edges() {
        let mut show = Showcase::new();
        press(&mut show, KeyCode::Char('m'));
        for _ in 0..100 {
            press(&mut show, KeyCode::Up);
            press(&mut show, KeyCode::Left);
        }
        assert_eq!(show.matrix_cursor(), (0, 0));
        for _ in 0..1000 {
            press(&mut show, KeyCode::Char('j'));
            press(&mut show, KeyCode::Char('l'));
        }
        let last_row = show.matrix().rows.len() - 1;
        assert_eq!(show.matrix_cursor(), (last_row, matrix::COLUMNS.len() - 1));
        assert_eq!(show.focus(), Focus::Matrix);
    }

    #[test]
    fn enter_on_own_opens_the_demo_with_that_theme() {
        let mut show = Showcase::new();
        press(&mut show, KeyCode::Char('m'));
        place(&mut show, "Composer", "Crush");
        press(&mut show, KeyCode::Enter);
        assert_eq!(show.focus(), Focus::Demo);
        assert_eq!(show.selected(), first_demo(&show, "Composer", "Crush"));
        assert_eq!(show.theme_preset(), ThemePreset::Crush);
    }

    #[test]
    fn enter_on_generic_opens_the_batten_demo_with_the_column_theme() {
        let mut show = Showcase::new();
        press(&mut show, KeyCode::Char('m'));
        place(&mut show, "Composer", "Pawan");
        press(&mut show, KeyCode::Enter);
        assert_eq!(show.focus(), Focus::Demo);
        assert_eq!(show.selected(), first_demo(&show, "Composer", "Batten"));
        assert_eq!(show.theme_preset(), ThemePreset::Pawan);
    }

    #[test]
    fn enter_on_missing_does_nothing() {
        let mut show = Showcase::new();
        press(&mut show, KeyCode::Char('m'));
        let pawan = column_index("Pawan");
        let component = component_where(&show, pawan, |cell| cell == Cell::Missing);
        place(&mut show, component, "Pawan");
        let before = (show.selected(), show.theme_preset());
        press(&mut show, KeyCode::Enter);
        assert_eq!(show.focus(), Focus::Matrix);
        assert_eq!((show.selected(), show.theme_preset()), before);
        assert!(!show.matrix_return);
    }

    #[test]
    fn enter_on_codeloom_keeps_the_active_theme() {
        let mut show = Showcase::new();
        show.theme_preset = ThemePreset::Pawan;
        press(&mut show, KeyCode::Char('m'));
        let codeloom = column_index("CodeLoom");
        let component = component_where(&show, codeloom, |cell| matches!(cell, Cell::Own(_)));
        place(&mut show, component, "CodeLoom");
        press(&mut show, KeyCode::Enter);
        assert_eq!(show.focus(), Focus::Demo);
        assert_eq!(show.selected(), first_demo(&show, component, "CodeLoom"));
        assert_eq!(show.theme_preset(), ThemePreset::Pawan);
    }

    #[test]
    fn esc_from_a_demo_opened_from_the_matrix_returns_to_it() {
        let mut show = Showcase::new();
        press(&mut show, KeyCode::Char('m'));
        place(&mut show, "Composer", "Crush");
        press(&mut show, KeyCode::Enter);
        press(&mut show, KeyCode::Esc);
        assert_eq!(show.focus(), Focus::Matrix);
        assert!(!show.matrix_return);
    }

    #[test]
    fn esc_from_a_normal_demo_still_returns_to_the_index() {
        let mut show = Showcase::new();
        press(&mut show, KeyCode::Tab);
        assert_eq!(show.focus(), Focus::Demo);
        press(&mut show, KeyCode::Esc);
        assert_eq!(show.focus(), Focus::Index);
    }

    #[test]
    fn matrix_return_does_not_stick_after_leaving_the_demo_by_other_keys() {
        let mut show = Showcase::new();
        press(&mut show, KeyCode::Char('m'));
        place(&mut show, "Composer", "Crush");
        press(&mut show, KeyCode::Enter);
        assert!(show.matrix_return);
        // Tab saca del demo al índice: la marca se limpia.
        press(&mut show, KeyCode::Tab);
        assert_eq!(show.focus(), Focus::Index);
        assert!(!show.matrix_return);
        // Un demo abierto de forma normal no vuelve a la matriz con Esc.
        press(&mut show, KeyCode::Tab);
        press(&mut show, KeyCode::Esc);
        assert_eq!(show.focus(), Focus::Index);
    }
}
