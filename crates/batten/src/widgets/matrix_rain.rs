//! Lluvia Matrix transparente, con emisión progresiva y drenaje al detenerla.

use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::Frame;

const GLYPHS: &[char] = &[
    'ｱ', 'ｲ', 'ｳ', 'ｴ', 'ｵ', 'ｶ', 'ｷ', 'ｸ', 'ｹ', 'ｺ', 'ｻ', 'ｼ', 'ｽ', 'ｾ', 'ｿ', 'ﾀ', 'ﾁ', 'ﾂ', 'ﾃ',
    'ﾄ', 'ﾅ', 'ﾆ', 'ﾇ', 'ﾈ', 'ﾉ', 'ﾊ', 'ﾋ', 'ﾌ', 'ﾍ', 'ﾎ', 'ﾏ', 'ﾐ', 'ﾑ', 'ﾒ', 'ﾓ', 'ﾔ', 'ﾕ', 'ﾖ',
    'ﾗ', 'ﾘ', 'ﾙ', 'ﾚ', 'ﾛ', 'ﾜ', 'ﾝ', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', ':', '.',
    '=', '*', '+', '-', '<', '>',
];

#[derive(Clone, Copy)]
struct Drop {
    head: f32,
    speed: f32,
    length: u16,
    seed: u64,
}

/// Stateful background. Call `advance` on the UI clock and paint it before the
/// foreground widgets. `stop` stops emission; existing drops drain to the edge.
pub struct MatrixRain {
    columns: Vec<Option<Drop>>,
    width: u16,
    height: u16,
    emitting: bool,
    random: u64,
    frame: u64,
}

impl Default for MatrixRain {
    fn default() -> Self {
        Self {
            columns: Vec::new(),
            width: 0,
            height: 0,
            emitting: false,
            random: 0x4d41_5452_4958,
            frame: 0,
        }
    }
}

impl MatrixRain {
    pub fn start(&mut self) {
        self.emitting = true;
    }

    pub fn stop(&mut self) {
        self.emitting = false;
    }

    pub fn animating(&self) -> bool {
        self.emitting || self.columns.iter().any(Option::is_some)
    }

    /// Advances one 50 ms UI tick. Stopped drops keep moving until their tails
    /// pass the bottom, then the background releases the screen.
    pub fn advance(&mut self) {
        if !self.animating() {
            return;
        }
        self.frame = self.frame.wrapping_add(1);
        for slot in &mut self.columns {
            if let Some(mut drop) = *slot {
                drop.head += drop.speed;
                if drop.head - f32::from(drop.length) > f32::from(self.height) {
                    *slot = None;
                } else {
                    *slot = Some(drop);
                }
            }
        }
        if self.emitting {
            for column in 0..self.columns.len() {
                if self.columns[column].is_none() && self.random(100) < 3 {
                    let length = 5 + self.random(12) as u16;
                    let delay = self.random(u64::from(self.height.max(1))) as f32;
                    self.columns[column] = Some(Drop {
                        head: -(f32::from(length) + delay),
                        speed: 0.25 + self.random(65) as f32 / 100.0,
                        length,
                        seed: self.random(u64::MAX),
                    });
                }
            }
        }
    }

    /// Paints only the falling glyphs. Draw foreground widgets afterwards;
    /// leaving empty cells untouched avoids a full-screen flash on start/stop.
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        if area.is_empty() || !self.animating() {
            return;
        }
        if self.width != area.width {
            self.columns.resize(area.width as usize, None);
            self.width = area.width;
        }
        self.height = area.height;
        let buffer = frame.buffer_mut();
        for (column, drop) in self.columns.iter().enumerate() {
            let Some(drop) = drop else { continue };
            let x = area.x + column as u16;
            if x >= area.right() {
                continue;
            }
            let top = (drop.head - f32::from(drop.length)).ceil().max(0.0) as u16;
            let bottom = drop.head.floor().max(0.0).min(f32::from(area.height - 1)) as u16;
            if top > bottom {
                continue;
            }
            for row in top..=bottom {
                let distance = drop.head - f32::from(row);
                let intensity = (1.0 - distance / f32::from(drop.length)).clamp(0.0, 1.0);
                let glyph_index =
                    (glyph_hash(drop.seed, row, self.frame_phase()) as usize) % GLYPHS.len();
                let glyph = GLYPHS[glyph_index].to_string();
                let color = if distance < 1.0 {
                    Color::Rgb(125, 185, 140)
                } else {
                    Color::Rgb(
                        2,
                        (18.0 + intensity * 62.0) as u8,
                        (6.0 + intensity * 12.0) as u8,
                    )
                };
                let cell = &mut buffer[(x, area.y + row)];
                cell.set_symbol(&glyph);
                cell.set_fg(color);
            }
        }
    }

    fn frame_phase(&self) -> u64 {
        self.frame
    }

    fn random(&mut self, limit: u64) -> u64 {
        self.random = self
            .random
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        if limit == 0 {
            0
        } else {
            self.random % limit
        }
    }
}

fn glyph_hash(seed: u64, row: u16, phase: u64) -> u64 {
    seed.wrapping_add(u64::from(row).wrapping_mul(31))
        .wrapping_add(phase / 4)
        .wrapping_mul(2_654_435_761)
}
