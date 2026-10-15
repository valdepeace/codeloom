use std::ops::Range;

/// Posición de scroll para pintar una barra: el `offset` es la primera línea
/// visible, `content` el total de líneas y `viewport` las filas visibles.
/// Puro (sin ratatui): el widget lo traduce a su `ScrollbarState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scroll {
    pub offset: usize,
    pub content: usize,
    pub viewport: usize,
}

/// Estado de scroll del transcript. Por defecto sigue el final (lo mas nuevo).
#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    scroll: usize,
    follow_bottom: bool,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            scroll: 0,
            follow_bottom: true,
        }
    }
}

impl Viewport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn follow_bottom(&mut self) {
        self.follow_bottom = true;
        self.scroll = 0;
    }

    /// Sube `n` lineas desde abajo (rompe el seguir-el-final).
    pub fn scroll_up(&mut self, n: usize) {
        self.scroll = self.scroll.saturating_add(n);
        self.follow_bottom = false;
    }

    /// Baja `n` lineas; al llegar abajo se reanuda el seguir-el-final.
    pub fn scroll_down(&mut self, n: usize) {
        self.scroll = self.scroll.saturating_sub(n);
        if self.scroll == 0 {
            self.follow_bottom = true;
        }
    }

    /// Rango `[start, end)` de lineas visibles.
    pub fn window(&self, total: usize, rows: usize) -> Range<usize> {
        if rows == 0 {
            return total..total;
        }
        let rows = rows.min(total.max(1));
        if self.follow_bottom {
            total.saturating_sub(rows)..total
        } else {
            let max_start = total.saturating_sub(rows);
            let start = max_start.saturating_sub(self.scroll.min(max_start));
            start..(start + rows).min(total)
        }
    }

    /// Posición de scroll para pintar una barra, derivada de `window`.
    pub fn scroll(&self, total: usize, rows: usize) -> Scroll {
        let start = self.window(total, rows).start;
        Scroll {
            offset: start,
            content: total,
            viewport: rows,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_bottom_by_default() {
        let vp = Viewport::new();
        assert_eq!(vp.window(100, 10), 90..100);
    }

    #[test]
    fn scrolling_up_breaks_follow_and_clamps() {
        let mut vp = Viewport::new();
        vp.scroll_up(25);
        assert!(!vp.follow_bottom);
        assert_eq!(vp.window(100, 10), 65..75);
        vp.scroll_up(10_000);
        assert_eq!(vp.window(100, 10), 0..10);
    }

    #[test]
    fn reclaiming_bottom_resumes_follow() {
        let mut vp = Viewport::new();
        vp.scroll_up(20);
        vp.follow_bottom();
        assert_eq!(vp.window(100, 10), 90..100);
    }

    #[test]
    fn handles_empty_and_tiny() {
        let vp = Viewport::new();
        assert_eq!(vp.window(0, 10), 0..0);
        assert_eq!(vp.window(3, 0), 3..3);
    }

    #[test]
    fn scroll_reports_position_and_lengths() {
        let vp = Viewport::new();
        let s = vp.scroll(100, 10);
        assert_eq!(s.content, 100);
        assert_eq!(s.viewport, 10);
        assert_eq!(s.offset, 90);
    }

    #[test]
    fn scroll_offset_follows_the_window() {
        let mut vp = Viewport::new();
        assert_eq!(vp.scroll(100, 10).offset, 90);
        vp.scroll_up(25);
        assert_eq!(vp.scroll(100, 10).offset, 65);
    }

    #[test]
    fn scroll_handles_empty() {
        let vp = Viewport::new();
        let s = vp.scroll(0, 10);
        assert_eq!(s.content, 0);
        assert_eq!(s.viewport, 10);
        assert_eq!(s.offset, 0);
    }
}
