//! Medidor de contexto: cuánta ventana del modelo ocupa la última petición.
//!
//! Dos estilos para poder compararlos: `Segments` (celdas `━`, aspecto de
//! mastra) y `Blocks` (`█░` con `ctx N%`, aspecto de Pawan). El ancho de la
//! salida es fijo por construcción: diez celdas y cifras formateadas.

use crate::widgets::theme::Theme;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

/// Número de celdas de la barra en ambos estilos.
const CELLS: usize = 10;

/// Umbral por defecto (en porcentaje) a partir del cual se avisa.
const DEFAULT_MARK_PERCENT: u8 = 80;

/// Estilo de presentación del medidor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeterStyle {
    /// Celdas `━` con fracción `used/window` y porcentaje.
    Segments,
    /// Bloques `█░` con `ctx N%`.
    Blocks,
}

/// Nivel de aviso del medidor, según el porcentaje de ocupación.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeterLevel {
    Normal,
    Warn,
    Critical,
}

/// Estado del medidor: tokens usados y ventana conocida del modelo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextMeter {
    pub used: u64,
    pub window: Option<u64>,
    /// Porcentaje a partir del cual el nivel pasa a `Warn`.
    pub mark_percent: u8,
}

impl ContextMeter {
    /// Crea un medidor con el umbral de aviso por defecto (80 %).
    pub fn new(used: u64, window: Option<u64>) -> Self {
        Self {
            used,
            window,
            mark_percent: DEFAULT_MARK_PERCENT,
        }
    }

    /// Porcentaje redondeado al entero más cercano, sin tope superior.
    /// `None` si no hay ventana conocida (o si la ventana es cero).
    pub fn percent(&self) -> Option<u32> {
        let window = u128::from(self.window?);
        if window == 0 {
            return None;
        }
        // Redondeo al entero más cercano (mitad hacia arriba) sin coma flotante.
        let scaled = u128::from(self.used) * 100;
        let rounded = (scaled * 2 + window) / (window * 2);
        Some(u32::try_from(rounded).unwrap_or(u32::MAX))
    }

    /// `Normal` por debajo del umbral, `Warn` desde él, `Critical` desde el 100 %.
    /// Sin ventana conocida el nivel es siempre `Normal`.
    ///
    /// Se compara con aritmética entera exacta y no con el porcentaje redondeado: 79,6 % se
    /// muestra como «80 %» pero aún no ha alcanzado el umbral, y la compactación (que usa la
    /// misma comparación exacta) todavía no ha saltado. El medidor no debe adelantarse a ella.
    pub fn level(&self) -> MeterLevel {
        let Some(window) = self.window.filter(|&w| w > 0) else {
            return MeterLevel::Normal;
        };
        let used = u128::from(self.used);
        let window = u128::from(window);
        if used >= window {
            MeterLevel::Critical
        } else if used * 100 >= window * u128::from(self.mark_percent) {
            MeterLevel::Warn
        } else {
            MeterLevel::Normal
        }
    }

    /// Número de celdas ocupadas: `round(min(pct, 100) / 10)`.
    fn filled_cells(pct: u32) -> usize {
        let capped = pct.min(100) as usize;
        (capped + 5) / 10
    }
}

/// Formatea una cantidad de tokens: `850`, `142k`, `1.2M`.
fn format_tokens(tokens: u64) -> String {
    if tokens < 1_000 {
        tokens.to_string()
    } else if tokens < 10_000 {
        format!("{:.1}k", tokens as f64 / 1_000.0)
    } else if tokens < 999_500 {
        format!("{}k", (tokens + 500) / 1_000)
    } else {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    }
}

/// Texto plano del medidor, sin estilos. Vacío si no hay uso.
pub fn meter_text(meter: &ContextMeter, style: MeterStyle) -> String {
    if meter.used == 0 {
        return String::new();
    }
    let Some(window) = meter.window else {
        return format!("ctx {} (sin ventana)", format_tokens(meter.used));
    };
    let pct = meter.percent().unwrap_or(0);
    let filled = ContextMeter::filled_cells(pct);
    match style {
        MeterStyle::Segments => {
            // En texto plano las diez celdas son `━`; el reparto se ve en los estilos.
            format!(
                "{}  {}/{}  {} %",
                "━".repeat(CELLS),
                format_tokens(meter.used),
                format_tokens(window),
                pct
            )
        }
        MeterStyle::Blocks => {
            format!(
                "ctx {}% {}{}",
                pct,
                "█".repeat(filled),
                "░".repeat(CELLS - filled)
            )
        }
    }
}

/// Ancho en columnas de `meter_text`.
pub fn meter_width(meter: &ContextMeter, style: MeterStyle) -> u16 {
    meter_text(meter, style).width() as u16
}

/// Spans con estilo según el nivel y el tema. Las celdas ocupadas toman el
/// color del nivel, las libres el estilo `muted` y el resto el neutro.
pub fn meter_spans(meter: &ContextMeter, style: MeterStyle, theme: &Theme) -> Vec<Span<'static>> {
    if meter.used == 0 {
        return Vec::new();
    }
    let neutral = theme.styles.default;
    if meter.window.is_none() {
        return vec![Span::styled(meter_text(meter, style), neutral)];
    }
    let pct = meter.percent().unwrap_or(0);
    let filled = ContextMeter::filled_cells(pct);
    let free = CELLS - filled;
    let level_style = match meter.level() {
        MeterLevel::Normal => neutral,
        MeterLevel::Warn => theme.styles.warning,
        MeterLevel::Critical => theme.styles.error,
    };
    let muted = theme.styles.muted;

    let (filled_glyph, free_glyph) = match style {
        MeterStyle::Segments => ("━", "━"),
        MeterStyle::Blocks => ("█", "░"),
    };

    let mut spans = Vec::with_capacity(4);
    match style {
        MeterStyle::Segments => {
            spans.push(Span::styled(filled_glyph.repeat(filled), level_style));
            spans.push(Span::styled(free_glyph.repeat(free), muted));
            let rest = meter_text(meter, style)
                .chars()
                .skip(CELLS)
                .collect::<String>();
            spans.push(Span::styled(rest, neutral));
        }
        MeterStyle::Blocks => {
            spans.push(Span::styled(format!("ctx {pct}% "), neutral));
            spans.push(Span::styled(filled_glyph.repeat(filled), level_style));
            spans.push(Span::styled(free_glyph.repeat(free), muted));
        }
    }
    spans.retain(|span| !span.content.is_empty());
    spans
}

/// Pinta el medidor alineado a la derecha de `area`, en su primera fila.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    meter: &ContextMeter,
    style: MeterStyle,
    theme: &Theme,
) {
    let width = meter_width(meter, style);
    if width == 0 || area.width == 0 || area.height == 0 {
        return;
    }
    let width = width.min(area.width);
    let rect = Rect::new(area.right() - width, area.y, width, 1);
    let line = Line::from(meter_spans(meter, style, theme));
    frame.render_widget(Paragraph::new(line), rect);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn text(meter: &ContextMeter, style: MeterStyle) -> String {
        meter_text(meter, style)
    }

    fn span_text(spans: &[Span<'static>]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    fn row(term: &Terminal<TestBackend>, y: u16) -> String {
        let buf = term.backend().buffer();
        let width = buf.area.width;
        (0..width).map(|x| buf[(x, y)].symbol()).collect()
    }

    fn sample() -> ContextMeter {
        ContextMeter::new(142_000, Some(200_000))
    }

    #[test]
    fn level_uses_the_exact_ratio_not_the_rounded_percent() {
        // 79 999 de 100 000 se redondea a «80 %», pero no ha alcanzado el umbral.
        let casi = ContextMeter::new(79_999, Some(100_000));
        assert_eq!(casi.percent(), Some(80));
        assert_eq!(casi.level(), MeterLevel::Normal);
        assert_eq!(
            ContextMeter::new(80_000, Some(100_000)).level(),
            MeterLevel::Warn
        );
        // 99 999 de 100 000 se redondea a «100 %», pero aún no es crítico.
        assert_eq!(
            ContextMeter::new(99_999, Some(100_000)).level(),
            MeterLevel::Warn
        );
        assert_eq!(
            ContextMeter::new(100_000, Some(100_000)).level(),
            MeterLevel::Critical
        );
    }

    #[test]
    fn percent_and_level_follow_the_mark() {
        let at = |used: u64| ContextMeter::new(used, Some(100));
        assert_eq!(at(79).level(), MeterLevel::Normal);
        assert_eq!(at(80).level(), MeterLevel::Warn);
        assert_eq!(at(99).level(), MeterLevel::Warn);
        assert_eq!(at(100).level(), MeterLevel::Critical);
        assert_eq!(at(130).level(), MeterLevel::Critical);
        assert_eq!(ContextMeter::new(500, None).level(), MeterLevel::Normal);
        assert_eq!(at(130).percent(), Some(130));
        assert_eq!(ContextMeter::new(500, None).percent(), None);
    }

    #[test]
    fn no_usage_renders_nothing() {
        let meter = ContextMeter::new(0, Some(200_000));
        assert_eq!(text(&meter, MeterStyle::Segments), "");
        assert_eq!(text(&meter, MeterStyle::Blocks), "");
        assert_eq!(meter_width(&meter, MeterStyle::Segments), 0);
        assert!(meter_spans(&meter, MeterStyle::Blocks, &Theme::test()).is_empty());
    }

    #[test]
    fn unknown_window_renders_no_bar() {
        let meter = ContextMeter::new(142_000, None);
        for style in [MeterStyle::Segments, MeterStyle::Blocks] {
            let out = text(&meter, style);
            assert_eq!(out, "ctx 142k (sin ventana)");
            assert!(!out.contains('━') && !out.contains('█') && !out.contains('░'));
        }
    }

    #[test]
    fn segments_text_has_ten_cells_and_the_fraction() {
        assert_eq!(
            text(&sample(), MeterStyle::Segments),
            "━━━━━━━━━━  142k/200k  71 %"
        );
    }

    #[test]
    fn blocks_text_matches_the_pawan_look() {
        assert_eq!(text(&sample(), MeterStyle::Blocks), "ctx 71% ███████░░░");
    }

    #[test]
    fn over_100_percent_fills_the_bar_and_shows_the_real_number() {
        let meter = ContextMeter::new(246_000, Some(200_000));
        assert_eq!(text(&meter, MeterStyle::Blocks), "ctx 123% ██████████");
        assert_eq!(
            text(&meter, MeterStyle::Segments),
            "━━━━━━━━━━  246k/200k  123 %"
        );
    }

    #[test]
    fn spans_split_filled_and_free_cells_with_different_styles() {
        let theme = Theme::test();
        let spans = meter_spans(&sample(), MeterStyle::Segments, &theme);
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content.as_ref(), "━━━━━━━");
        assert_eq!(spans[1].content.as_ref(), "━━━");
        assert_eq!(spans[0].style, theme.styles.default);
        assert_eq!(spans[1].style, theme.styles.muted);
        assert_ne!(spans[0].style, spans[1].style);
        assert_eq!(span_text(&spans), text(&sample(), MeterStyle::Segments));

        let blocks = meter_spans(&sample(), MeterStyle::Blocks, &theme);
        assert_eq!(span_text(&blocks), text(&sample(), MeterStyle::Blocks));
        assert_eq!(blocks[1].content.as_ref(), "███████");
        assert_eq!(blocks[2].content.as_ref(), "░░░");
    }

    #[test]
    fn warn_and_critical_use_the_theme_alert_styles() {
        let theme = Theme::test();
        let warn = meter_spans(
            &ContextMeter::new(170_000, Some(200_000)),
            MeterStyle::Segments,
            &theme,
        );
        assert_eq!(warn[0].style, theme.styles.warning);
        let critical = meter_spans(
            &ContextMeter::new(210_000, Some(200_000)),
            MeterStyle::Segments,
            &theme,
        );
        assert_eq!(critical[0].style, theme.styles.error);
    }

    #[test]
    fn width_matches_the_text() {
        for style in [MeterStyle::Segments, MeterStyle::Blocks] {
            let meter = sample();
            assert_eq!(
                meter_width(&meter, style) as usize,
                text(&meter, style).width()
            );
        }
    }

    #[test]
    fn render_right_aligns_inside_the_area() {
        let theme = Theme::test();
        let meter = ContextMeter::new(100_000, Some(200_000));
        let expected = text(&meter, MeterStyle::Blocks);
        let width = expected.width();
        let mut term = Terminal::new(TestBackend::new(30, 1)).unwrap();
        term.draw(|f| render(f, f.area(), &meter, MeterStyle::Blocks, &theme))
            .unwrap();
        assert_eq!(
            row(&term, 0),
            format!("{}{}", " ".repeat(30 - width), expected)
        );
    }
}
