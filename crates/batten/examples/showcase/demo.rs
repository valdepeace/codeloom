//! Una demo del showcase: un widget (o composición) autónomo.

use batten::widgets::{Anim, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::Frame;

pub trait Demo {
    /// Título en el índice.
    fn title(&self) -> &str;
    /// Línea de ayuda con las teclas propias del demo.
    fn hint(&self) -> &str;
    /// Pinta la demo dentro de `area`.
    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme);
    /// Avanza la animación (una vez por frame). Por defecto no hace nada.
    fn tick(&mut self, _anim: &Anim) {}
    /// Procesa una tecla. En v0 el valor de retorno se ignora.
    fn handle_key(&mut self, _code: KeyCode) -> bool {
        false
    }
}
