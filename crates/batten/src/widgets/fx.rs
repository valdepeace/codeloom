//! Efectos visuales (tachyonfx) con presets que toman colores del tema.
//! Se aplican sobre el buffer después de pintar:
//! `effect.process(dt, frame.buffer_mut(), area)` en cada frame hasta `done()`.

use super::gradient::color_of;
use super::theme::Theme;
pub use tachyonfx;
use tachyonfx::{fx, Interpolation, Motion};
pub use tachyonfx::{Duration as FxDuration, Effect, EffectManager};

/// Apertura de diálogo: barrido de arriba abajo desde `dim` (250 ms).
pub fn dialog_open(theme: &Theme) -> Effect {
    fx::sweep_in(
        Motion::UpToDown,
        10,
        0,
        color_of(theme.styles.dim),
        (250, Interpolation::QuadOut),
    )
}

/// Entrada de aviso: barrido de izquierda a derecha desde `dim` (200 ms).
pub fn toast_in(theme: &Theme) -> Effect {
    fx::sweep_in(
        Motion::LeftToRight,
        8,
        0,
        color_of(theme.styles.dim),
        (200, Interpolation::QuadOut),
    )
}

/// Desvanecer el texto hacia `dim` en `ms` milisegundos.
pub fn fade_out(ms: u32, theme: &Theme) -> Effect {
    fx::fade_to_fg(color_of(theme.styles.dim), (ms, Interpolation::QuadIn))
}

/// Mensaje nuevo: el texto aparece desde `dim` (300 ms).
pub fn message_in(theme: &Theme) -> Effect {
    fx::fade_from_fg(color_of(theme.styles.dim), (300, Interpolation::QuadOut))
}

/// Destello de error: el texto parte del color de error y vuelve (400 ms).
pub fn error_flash(theme: &Theme) -> Effect {
    fx::fade_from_fg(color_of(theme.styles.error), (400, Interpolation::QuadOut))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::style::{Color, Style};

    fn run(mut e: Effect, ms: u32) -> (Effect, Buffer) {
        let mut b = Buffer::empty(Rect::new(0, 0, 6, 1));
        b.set_string(0, 0, "hola", Style::default().fg(Color::White));
        let area = b.area;
        e.process(FxDuration::from_millis(ms), &mut b, area);
        (e, b)
    }

    #[test]
    fn presets_finish_after_their_duration() {
        let th = Theme::test();
        let cases = [
            (dialog_open(&th), 250),
            (toast_in(&th), 200),
            (fade_out(150, &th), 150),
            (message_in(&th), 300),
            (error_flash(&th), 400),
        ];
        for (e, ms) in cases {
            let (e, _) = run(e, ms + 50);
            assert!(e.done());
        }
    }

    #[test]
    fn message_in_is_midway_after_100ms() {
        let (e, b) = run(message_in(&Theme::test()), 100);
        assert!(e.running());
        assert_ne!(b[(0, 0)].fg, Color::White);
    }

    #[test]
    fn empty_area_does_not_panic() {
        let mut b = Buffer::empty(Rect::new(0, 0, 0, 0));
        let mut e = dialog_open(&Theme::test());
        let area = b.area;
        e.process(FxDuration::from_millis(50), &mut b, area);
    }
}
