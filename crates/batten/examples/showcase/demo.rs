//! Una demo del showcase: un widget (o composición) autónomo.

use batten::widgets::{Anim, Theme};
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::Frame;

pub trait Demo {
    /// Título interno; el catálogo agrupado define el texto visible del índice.
    #[allow(dead_code)]
    fn title(&self) -> &str;
    /// Línea de ayuda con las teclas propias del demo.
    fn hint(&self) -> &str;
    /// Pinta la demo dentro de `area`.
    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme);
    /// La referencia representa una pantalla completa y omite el marco del showcase.
    fn fullscreen(&self) -> bool {
        false
    }
    /// Avanza la animación (una vez por frame). Por defecto no hace nada.
    fn tick(&mut self, _anim: &Anim) {}
    /// Procesa una tecla. En v0 el valor de retorno se ignora.
    fn handle_key(&mut self, _code: KeyCode) -> bool {
        false
    }
    /// Tecla con modificadores. Por defecto los ignora y llama a `handle_key`.
    fn handle_key_event(&mut self, code: KeyCode, _mods: KeyModifiers) -> bool {
        self.handle_key(code)
    }
}
