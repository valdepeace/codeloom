//! Estado del showcase: tema activo, catálogo, selección y foco.

use crate::demo::Demo;
use crate::demos::{self, CatalogItem};
use crate::matrix::{self, Cell, Matrix};
use crate::theme_view::{self, ThemeEntry, ThemeExample, ThemeView};
use batten::widgets::{Anim, ThemePreset};
use ratatui::crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Index,
    Demo,
    Matrix,
    TextFilter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    None,
}

/// Fila del índice lateral. El texto no lleva el símbolo del componente: el dibujo lo pinta
/// aparte, con su estilo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexRow {
    /// Índice del catálogo del ejemplo; `None` en las cabeceras y en el aviso «Sin resultados».
    pub catalog_index: Option<usize>,
    /// Texto de la fila. Las filas hijas van ya sangradas con dos espacios.
    pub text: String,
    /// `true` si la fila es de un componente (cabecera o fila única) y lleva símbolo.
    pub marked: bool,
    /// Solo con `marked`: `true` si el símbolo es el del genérico (`◐`), `false` si es propio (`●`).
    pub generic: bool,
}

pub struct Showcase {
    demos: Vec<CatalogItem>,
    /// Índice del catálogo del demo seleccionado (el de un ejemplo, nunca el de una cabecera).
    selected: usize,
    focus: Focus,
    quit: bool,
    query: String,
    /// Matriz de componentes por tema, derivada una vez del catálogo.
    matrix: Matrix,
    /// Celda bajo el cursor de la matriz: (fila, columna), siempre dentro de los límites.
    matrix_cursor: (usize, usize),
    /// `true` si el demo activo se abrió desde la matriz; `Esc` vuelve entonces a ella.
    matrix_return: bool,
    /// Columna de `matrix::COLUMNS` del tema activo. El tema es estado propio del índice.
    theme_column: usize,
    /// Vista del tema activo: sus componentes y ejemplos. Se recalcula al cambiar de tema.
    view: ThemeView,
}

impl Showcase {
    pub fn new() -> Self {
        let demos = demos::catalog();
        let matrix = matrix::build(&demos);
        let view = theme_view::theme_view(&demos, 0);
        Self {
            demos,
            selected: 0,
            focus: Focus::Index,
            quit: false,
            query: String::new(),
            matrix,
            matrix_cursor: (0, 0),
            matrix_return: false,
            theme_column: 0,
            view,
        }
    }

    pub fn len(&self) -> usize {
        self.demos.len()
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    /// Columna de `matrix::COLUMNS` del tema activo.
    pub fn theme_column(&self) -> usize {
        self.theme_column
    }

    /// Vista del tema activo: componentes y ejemplos que el índice muestra.
    pub fn view(&self) -> &ThemeView {
        &self.view
    }

    /// Entradas de la vista que pasan el filtro de texto, con sus ejemplos visibles.
    ///
    /// Un componente entra entero si su nombre contiene el texto; si no, solo entran sus ejemplos
    /// cuya etiqueta lo contiene, y si ninguno lo hace el componente se omite. Sin texto, todo.
    fn visible(&self) -> Vec<(&ThemeEntry, Vec<&ThemeExample>)> {
        let query = self.query.to_lowercase();
        self.view
            .entries
            .iter()
            .filter_map(|entry| {
                if query.is_empty() || entry.component.to_lowercase().contains(&query) {
                    return Some((entry, entry.examples.iter().collect()));
                }
                let examples: Vec<&ThemeExample> = entry
                    .examples
                    .iter()
                    .filter(|example| example.label.to_lowercase().contains(&query))
                    .collect();
                (!examples.is_empty()).then_some((entry, examples))
            })
            .collect()
    }

    /// Índices del catálogo seleccionables, en orden de la vista. Son los ejemplos visibles: la
    /// cabecera de un componente con varios ejemplos no es seleccionable.
    pub fn filtered_indices(&self) -> Vec<usize> {
        self.visible()
            .iter()
            .flat_map(|(_, examples)| examples.iter().map(|example| example.catalog_index))
            .collect()
    }

    /// Filas del índice.
    ///
    /// Un componente con un único ejemplo visible es una sola fila seleccionable. Con varios, una
    /// cabecera sin índice (no seleccionable, con `─`) y una fila hija por ejemplo, sangrada con su
    /// etiqueta. Las cabeceras y las filas únicas van marcadas (`marked`) con el símbolo de su tipo.
    pub fn index_rows(&self) -> Vec<IndexRow> {
        let mut rows = Vec::new();
        for (entry, examples) in self.visible() {
            if let [example] = examples.as_slice() {
                rows.push(IndexRow {
                    catalog_index: Some(example.catalog_index),
                    text: entry.component.to_string(),
                    marked: true,
                    generic: entry.generic,
                });
            } else {
                rows.push(IndexRow {
                    catalog_index: None,
                    text: format!("{} ─", entry.component),
                    marked: true,
                    generic: entry.generic,
                });
                for example in examples {
                    rows.push(IndexRow {
                        catalog_index: Some(example.catalog_index),
                        text: format!("  {}", example.label),
                        marked: false,
                        generic: false,
                    });
                }
            }
        }
        if rows.is_empty() {
            rows.push(IndexRow {
                catalog_index: None,
                text: "Sin resultados".to_string(),
                marked: false,
                generic: false,
            });
        }
        rows
    }

    pub fn visible_index_rows(&self, height: u16) -> (usize, Vec<IndexRow>) {
        let rows = self.index_rows();
        let selected_row = rows
            .iter()
            .position(|row| row.catalog_index == Some(self.selected))
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

    /// Preset del tema activo; se deriva de su columna, no se guarda aparte.
    pub fn theme_preset(&self) -> ThemePreset {
        matrix::COLUMNS
            .get(self.theme_column)
            .and_then(|(_, preset)| *preset)
            .unwrap_or(ThemePreset::Batten)
    }

    /// Cambia el tema del índice por su columna en `matrix::COLUMNS`; los índices fuera de ella
    /// se ignoran. Recalcula la vista.
    ///
    /// Si el componente seleccionado existe en el tema nuevo, se selecciona su primer ejemplo
    /// visible; si no, el primero de la lista. Con la lista vacía la selección no cambia. No
    /// cambia el foco ni `matrix_return`.
    pub fn set_theme_column(&mut self, column: usize) {
        if column >= matrix::COLUMNS.len() {
            return;
        }
        let previous = self.demos.get(self.selected).map(|item| item.component);
        self.theme_column = column;
        self.view = theme_view::theme_view(&self.demos, column);
        let kept = previous.and_then(|component| self.first_visible_of(component));
        if let Some(index) = kept.or_else(|| self.filtered_indices().first().copied()) {
            self.selected = index;
        }
    }

    /// Primer ejemplo visible del componente en la vista actual, si lo hay.
    fn first_visible_of(&self, component: &str) -> Option<usize> {
        self.visible()
            .into_iter()
            .find(|(entry, _)| entry.component == component)
            .and_then(|(_, examples)| examples.first().map(|example| example.catalog_index))
    }

    /// Pasa al tema siguiente (o, con `backwards`, al anterior), con vuelta.
    pub fn cycle_theme_column(&mut self, backwards: bool) {
        let columns = matrix::COLUMNS.len();
        let next = if backwards {
            (self.theme_column + columns - 1) % columns
        } else {
            (self.theme_column + 1) % columns
        };
        self.set_theme_column(next);
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
    pub fn matrix(&self) -> &Matrix {
        &self.matrix
    }

    /// Posición (fila, columna) del cursor de la matriz.
    pub fn matrix_cursor(&self) -> (usize, usize) {
        self.matrix_cursor
    }

    /// Celda bajo el cursor de la matriz, o `None` si la matriz no tiene filas.
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

    /// Cambia al tema de la columna de la celda bajo el cursor y abre el demo de esa celda. Las
    /// celdas `Missing` no hacen nada.
    pub fn open_matrix_cell(&mut self) {
        let (row, column) = self.matrix_cursor;
        let Some(component) = self.matrix.rows.get(row).map(|row| row.component) else {
            return;
        };
        let Some(index) = matrix::target(&self.demos, component, column) else {
            return;
        };
        // Un filtro de texto activo podría ocultar el ejemplo que se abre: se limpia antes.
        self.query.clear();
        self.set_theme_column(column);
        self.select(index, true);
        self.matrix_return = true;
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

    /// Construye un showcase de prueba con un demo por entrada.
    ///
    /// Solo para tests de navegación: el proveedor «Test» no existe en la matriz, así que la vista
    /// se construye a mano con una entrada de un único ejemplo por demo (todas bajo «Tests»).
    #[cfg(test)]
    pub(crate) fn with_demos(demos: Vec<Box<dyn Demo>>) -> Self {
        let demos: Vec<CatalogItem> = demos
            .into_iter()
            .map(|demo| CatalogItem {
                component: "Tests",
                variant: "Test",
                demo,
            })
            .collect();
        let matrix = matrix::build(&demos);
        let view = ThemeView {
            entries: (0..demos.len())
                .map(|catalog_index| ThemeEntry {
                    component: "Tests",
                    generic: false,
                    examples: vec![ThemeExample {
                        catalog_index,
                        label: "Test".to_string(),
                    }],
                })
                .collect(),
            missing: 0,
        };
        Self {
            demos,
            selected: 0,
            focus: Focus::Index,
            quit: false,
            query: String::new(),
            matrix,
            matrix_cursor: (0, 0),
            matrix_return: false,
            theme_column: 0,
            view,
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
        show.cycle_theme_column(mods.contains(KeyModifiers::SHIFT));
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
            KeyCode::Left => {
                show.cycle_theme_column(true);
                Action::None
            }
            KeyCode::Right => {
                show.cycle_theme_column(false);
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

    // ADAPTADO: el preset ya no es un campo (se deriva de la columna); el tema de partida se fija
    // con `set_theme_column` en lugar de asignar `theme_preset`.
    #[test]
    fn enter_on_codeloom_applies_its_palette() {
        let mut show = Showcase::new();
        show.set_theme_column(column_index("Pawan"));
        press(&mut show, KeyCode::Char('m'));
        let codeloom = column_index("CodeLoom");
        let component = component_where(&show, codeloom, |cell| matches!(cell, Cell::Own(_)));
        place(&mut show, component, "CodeLoom");
        press(&mut show, KeyCode::Enter);
        assert_eq!(show.focus(), Focus::Demo);
        assert_eq!(show.selected(), first_demo(&show, component, "CodeLoom"));
        assert_eq!(show.theme_preset(), ThemePreset::CodeLoom);
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

    #[test]
    fn changing_theme_keeps_the_component_when_it_exists() {
        let mut show = Showcase::new();
        show.select(first_demo(&show, "Composer", "Batten"), false);
        show.set_theme_column(column_index("Crush"));
        assert_eq!(show.selected(), first_demo(&show, "Composer", "Crush"));
        assert_eq!(show.theme_preset(), ThemePreset::Crush);
        assert_eq!(show.focus(), Focus::Index);
    }

    #[test]
    fn changing_theme_selects_the_first_when_it_does_not() {
        let mut show = Showcase::new();
        // Plan card solo tiene demo de Crush: en Batten no existe.
        show.set_theme_column(column_index("Crush"));
        show.select(first_demo(&show, "Plan card", "Crush"), false);
        show.set_theme_column(column_index("Batten"));
        assert_eq!(show.selected(), show.filtered_indices()[0]);
        assert_ne!(show.component_at(show.selected()), Some("Plan card"));
        assert_eq!(show.theme_preset(), ThemePreset::Batten);
    }

    #[test]
    fn left_right_cycle_the_theme_with_wrap() {
        let mut show = Showcase::new();
        assert_eq!(show.theme_column(), 0);
        press(&mut show, KeyCode::Left); // vuelta: Batten -> CodeLoom
        assert_eq!(show.theme_column(), matrix::COLUMNS.len() - 1);
        assert_eq!(show.theme_preset(), ThemePreset::CodeLoom);
        press(&mut show, KeyCode::Right); // vuelta: CodeLoom -> Batten
        assert_eq!(show.theme_column(), 0);
        press(&mut show, KeyCode::Right);
        assert_eq!(show.theme_column(), 1);
        assert_eq!(show.focus(), Focus::Index);
    }

    #[test]
    fn left_right_do_not_change_the_theme_in_demo_focus() {
        let mut show = Showcase::new();
        show.select(first_demo(&show, "Composer", "Batten"), true);
        assert_eq!(show.focus(), Focus::Demo);
        press(&mut show, KeyCode::Left);
        press(&mut show, KeyCode::Right);
        assert_eq!(show.theme_column(), 0);
        assert_eq!(show.focus(), Focus::Demo);
    }

    #[test]
    fn f7_still_cycles_the_theme() {
        let mut show = Showcase::new();
        route(
            &mut show,
            KeyEventKind::Press,
            KeyCode::F(7),
            KeyModifiers::NONE,
        );
        assert_eq!(show.theme_column(), 1);
        route(
            &mut show,
            KeyEventKind::Press,
            KeyCode::F(7),
            KeyModifiers::SHIFT,
        );
        assert_eq!(show.theme_column(), 0);
        // Con el foco en el demo, F7 también cambia de tema.
        press(&mut show, KeyCode::Tab);
        route(
            &mut show,
            KeyEventKind::Press,
            KeyCode::F(7),
            KeyModifiers::NONE,
        );
        assert_eq!(show.theme_column(), 1);
        assert_eq!(show.focus(), Focus::Demo);
    }

    #[test]
    fn a_component_with_several_examples_has_child_rows() {
        let mut show = Showcase::new();
        show.set_theme_column(column_index("Crush"));
        let rows = show.index_rows();
        // ADAPTADO: el símbolo ya no forma parte del texto; la cabecera se identifica por `marked`.
        let header = rows
            .iter()
            .position(|row| {
                row.catalog_index.is_none() && row.text.starts_with("Assistant message")
            })
            .expect("cabecera de Assistant message");
        assert_eq!(rows[header].text, "Assistant message ─");
        assert!(rows[header].marked && !rows[header].generic);
        let children: Vec<&str> = rows[header + 1..]
            .iter()
            // Las filas hijas van sangradas y sin símbolo; las de componentes con un ejemplo no.
            .take_while(|row| row.catalog_index.is_some() && row.text.starts_with("  "))
            .map(|row| row.text.as_str())
            .collect();
        assert_eq!(children, ["  pending", "  refusal", "  summary"]);
        for row in rows[header + 1..].iter().take(children.len()) {
            assert!(!row.marked);
            let index = row.catalog_index.expect("fila hija seleccionable");
            assert_eq!(show.component_at(index), Some("Assistant message"));
        }
    }

    #[test]
    fn a_component_with_one_example_has_no_child_rows() {
        let mut show = Showcase::new();
        show.set_theme_column(column_index("Crush"));
        let rows = show.index_rows();
        let plan = first_demo(&show, "Plan card", "Crush");
        // ADAPTADO: la fila única guarda el nombre sin símbolo y con `marked` para el dibujo.
        assert!(rows
            .iter()
            .any(|row| row.catalog_index == Some(plan) && row.text == "Plan card" && row.marked));
        assert!(rows.iter().all(|row| !row.text.ends_with("Plan card ─")));
    }

    #[test]
    fn text_filter_matches_component_names_in_the_theme() {
        let mut show = Showcase::new();
        show.set_theme_column(column_index("Crush"));
        for c in "PLAN CARD".chars() {
            show.push_query(c);
        }
        assert_eq!(
            show.filtered_indices(),
            vec![first_demo(&show, "Plan card", "Crush")]
        );
        // El filtro es del tema: en Batten no hay «Plan card».
        show.set_theme_column(column_index("Batten"));
        // ADAPTADO: `index_rows` devuelve `IndexRow`, no tuplas.
        let rows = show.index_rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].catalog_index, None);
        assert_eq!(rows[0].text, "Sin resultados");
        // El filtro también casa con la etiqueta del ejemplo.
        show.clear_query();
        show.set_theme_column(column_index("Crush"));
        for c in "pending".chars() {
            show.push_query(c);
        }
        assert!(show.filtered_indices().contains(&first_demo(
            &show,
            "Assistant message",
            "Crush · pending"
        )));
    }

    #[test]
    fn matrix_enter_switches_theme_and_selects_the_example() {
        let mut show = Showcase::new();
        press(&mut show, KeyCode::Char('m'));
        place(&mut show, "Composer", "Pawan");
        press(&mut show, KeyCode::Enter);
        assert_eq!(show.focus(), Focus::Demo);
        assert_eq!(show.theme_column(), column_index("Pawan"));
        assert_eq!(show.selected(), first_demo(&show, "Composer", "Batten"));
        assert!(show.matrix_return);
        assert!(show
            .view()
            .entries
            .iter()
            .any(|entry| entry.component == "Composer" && entry.generic));
    }

    /// Un filtro de texto que ocultaría el ejemplo abierto desde la matriz se limpia: si no, el
    /// índice quedaría sin nada seleccionado.
    #[test]
    fn matrix_enter_clears_a_filter_that_would_hide_the_example() {
        let mut show = Showcase::new();
        press(&mut show, KeyCode::Char('/'));
        for c in "zzzz".chars() {
            press(&mut show, KeyCode::Char(c));
        }
        press(&mut show, KeyCode::Enter);
        assert_eq!(show.query(), "zzzz", "precondición: hay un filtro activo");
        press(&mut show, KeyCode::Char('m'));
        place(&mut show, "Composer", "Pawan");
        press(&mut show, KeyCode::Enter);
        assert_eq!(show.query(), "");
        let selected = show.selected();
        assert!(
            show.filtered_indices().contains(&selected),
            "el ejemplo abierto debe verse en el índice"
        );
    }
}
