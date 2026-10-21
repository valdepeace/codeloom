//! Estado del diálogo de preguntas (`ask_user`). Sin teclas reales: la app
//! traduce su teclado a [`QuestionKey`].

use std::collections::BTreeSet;

/// Una pregunta con sus opciones `(etiqueta, descripción)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionSpec {
    pub header: String,
    pub question: String,
    pub options: Vec<(String, String)>,
    /// `true`: selección múltiple; `false`: una sola opción.
    pub multi: bool,
}

/// Teclas que entiende el diálogo. La barra espaciadora llega como `Char(' ')`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestionKey {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Esc,
    Backspace,
    Char(char),
}

/// Resultado de una tecla.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestionOutcome {
    Pending,
    /// Respuestas de todas las preguntas, en orden.
    Done(Vec<Vec<String>>),
    Cancelled,
}

/// Estado de respuesta de una pregunta.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Slot {
    /// 0..=options.len(); `options.len()` es la fila «Other…».
    cursor: usize,
    selected: BTreeSet<usize>,
    other: String,
    editing: bool,
}

/// Diálogo de preguntas: una máquina pura sobre una lista de [`QuestionSpec`].
#[derive(Debug, Clone)]
pub struct QuestionDialog {
    specs: Vec<QuestionSpec>,
    slots: Vec<Slot>,
    current: usize,
}

impl QuestionDialog {
    /// # Panics
    ///
    /// Si `specs` está vacío: el diálogo necesita al menos una pregunta.
    pub fn new(specs: Vec<QuestionSpec>) -> Self {
        assert!(
            !specs.is_empty(),
            "el diálogo necesita al menos una pregunta"
        );
        let slots = vec![Slot::default(); specs.len()];
        Self {
            specs,
            slots,
            current: 0,
        }
    }

    /// Procesa una tecla y devuelve el resultado.
    pub fn handle(&mut self, key: QuestionKey) -> QuestionOutcome {
        if self.slot().editing {
            return self.handle_editing(key);
        }
        let n = self.spec().options.len();
        match key {
            QuestionKey::Esc => return QuestionOutcome::Cancelled,
            QuestionKey::Up => {
                let s = self.slot_mut();
                s.cursor = (s.cursor + n) % (n + 1);
            }
            QuestionKey::Down => {
                let s = self.slot_mut();
                s.cursor = (s.cursor + 1) % (n + 1);
            }
            QuestionKey::Left => {
                if self.current > 0 {
                    self.current -= 1;
                }
            }
            QuestionKey::Right => {
                if self.answered(self.current) && self.current + 1 < self.len() {
                    self.current += 1;
                }
            }
            QuestionKey::Backspace => {}
            QuestionKey::Char(' ') => {
                let cursor = self.slot().cursor;
                if cursor == n {
                    self.slot_mut().editing = true;
                } else if self.spec().multi {
                    self.toggle(cursor);
                } else {
                    self.select_only(cursor);
                }
            }
            QuestionKey::Char(_) => {}
            QuestionKey::Enter => {
                let cursor = self.slot().cursor;
                if cursor == n {
                    self.slot_mut().editing = true;
                    return QuestionOutcome::Pending;
                }
                if self.spec().multi {
                    // Sin nada elegido ni texto, Enter selecciona la fila actual.
                    let s = self.slot_mut();
                    if s.selected.is_empty() && s.other.trim().is_empty() {
                        s.selected.insert(cursor);
                    }
                } else {
                    self.select_only(cursor);
                }
                return self.confirm();
            }
        }
        QuestionOutcome::Pending
    }

    /// Teclas mientras se escribe el texto de «Other…».
    fn handle_editing(&mut self, key: QuestionKey) -> QuestionOutcome {
        match key {
            // Esc solo sale de la edición; conserva el texto.
            QuestionKey::Esc => {
                self.slot_mut().editing = false;
                QuestionOutcome::Pending
            }
            QuestionKey::Enter => {
                let multi = self.spec().multi;
                let s = self.slot_mut();
                s.editing = false;
                if s.other.trim().is_empty() {
                    return QuestionOutcome::Pending;
                }
                if !multi {
                    // En única, el texto libre sustituye a la opción elegida.
                    s.selected.clear();
                }
                self.confirm()
            }
            QuestionKey::Char(c) => {
                self.slot_mut().other.push(c);
                QuestionOutcome::Pending
            }
            QuestionKey::Backspace => {
                self.slot_mut().other.pop();
                QuestionOutcome::Pending
            }
            _ => QuestionOutcome::Pending,
        }
    }

    /// Alterna la opción `option` (selección múltiple).
    fn toggle(&mut self, option: usize) {
        let s = self.slot_mut();
        if !s.selected.remove(&option) {
            s.selected.insert(option);
        }
    }

    /// Deja solo `option` seleccionada (selección única) y descarta el texto libre.
    fn select_only(&mut self, option: usize) {
        let s = self.slot_mut();
        s.selected.clear();
        s.selected.insert(option);
        s.other.clear();
    }

    /// Confirma la pregunta actual: avanza, o devuelve todas las respuestas si era la última.
    fn confirm(&mut self) -> QuestionOutcome {
        if self.current + 1 < self.len() {
            self.current += 1;
            QuestionOutcome::Pending
        } else {
            QuestionOutcome::Done(self.answers())
        }
    }

    /// `true` si la pregunta `index` tiene alguna respuesta.
    fn answered(&self, index: usize) -> bool {
        let slot = &self.slots[index];
        !slot.selected.is_empty() || !slot.other.trim().is_empty()
    }

    /// Respuestas de todas las preguntas: etiquetas seleccionadas y, al final, el texto libre.
    fn answers(&self) -> Vec<Vec<String>> {
        self.specs
            .iter()
            .zip(&self.slots)
            .map(|(spec, slot)| {
                let mut answer: Vec<String> = slot
                    .selected
                    .iter()
                    .filter_map(|&i| spec.options.get(i))
                    .map(|(label, _)| label.clone())
                    .collect();
                let other = slot.other.trim();
                if !other.is_empty() {
                    answer.push(other.to_string());
                }
                answer
            })
            .collect()
    }

    fn slot_mut(&mut self) -> &mut Slot {
        &mut self.slots[self.current]
    }

    /// Pregunta actual (base 0).
    pub fn current(&self) -> usize {
        self.current
    }

    /// Número de preguntas.
    pub fn len(&self) -> usize {
        self.specs.len()
    }

    /// Nunca está vacío (se exige al menos una pregunta en `new`).
    pub fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }

    /// Especificación de la pregunta actual.
    pub fn spec(&self) -> &QuestionSpec {
        &self.specs[self.current]
    }

    /// Fila del cursor en la pregunta actual; `options.len()` es «Other…».
    pub fn cursor(&self) -> usize {
        self.slot().cursor
    }

    /// `true` si la opción `option` está seleccionada en la pregunta actual.
    pub fn is_selected(&self, option: usize) -> bool {
        self.slot().selected.contains(&option)
    }

    /// `true` mientras se escribe el texto de «Other…».
    pub fn editing_other(&self) -> bool {
        self.slot().editing
    }

    /// Texto de «Other…» de la pregunta actual.
    pub fn other_text(&self) -> &str {
        &self.slot().other
    }

    fn slot(&self) -> &Slot {
        &self.slots[self.current]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(multi: bool, n_options: usize) -> QuestionSpec {
        QuestionSpec {
            header: "Stack".into(),
            question: "Which?".into(),
            options: (0..n_options)
                .map(|i| ((b'a' + i as u8) as char).to_string())
                .map(|label| (label, String::new()))
                .collect(),
            multi,
        }
    }

    fn press(d: &mut QuestionDialog, keys: &[QuestionKey]) -> QuestionOutcome {
        let mut last = QuestionOutcome::Pending;
        for key in keys {
            last = d.handle(*key);
        }
        last
    }

    fn type_text(d: &mut QuestionDialog, text: &str) {
        for c in text.chars() {
            d.handle(QuestionKey::Char(c));
        }
    }

    /// Lleva el cursor a la fila «Other…» de la pregunta actual.
    fn to_other(d: &mut QuestionDialog) {
        while d.cursor() != d.spec().options.len() {
            d.handle(QuestionKey::Down);
        }
    }

    #[test]
    fn single_choice_selects_and_finishes() {
        let mut d = QuestionDialog::new(vec![spec(false, 3)]);
        let out = press(&mut d, &[QuestionKey::Down, QuestionKey::Enter]);
        assert_eq!(out, QuestionOutcome::Done(vec![vec!["b".to_string()]]));
    }

    #[test]
    fn multi_choice_toggles_then_confirms() {
        let mut d = QuestionDialog::new(vec![spec(true, 3)]);
        let out = press(
            &mut d,
            &[
                QuestionKey::Char(' '),
                QuestionKey::Down,
                QuestionKey::Char(' '),
                QuestionKey::Enter,
            ],
        );
        assert_eq!(
            out,
            QuestionOutcome::Done(vec![vec!["a".to_string(), "b".to_string()]])
        );
    }

    #[test]
    fn multi_enter_with_nothing_selects_current() {
        let mut d = QuestionDialog::new(vec![spec(true, 3)]);
        let out = press(&mut d, &[QuestionKey::Enter]);
        assert_eq!(out, QuestionOutcome::Done(vec![vec!["a".to_string()]]));
    }

    #[test]
    fn other_text_is_an_answer() {
        let mut d = QuestionDialog::new(vec![spec(false, 3)]);
        to_other(&mut d);
        let out = press(
            &mut d,
            &[
                QuestionKey::Enter,
                QuestionKey::Char('h'),
                QuestionKey::Char('i'),
                QuestionKey::Enter,
            ],
        );
        assert_eq!(out, QuestionOutcome::Done(vec![vec!["hi".to_string()]]));
    }

    #[test]
    fn empty_other_does_not_advance() {
        let mut d = QuestionDialog::new(vec![spec(false, 2), spec(false, 2)]);
        to_other(&mut d);
        let out = press(&mut d, &[QuestionKey::Enter, QuestionKey::Enter]);
        assert_eq!(out, QuestionOutcome::Pending);
        assert!(!d.editing_other());
        assert_eq!(d.current(), 0);
    }

    #[test]
    fn multi_other_is_appended_after_selected() {
        let mut d = QuestionDialog::new(vec![spec(true, 3)]);
        press(&mut d, &[QuestionKey::Char(' ')]);
        to_other(&mut d);
        press(&mut d, &[QuestionKey::Enter]);
        type_text(&mut d, "x");
        let out = press(&mut d, &[QuestionKey::Enter]);
        assert_eq!(
            out,
            QuestionOutcome::Done(vec![vec!["a".to_string(), "x".to_string()]])
        );
    }

    #[test]
    fn right_requires_an_answer() {
        let mut d = QuestionDialog::new(vec![spec(true, 2), spec(true, 2)]);
        press(&mut d, &[QuestionKey::Right]);
        assert_eq!(d.current(), 0);
        press(&mut d, &[QuestionKey::Char(' '), QuestionKey::Right]);
        assert_eq!(d.current(), 1);
        // En la última pregunta, «Right» no hace nada.
        press(&mut d, &[QuestionKey::Right]);
        assert_eq!(d.current(), 1);
    }

    #[test]
    fn left_goes_back_and_keeps_answers() {
        let mut d = QuestionDialog::new(vec![spec(false, 2), spec(false, 2)]);
        press(&mut d, &[QuestionKey::Enter]);
        assert_eq!(d.current(), 1);
        press(&mut d, &[QuestionKey::Left]);
        assert_eq!(d.current(), 0);
        assert!(d.is_selected(0));
        // En la primera pregunta, «Left» no hace nada.
        press(&mut d, &[QuestionKey::Left]);
        assert_eq!(d.current(), 0);
    }

    #[test]
    fn esc_cancels_but_only_leaves_edit_first() {
        let mut d = QuestionDialog::new(vec![spec(false, 2)]);
        to_other(&mut d);
        press(&mut d, &[QuestionKey::Enter, QuestionKey::Char('x')]);
        assert_eq!(press(&mut d, &[QuestionKey::Esc]), QuestionOutcome::Pending);
        assert!(!d.editing_other());
        assert_eq!(d.other_text(), "x");
        assert_eq!(
            press(&mut d, &[QuestionKey::Esc]),
            QuestionOutcome::Cancelled
        );
    }

    #[test]
    fn cursor_wraps_and_up_down_are_ignored_while_editing() {
        let mut d = QuestionDialog::new(vec![spec(false, 2)]);
        press(&mut d, &[QuestionKey::Up]);
        assert_eq!(d.cursor(), 2);
        press(&mut d, &[QuestionKey::Down]);
        assert_eq!(d.cursor(), 0);
        to_other(&mut d);
        press(
            &mut d,
            &[QuestionKey::Enter, QuestionKey::Down, QuestionKey::Up],
        );
        assert_eq!(d.cursor(), 2);
        assert!(d.editing_other());
    }

    #[test]
    fn two_questions_flow_to_done() {
        let mut d = QuestionDialog::new(vec![spec(false, 2), spec(false, 2)]);
        assert_eq!(d.len(), 2);
        let out = press(
            &mut d,
            &[QuestionKey::Enter, QuestionKey::Down, QuestionKey::Enter],
        );
        assert_eq!(
            out,
            QuestionOutcome::Done(vec![vec!["a".to_string()], vec!["b".to_string()]])
        );
    }
}
