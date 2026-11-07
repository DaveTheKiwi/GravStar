//! The level being edited: a grid of tile ids plus the spawn and delivery
//! points. Pure data and the operations on it - no undo, no tools (see
//! editor.rs). Tiles are stored row by row: the tile at (col, row) is
//! tiles[row * width + col].

use crate::constants::TILE_SIZE;
use crate::level::{DeliveryPoint, Level, SpawnPoint};
use crate::tile_instance::TileInstance;

/// A rectangle of tiles: (col, row, width, height)
pub type TileRect = (i32, i32, usize, usize);

#[derive(Clone, Debug, PartialEq)]
pub struct EditorDocument {
    pub name: String,
    pub width: usize,
    pub height: usize,
    pub tiles: Vec<u16>,
    pub spawn_points: Vec<SpawnPoint>,
    pub delivery_points: Vec<DeliveryPoint>,
}

impl EditorDocument {
    /// A level with nothing in it
    pub fn new(name: &str, width: usize, height: usize) -> Self {
        EditorDocument {
            name: name.to_string(),
            width,
            height,
            tiles: vec![0; width * height],
            spawn_points: Vec::new(),
            delivery_points: Vec::new(),
        }
    }

    pub fn from_level(level: &Level) -> Self {
        EditorDocument {
            name: level.name.clone(),
            width: level.width,
            height: level.height,
            tiles: level.tiles.iter().map(|t| t.tile_id).collect(),
            spawn_points: level.spawn_points.clone(),
            delivery_points: level.delivery_points.clone(),
        }
    }

    /// A playable level (fresh, undamaged tiles)
    pub fn to_level(&self) -> Level {
        let mut level = Level::new(&self.name, self.width, self.height);
        level.tiles = self.tiles.iter().map(|&id| TileInstance::new(id)).collect();
        level.spawn_points = self.spawn_points.clone();
        level.delivery_points = self.delivery_points.clone();
        level
    }

    pub fn in_bounds(&self, col: i32, row: i32) -> bool {
        col >= 0 && row >= 0 && (col as usize) < self.width && (row as usize) < self.height
    }

    /// The index of (col, row) in `tiles`, or None outside the level
    pub fn index(&self, col: i32, row: i32) -> Option<usize> {
        if self.in_bounds(col, row) {
            Some(row as usize * self.width + col as usize)
        } else {
            None
        }
    }

    /// The tile id at (col, row); 0 (empty) outside the level
    pub fn tile_id(&self, col: i32, row: i32) -> u16 {
        self.index(col, row).map_or(0, |i| self.tiles[i])
    }

    /// The tile size in pixels (levels are all one size)
    pub fn tile_size(&self) -> usize {
        TILE_SIZE
    }

    // ---- spawn points

    pub fn spawn_at(&self, col: i32, row: i32) -> Option<&SpawnPoint> {
        self.spawn_points.iter().find(|s| s.col as i32 == col && s.row as i32 == row)
    }

    fn renumber_spawns(&mut self) {
        for (i, s) in self.spawn_points.iter_mut().enumerate() {
            s.id = i as u32 + 1;
        }
    }

    /// Add a spawn point. False if there is one there already or it is
    /// outside the level.
    pub fn add_spawn(&mut self, col: i32, row: i32) -> bool {
        if !self.in_bounds(col, row) || self.spawn_at(col, row).is_some() {
            return false;
        }
        let id = self.spawn_points.len() as u32 + 1;
        self.spawn_points.push(SpawnPoint { col: col as usize, row: row as usize, id });
        true
    }

    /// Remove the spawn point at (col, row), renumbering the rest. False if
    /// there was none.
    pub fn remove_spawn(&mut self, col: i32, row: i32) -> bool {
        let Some(at) = self.spawn_points.iter()
            .position(|s| s.col as i32 == col && s.row as i32 == row) else { return false };
        self.spawn_points.remove(at);
        self.renumber_spawns();
        true
    }

    pub fn has_minimum_spawn_points(&self) -> bool {
        self.spawn_points.len() >= 2
    }

    // ---- regions

    /// The part of a rectangle that lies inside the level, or None if none does
    pub fn clip_rect(&self, rect: TileRect) -> Option<TileRect> {
        let (col, row, width, height) = rect;
        let left = col.max(0);
        let top = row.max(0);
        let right = (col + width as i32).min(self.width as i32);
        let bottom = (row + height as i32).min(self.height as i32);
        if right <= left || bottom <= top {
            return None;
        }
        Some((left, top, (right - left) as usize, (bottom - top) as usize))
    }

    /// The tile ids of a rectangle (the part inside the level), row by row,
    /// and the size of what was copied
    pub fn copy_region(&self, rect: TileRect) -> Option<(Vec<u16>, usize, usize)> {
        let (left, top, w, h) = self.clip_rect(rect)?;
        let mut out = Vec::with_capacity(w * h);
        for r in top..top + h as i32 {
            let start = r as usize * self.width + left as usize;
            out.extend_from_slice(&self.tiles[start..start + w]);
        }
        Some((out, w, h))
    }

    /// True if any tile in the rectangle is not empty
    pub fn region_has_tiles(&self, rect: TileRect) -> bool {
        let Some((left, top, w, h)) = self.clip_rect(rect) else { return false };
        (top..top + h as i32).any(|r| {
            let start = r as usize * self.width + left as usize;
            self.tiles[start..start + w].iter().any(|&t| t != 0)
        })
    }

    /// The tiles a flood fill started at (col, row) would change, as indices:
    /// those joined to it up, down, left and right (not diagonally) holding
    /// the same tile id. With a `limit` the fill stays inside it. Empty if the
    /// start is outside the level or the limit.
    pub fn find_fill_region(&self, col: i32, row: i32, limit: Option<TileRect>) -> Vec<usize> {
        if !self.in_bounds(col, row) {
            return Vec::new();
        }
        let (left, top, right, bottom) = match limit {
            None => (0, 0, self.width as i32, self.height as i32),
            Some((c, r, w, h)) => (c.max(0), r.max(0),
                                   (c + w as i32).min(self.width as i32),
                                   (r + h as i32).min(self.height as i32)),
        };
        if col < left || col >= right || row < top || row >= bottom {
            return Vec::new();
        }
        let width = self.width;
        let target = self.tiles[row as usize * width + col as usize];
        let mut seen = vec![false; self.tiles.len()];
        let mut region = Vec::new();
        let mut stack = vec![(col, row)];
        seen[row as usize * width + col as usize] = true;
        while let Some((c, r)) = stack.pop() {
            region.push(r as usize * width + c as usize);
            for (nc, nr) in [(c + 1, r), (c - 1, r), (c, r + 1), (c, r - 1)] {
                if nc < left || nc >= right || nr < top || nr >= bottom {
                    continue;
                }
                let i = nr as usize * width + nc as usize;
                if !seen[i] && self.tiles[i] == target {
                    seen[i] = true;
                    stack.push((nc, nr));
                }
            }
        }
        region
    }

    /// Resize the level, keeping the tiles that still fit. Spawn and
    /// delivery points that fall outside the new size are dropped (and the
    /// spawn points renumbered). Returns how many points were dropped.
    pub fn resize(&mut self, new_width: usize, new_height: usize) -> usize {
        let mut tiles = vec![0u16; new_width * new_height];
        for row in 0..self.height.min(new_height) {
            let (from, to) = (row * self.width, row * new_width);
            let n = self.width.min(new_width);
            tiles[to..to + n].copy_from_slice(&self.tiles[from..from + n]);
        }
        self.tiles = tiles;
        self.width = new_width;
        self.height = new_height;
        let before = self.spawn_points.len() + self.delivery_points.len();
        let (w, h) = (new_width, new_height);
        self.spawn_points.retain(|s| s.col < w && s.row < h);
        self.delivery_points.retain(|p| p.col < w && p.row < h);
        self.renumber_spawns();
        before - self.spawn_points.len() - self.delivery_points.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(w: usize, h: usize) -> EditorDocument {
        EditorDocument::new("t", w, h)
    }

    #[test]
    fn a_new_document_is_empty_and_indexes_row_by_row() {
        let mut d = doc(4, 3);
        assert_eq!(d.tiles.len(), 12);
        d.tiles[1 * 4 + 2] = 7;
        assert_eq!(d.tile_id(2, 1), 7);
        assert_eq!(d.index(2, 1), Some(6));
        assert_eq!(d.tile_id(-1, 0), 0);
        assert_eq!(d.index(4, 0), None);
    }

    #[test]
    fn spawn_points_are_numbered_and_renumbered() {
        let mut d = doc(10, 10);
        assert!(d.add_spawn(1, 1) && d.add_spawn(5, 5) && d.add_spawn(7, 2));
        assert!(!d.add_spawn(5, 5), "no duplicates");
        assert!(!d.add_spawn(50, 5), "inside the level only");
        assert!(d.remove_spawn(5, 5));
        assert!(!d.remove_spawn(5, 5));
        let ids: Vec<u32> = d.spawn_points.iter().map(|s| s.id).collect();
        assert_eq!(ids, vec![1, 2]);
        assert_eq!((d.spawn_points[1].col, d.spawn_points[1].row), (7, 2));
        assert!(d.has_minimum_spawn_points());
    }

    #[test]
    fn copying_a_region_clips_it_to_the_level() {
        let mut d = doc(5, 5);
        d.tiles[0] = 1;
        d.tiles[6] = 2;
        let (tiles, w, h) = d.copy_region((-1, -1, 3, 3)).unwrap();
        assert_eq!((w, h), (2, 2));
        assert_eq!(tiles, vec![1, 0, 0, 2]);
        assert!(d.copy_region((9, 9, 2, 2)).is_none());
        assert!(d.region_has_tiles((0, 0, 2, 2)));
        assert!(!d.region_has_tiles((3, 3, 2, 2)));
    }

    #[test]
    fn fill_stays_in_its_own_region_and_does_not_cross_diagonals() {
        // 0 1 0
        // 1 0 0     the empty tile at (1,1) is joined to (2,1), (2,0), (1,2)... but
        // 0 0 0     not diagonally to (0,0)
        let mut d = doc(3, 3);
        d.tiles[1] = 1;
        d.tiles[3] = 1;
        let mut region = d.find_fill_region(1, 1, None);
        region.sort();
        assert_eq!(region, vec![2, 4, 5, 6, 7, 8]);
        assert_eq!(d.find_fill_region(0, 0, None), vec![0]);
        // a limit keeps the fill inside
        let mut limited = d.find_fill_region(1, 1, Some((1, 1, 2, 2)));
        limited.sort();
        assert_eq!(limited, vec![4, 5, 7, 8]);
        assert!(d.find_fill_region(1, 1, Some((0, 0, 1, 1))).is_empty());
        assert!(d.find_fill_region(9, 9, None).is_empty());
    }

    #[test]
    fn resizing_keeps_what_fits_and_drops_points_that_no_longer_do() {
        let mut d = doc(6, 6);
        d.tiles[5 * 6 + 5] = 9;
        d.tiles[1 * 6 + 1] = 4;
        d.add_spawn(1, 1);
        d.add_spawn(5, 5);
        d.add_spawn(2, 2);
        d.delivery_points.push(DeliveryPoint { col: 5, row: 0, id: 1 });
        let dropped = d.resize(4, 4);
        assert_eq!(dropped, 2); // the spawn at (5,5) and the delivery point
        assert_eq!(d.tiles.len(), 16);
        assert_eq!(d.tile_id(1, 1), 4);
        let ids: Vec<u32> = d.spawn_points.iter().map(|s| s.id).collect();
        assert_eq!(ids, vec![1, 2]);
        d.resize(8, 8); // growing adds empty tiles
        assert_eq!(d.tile_id(1, 1), 4);
        assert_eq!(d.tile_id(7, 7), 0);
    }

    #[test]
    fn a_document_survives_a_trip_through_a_level() {
        let mut d = doc(5, 4);
        d.tiles[7] = 3;
        d.add_spawn(0, 0);
        let again = EditorDocument::from_level(&d.to_level());
        assert_eq!(again, d);
    }
}
