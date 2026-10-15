use std::ops::Range;

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
}
