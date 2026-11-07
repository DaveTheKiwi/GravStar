//! A rectangular selection of tiles in the editor. It runs between two corner
//! tiles (both included), however they were dragged; a single click selects
//! one tile.

use crate::editor_document::TileRect;

#[derive(Clone, Debug, Default)]
pub struct Selection {
    anchor: Option<(i32, i32)>,
    corner: (i32, i32),
    pub dragging: bool,
}

impl Selection {
    /// Start a new selection (replacing any there was)
    pub fn begin(&mut self, col: i32, row: i32) {
        self.anchor = Some((col, row));
        self.corner = (col, row);
        self.dragging = true;
    }

    pub fn drag_to(&mut self, col: i32, row: i32) {
        if self.dragging {
            self.corner = (col, row);
        }
    }

    pub fn end(&mut self) {
        self.dragging = false;
    }

    pub fn select_all(&mut self, width: usize, height: usize) {
        self.set_rect((0, 0, width, height));
    }

    pub fn clear(&mut self) {
        self.anchor = None;
        self.dragging = false;
    }

    pub fn has_selection(&self) -> bool {
        self.anchor.is_some()
    }

    /// (col, row, width, height) of the selection
    pub fn rect(&self) -> Option<TileRect> {
        let (ax, ay) = self.anchor?;
        let (left, right) = (ax.min(self.corner.0), ax.max(self.corner.0));
        let (top, bottom) = (ay.min(self.corner.1), ay.max(self.corner.1));
        Some((left, top, (right - left + 1) as usize, (bottom - top + 1) as usize))
    }

    /// Select exactly this rectangle
    pub fn set_rect(&mut self, rect: TileRect) {
        let (col, row, width, height) = rect;
        self.anchor = Some((col, row));
        self.corner = (col + width as i32 - 1, row + height as i32 - 1);
        self.dragging = false;
    }

    pub fn contains(&self, col: i32, row: i32) -> bool {
        self.rect().map_or(false, |(l, t, w, h)| {
            col >= l && col < l + w as i32 && row >= t && row < t + h as i32
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_selects_one_tile_and_a_drag_any_way_round_a_rectangle() {
        let mut s = Selection::default();
        assert!(s.rect().is_none());
        s.begin(4, 4);
        assert_eq!(s.rect(), Some((4, 4, 1, 1)));
        s.drag_to(2, 7);
        assert_eq!(s.rect(), Some((2, 4, 3, 4)));
        s.end();
        s.drag_to(9, 9); // not dragging any more
        assert_eq!(s.rect(), Some((2, 4, 3, 4)));
        assert!(s.contains(3, 5) && !s.contains(5, 5));
    }

    #[test]
    fn select_all_and_clear() {
        let mut s = Selection::default();
        s.select_all(10, 6);
        assert_eq!(s.rect(), Some((0, 0, 10, 6)));
        s.clear();
        assert!(!s.has_selection() && !s.contains(0, 0));
    }
}
