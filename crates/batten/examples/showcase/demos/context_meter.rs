//! Demo del medidor de contexto: los dos estilos apilados, el ciclo de compactación
//! y filas fijas de referencia.
//!
//! El reloj es un contador de frames propio que avanza en `tick`. Cada
//! `FRAMES_PER_STEP` frames el medidor da un paso, para que el movimiento sea legible.

use crate::demo::Demo;
use batten::widgets::context_meter::{self, ContextMeter, MeterStyle};
use batten::widgets::{Anim, Theme};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

/// Ventana de contexto del demo, en tokens.
const WINDOW: u64 = 200_000;
/// Frames que tarda el medidor en dar un paso.
const FRAMES_PER_STEP: u64 = 3;
/// Porcentaje con el que termina el barrido; el uso sube de 0 a este valor y reinicia.
const SWEEP_MAX_PERCENT: u64 = 120;
/// Tokens que añade cada paso de subida del ciclo.
const CYCLE_RISE: u64 = 4_000;
/// Pasos de subida del ciclo, desde la cola tras la caída hasta el 80 %.
const CYCLE_CLIMB_STEPS: u64 = 30;
/// Pasos en los que se muestra «compactando…» con el uso en el 80 %.
const CYCLE_COMPACT_STEPS: u64 = 8;
/// Tokens del resumen vigente tras compactar.
const SUMMARY: u64 = 25_000;
/// Uso tras la caída: el resumen más una cola de mensajes.
const AFTER_DROP: u64 = 40_000;
/// Anchura de la columna de etiquetas, en celdas.
const LABEL_WIDTH: u16 = 16;

/// Una fila del demo: un medidor con su etiqueta, un hueco o el texto final.
enum Row {
    Meter {
        label: &'static str,
        label_style: Style,
        meter: ContextMeter,
        style: MeterStyle,
    },
    Blank,
    Text(&'static str),
}

/// Porcentaje del barrido en `ticks`: sube de 0 a 120 y vuelve a empezar.
fn sweep_percent(ticks: u64) -> u64 {
    (ticks / FRAMES_PER_STEP) % (SWEEP_MAX_PERCENT + 1)
}

/// Estado del ciclo de compactación en `ticks`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CycleState {
    meter: ContextMeter,
    compacting: bool,
}

/// Ciclo: sube desde «resumen + cola» hasta el 80 %, se queda un rato en «compactando…»
/// y cae de nuevo a «resumen + cola».
fn cycle_state(ticks: u64) -> CycleState {
    let period = CYCLE_CLIMB_STEPS + CYCLE_COMPACT_STEPS;
    let step = (ticks / FRAMES_PER_STEP) % period;
    let (used, compacting) = if step < CYCLE_CLIMB_STEPS {
        (AFTER_DROP + step * CYCLE_RISE, false)
    } else {
        (AFTER_DROP + CYCLE_CLIMB_STEPS * CYCLE_RISE, true)
    };
    CycleState {
        meter: ContextMeter::new(used, Some(WINDOW)).with_memory(SUMMARY),
        compacting,
    }
}

/// Filas del demo, en orden de prioridad: si la altura no da para todas, se pintan las primeras.
fn rows(ticks: u64, theme: &Theme) -> Vec<Row> {
    let muted = theme.styles.muted;
    let sweep = ContextMeter::new(WINDOW * sweep_percent(ticks) / 100, Some(WINDOW));
    let cycle = cycle_state(ticks);
    let at = |percent: u64| ContextMeter::new(WINDOW * percent / 100, Some(WINDOW));

    let (cycle_label, cycle_label_style) = if cycle.compacting {
        ("compactando…", theme.styles.warning)
    } else {
        ("ciclo", muted)
    };

    vec![
        Row::Meter {
            label: "Segments 200k",
            label_style: muted,
            meter: sweep,
            style: MeterStyle::Segments,
        },
        Row::Meter {
            label: "Blocks 200k",
            label_style: muted,
            meter: sweep,
            style: MeterStyle::Blocks,
        },
        Row::Blank,
        Row::Meter {
            label: cycle_label,
            label_style: cycle_label_style,
            meter: cycle.meter,
            style: MeterStyle::Segments,
        },
        Row::Blank,
        Row::Meter {
            label: "50 %",
            label_style: muted,
            meter: at(50),
            style: MeterStyle::Segments,
        },
        Row::Meter {
            label: "85 % · aviso",
            label_style: muted,
            meter: at(85),
            style: MeterStyle::Blocks,
        },
        Row::Meter {
            label: "105 % · crítico",
            label_style: muted,
            meter: at(105),
            style: MeterStyle::Segments,
        },
        Row::Meter {
            label: "con resumen",
            label_style: muted,
            meter: ContextMeter::new(100_000, Some(WINDOW)).with_memory(SUMMARY),
            style: MeterStyle::Blocks,
        },
        Row::Meter {
            label: "sin ventana",
            label_style: muted,
            meter: ContextMeter::new(142_000, None),
            style: MeterStyle::Segments,
        },
        Row::Blank,
        Row::Text(
            "Aviso desde el 80 %: ahí salta la compactación. El resumen (color de acento del tema) es una estimación.",
        ),
    ]
}

pub struct ContextMeterDemo {
    ticks: u64,
}

impl ContextMeterDemo {
    pub fn new() -> Self {
        Self { ticks: 0 }
    }
}

impl Demo for ContextMeterDemo {
    fn title(&self) -> &str {
        "Context meter"
    }

    fn hint(&self) -> &str {
        "avanza sola · ventana 200k · Segments y Blocks"
    }

    fn tick(&mut self, _anim: &Anim) {
        self.ticks = self.ticks.wrapping_add(1);
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        for (index, row) in rows(self.ticks, theme).into_iter().enumerate() {
            let Ok(offset) = u16::try_from(index) else {
                break;
            };
            if offset >= area.height {
                break;
            }
            let row_y = area.y.saturating_add(offset);
            match row {
                Row::Meter {
                    label,
                    label_style,
                    meter,
                    style,
                } => {
                    let row_area = Rect::new(area.x, row_y, area.width, 1);
                    let [label_area, meter_area] =
                        Layout::horizontal([Constraint::Length(LABEL_WIDTH), Constraint::Min(0)])
                            .areas(row_area);
                    frame.render_widget(
                        Paragraph::new(Line::from(Span::styled(label, label_style))),
                        label_area,
                    );
                    context_meter::render(frame, meter_area, &meter, style, theme);
                }
                Row::Blank => {}
                Row::Text(text) => {
                    // El texto ocupa lo que quede de área, con ajuste de línea.
                    let rest = Rect::new(area.x, row_y, area.width, area.height - offset);
                    frame.render_widget(
                        Paragraph::new(Line::from(Span::styled(text, theme.styles.muted)))
                            .wrap(Wrap { trim: true }),
                        rest,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use batten::widgets::context_meter::MeterLevel;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    /// Pinta el demo en un `TestBackend` y devuelve las filas de texto.
    fn draw(demo: &mut ContextMeterDemo, width: u16, height: u16) -> Vec<String> {
        let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
        term.draw(|f| {
            let area = f.area();
            demo.render(f, area, &Theme::test());
        })
        .unwrap();
        let buffer = term.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect()
            })
            .collect()
    }

    #[test]
    fn renders_both_glyph_families_at_80x24() {
        let mut demo = ContextMeterDemo::new();
        let filas = draw(&mut demo, 80, 24);
        assert!(filas.iter().any(|fila| fila.contains('━')), "falta ━");
        assert!(filas.iter().any(|fila| fila.contains('█')), "falta █");
        assert!(filas
            .iter()
            .any(|fila| fila.contains("Aviso desde el 80 %")));
    }

    #[test]
    fn small_area_paints_what_fits_without_panicking() {
        let mut demo = ContextMeterDemo::new();
        let filas = draw(&mut demo, 40, 6);
        assert_eq!(filas.len(), 6);
        assert!(filas[0].contains("Segments 200k"));
        assert!(filas[1].contains("Blocks 200k"));
        // Con altura 6 el texto final no cabe y el ciclo sí.
        assert!(filas.iter().any(|fila| fila.contains("ciclo")));
        for (w, h) in [(0u16, 0u16), (1, 1), (3, 2)] {
            let _ = draw(&mut demo, w, h);
        }
    }

    #[test]
    fn sweep_goes_from_0_to_120_and_restarts() {
        assert_eq!(sweep_percent(0), 0);
        assert_eq!(
            sweep_percent(SWEEP_MAX_PERCENT * FRAMES_PER_STEP),
            SWEEP_MAX_PERCENT
        );
        assert_eq!(sweep_percent((SWEEP_MAX_PERCENT + 1) * FRAMES_PER_STEP), 0);
    }

    #[test]
    fn cycle_climbs_to_80_compacts_and_drops() {
        let at_step = |step: u64| cycle_state(step * FRAMES_PER_STEP);

        let before = at_step(CYCLE_CLIMB_STEPS - 1);
        assert!(!before.compacting);

        let peak = at_step(CYCLE_CLIMB_STEPS);
        assert!(peak.compacting);
        assert_eq!(peak.meter.used, 160_000);
        assert_eq!(peak.meter.level(), MeterLevel::Warn);

        // Tras la fase de compactación el ciclo vuelve al inicio: cae a «resumen + cola».
        let after = at_step(CYCLE_CLIMB_STEPS + CYCLE_COMPACT_STEPS);
        assert!(!after.compacting);
        assert_eq!(after.meter.used, AFTER_DROP);
        assert_eq!(after.meter.memory, SUMMARY);
    }

    #[test]
    fn compacting_label_appears_when_the_clock_advances() {
        let mut demo = ContextMeterDemo::new();
        let anim = Anim::new();
        let mut visto = false;
        for _ in 0..200 {
            demo.tick(&anim);
            let filas = draw(&mut demo, 80, 24);
            if filas.iter().any(|fila| fila.contains("compactando…")) {
                visto = true;
                break;
            }
        }
        assert!(visto, "nunca apareció «compactando…»");
    }
}
