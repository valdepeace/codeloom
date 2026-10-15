//! Estado de scroll en filas visuales (no líneas lógicas). El widget fija la
//! geometría al pintar (`set_geometry`); las teclas mueven `top`.

use std::ops::Range;

/// Posición de scroll para pintar una barra: el `offset` es la primera fila
/// visible, `content` el total de filas y `viewport` las filas visibles.
/// Puro (sin ratatui): el widget lo traduce a su `ScrollbarState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scroll {
    pub offset: usize,
    pub content: usize,
    pub viewport: usize,
}

/// Scroll del transcript. Por defecto sigue el final; si el usuario sube,
/// `top` queda fijo aunque llegue contenido nuevo.
#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    top: usize,
    follow: bool,
    total: usize,
    rows: usize,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            top: 0,
            follow: true,
            total: 0,
            rows: 0,
        }
    }
}

impl Viewport {
    pub fn new() -> Self {
        Self::default()
    }

    /// Lo llama el widget al pintar: filas totales del contenido y visibles.
    pub fn set_geometry(&mut self, total: usize, rows: usize) {
        self.total = total;
        self.rows = rows;
        // ponytail: en un resize solo se acota `top`; no se conserva la línea
        // que había arriba. Anclar a línea lógica si molesta.
        self.top = if self.follow {
            self.max_top()
        } else {
            self.top.min(self.max_top())
        };
    }

    fn max_top(&self) -> usize {
        self.total.saturating_sub(self.rows)
    }

    /// Vuelve al final y lo sigue.
    pub fn follow_bottom(&mut self) {
        self.follow = true;
        self.top = self.max_top();
    }

    pub fn is_following(&self) -> bool {
        self.follow
    }

    /// Sube `n` filas y deja de seguir el final. Si todo cabe, no hace nada.
    pub fn scroll_up(&mut self, n: usize) {
        if self.max_top() == 0 {
            return;
        }
        if self.follow {
            self.top = self.max_top();
        }
        self.follow = false;
        self.top = self.top.saturating_sub(n);
    }

    /// Baja `n` filas; al llegar al final vuelve a seguirlo.
    pub fn scroll_down(&mut self, n: usize) {
        self.top = self.top.saturating_add(n).min(self.max_top());
        if self.top >= self.max_top() {
            self.follow = true;
        }
    }

    /// Una página arriba (las filas visibles; mínimo 1).
    pub fn page_up(&mut self) {
        self.scroll_up(self.rows.max(1));
    }

    /// Una página abajo (las filas visibles; mínimo 1).
    pub fn page_down(&mut self) {
        self.scroll_down(self.rows.max(1));
    }

    /// Media página arriba (mínimo 1 fila).
    pub fn half_page_up(&mut self) {
        self.scroll_up((self.rows / 2).max(1));
    }

    /// Media página abajo (mínimo 1 fila).
    pub fn half_page_down(&mut self) {
        self.scroll_down((self.rows / 2).max(1));
    }

    /// Al principio (deja de seguir el final si no cabe todo).
    pub fn to_top(&mut self) {
        self.scroll_up(usize::MAX);
    }

    /// Desplaza lo mínimo para que se vean las filas `[start, end)`.
    pub fn ensure_visible(&mut self, start: usize, end: usize) {
        if start < self.top {
            self.top = start;
        } else if end > self.top + self.rows {
            self.top = end.saturating_sub(self.rows).min(self.max_top());
        }
        self.follow = self.top >= self.max_top();
    }

    /// Primera fila visible.
    pub fn top(&self) -> usize {
        self.top
    }

    /// Filas visibles `[top, top + rows)` acotadas al total.
    pub fn window(&self) -> Range<usize> {
        self.top..(self.top + self.rows).min(self.total)
    }

    /// Posición para la scrollbar.
    pub fn scroll(&self) -> Scroll {
        Scroll {
            offset: self.top,
            content: self.total,
            viewport: self.rows,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vp(total: usize, rows: usize) -> Viewport {
        let mut v = Viewport::new();
        v.set_geometry(total, rows);
        v
    }

    #[test]
    fn follows_bottom_by_default() {
        assert_eq!(vp(100, 10).window(), 90..100);
    }

    #[test]
    fn scroll_up_breaks_follow_and_clamps() {
        let mut v = vp(100, 10);
        v.scroll_up(25);
        assert!(!v.is_following());
        assert_eq!(v.window(), 65..75);
        v.scroll_up(10_000);
        assert_eq!(v.window(), 0..10);
    }

    #[test]
    fn reading_position_is_kept_when_content_grows() {
        let mut v = vp(100, 10);
        v.scroll_up(25);
        v.set_geometry(150, 10);
        assert_eq!(v.window(), 65..75);
        let mut following = vp(100, 10);
        following.set_geometry(150, 10);
        assert_eq!(following.window(), 140..150);
    }

    #[test]
    fn scrolling_down_to_the_end_resumes_follow() {
        let mut v = vp(100, 10);
        v.scroll_up(5);
        v.scroll_down(5);
        assert!(v.is_following());
        v.set_geometry(120, 10);
        assert_eq!(v.window(), 110..120);
    }

    #[test]
    fn content_that_fits_never_breaks_follow() {
        let mut v = vp(3, 10);
        v.scroll_up(1);
        assert!(v.is_following());
        assert_eq!(v.window(), 0..3);
    }

    #[test]
    fn resize_clamps_top() {
        let mut v = vp(100, 10);
        v.scroll_up(5);
        v.set_geometry(100, 50);
        assert_eq!(v.window(), 50..100);
    }

    #[test]
    fn handles_empty_and_zero_rows() {
        assert_eq!(vp(0, 10).window(), 0..0);
        assert_eq!(vp(3, 0).window(), 3..3);
    }

    #[test]
    fn page_helpers_move_by_rows() {
        let mut v = vp(100, 10);
        v.page_up();
        assert_eq!(v.window(), 80..90);
        v.half_page_up();
        assert_eq!(v.window(), 75..85);
        v.half_page_down();
        v.page_down();
        assert!(v.is_following());
        v.to_top();
        assert_eq!(v.window(), 0..10);
    }

    #[test]
    fn page_with_zero_rows_still_moves() {
        let mut v = vp(100, 0);
        v.page_up();
        assert_eq!(v.top(), 99);
    }

    #[test]
    fn ensure_visible_moves_the_minimum() {
        let mut v = vp(100, 10);
        v.ensure_visible(5, 6);
        assert_eq!(v.window(), 5..15);
        assert!(!v.is_following());
        v.ensure_visible(50, 52);
        assert_eq!(v.window(), 42..52);
        v.ensure_visible(98, 100);
        assert!(v.is_following());
    }

    #[test]
    fn scroll_reports_geometry() {
        assert_eq!(
            vp(100, 10).scroll(),
            Scroll {
                offset: 90,
                content: 100,
                viewport: 10
            }
        );
    }
}
