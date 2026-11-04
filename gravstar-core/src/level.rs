//! A level: the grid of tiles plus the points players spawn at and deliver
//! cargo to.

use crate::constants::{MIN_ISLAND_AREA, TILE_SIZE};
use crate::terrain_islands::find_small_islands;
use crate::tile_instance::TileInstance;
use crate::tile_registry::TileRegistry;
use serde::Deserialize;

/// Where a ship can appear (the tile's top-left corner is used)
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct SpawnPoint {
    pub col: usize,
    pub row: usize,
    /// Older level files call this "player"
    #[serde(alias = "player")]
    pub id: u32,
}

/// Where cargo is delivered
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct DeliveryPoint {
    pub col: usize,
    pub row: usize,
    pub id: u32,
}

/// One pixel of a piece of terrain that was cut free and deleted
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IslandPixel {
    pub x: i32,
    pub y: i32,
    /// The kind of tile it belonged to (so it can be drawn breaking up)
    pub tile_id: u16,
}

/// What a cut did
#[derive(Debug, Default)]
pub struct CarveResult {
    /// Every tile that changed, as (col, row) - redraw these
    pub changed: Vec<(usize, usize)>,
    /// The small pieces the cut set free; they are already gone from the
    /// terrain (this is so something can show them crumbling)
    pub islands: Vec<Vec<IslandPixel>>,
}

pub struct Level {
    pub name: String,
    pub width: usize,
    pub height: usize,
    /// Row by row: the tile at (col, row) is tiles[row * width + col]
    pub tiles: Vec<TileInstance>,
    pub spawn_points: Vec<SpawnPoint>,
    pub delivery_points: Vec<DeliveryPoint>,
}

impl Level {
    /// A level with nothing in it
    pub fn new(name: &str, width: usize, height: usize) -> Self {
        Level {
            name: name.to_string(),
            width,
            height,
            tiles: (0..width * height).map(|_| TileInstance::new(0)).collect(),
            spawn_points: Vec::new(),
            delivery_points: Vec::new(),
        }
    }

    pub fn in_bounds(&self, col: i32, row: i32) -> bool {
        col >= 0 && row >= 0 && (col as usize) < self.width && (row as usize) < self.height
    }

    /// The tile at (col, row); None outside the level
    pub fn tile(&self, col: i32, row: i32) -> Option<&TileInstance> {
        if self.in_bounds(col, row) {
            Some(&self.tiles[row as usize * self.width + col as usize])
        } else {
            None
        }
    }

    pub fn tile_mut(&mut self, col: i32, row: i32) -> Option<&mut TileInstance> {
        if self.in_bounds(col, row) {
            let index = row as usize * self.width + col as usize;
            Some(&mut self.tiles[index])
        } else {
            None
        }
    }

    /// Replace the tile at (col, row) with a fresh, undamaged one
    pub fn set_tile(&mut self, col: usize, row: usize, tile_id: u16) {
        let index = row * self.width + col;
        self.tiles[index] = TileInstance::new(tile_id);
    }

    /// Cut a circular hole in the terrain, centred on a world position.
    /// Handles a cut that spans several tiles and skips empty and
    /// indestructible tiles. Any piece of terrain the cut leaves floating
    /// free, smaller than MIN_ISLAND_AREA pixels, is deleted too, including
    /// in neighbouring tiles it reaches into.
    pub fn carve_circle(&mut self, registry: &TileRegistry, world_x: f64,
                        world_y: f64, radius: f64) -> CarveResult {
        let ts = TILE_SIZE as f64;
        let first_col = ((world_x - radius) / ts).floor() as i32;
        let last_col = ((world_x + radius) / ts).floor() as i32;
        let first_row = ((world_y - radius) / ts).floor() as i32;
        let last_row = ((world_y + radius) / ts).floor() as i32;

        let mut result = CarveResult::default();
        for row in first_row..=last_row {
            for col in first_col..=last_col {
                let Some(tile) = self.tile_mut(col, row) else { continue };
                if tile.tile_id == 0 {
                    continue;
                }
                match registry.get(tile.tile_id) {
                    Some(def) if def.destructible => {}
                    _ => continue,
                }
                if tile.carve_circle(registry, world_x - col as f64 * ts,
                                     world_y - row as f64 * ts, radius) {
                    result.changed.push((col as usize, row as usize));
                }
            }
        }

        if !result.changed.is_empty() {
            self.remove_small_islands(registry, world_x, world_y, radius, &mut result);
        }
        result
    }

    /// Delete the small pieces a cut just left floating free. Every tile this
    /// changes is added to `result.changed`.
    fn remove_small_islands(&mut self, registry: &TileRegistry, world_x: f64,
                            world_y: f64, radius: f64, result: &mut CarveResult) {
        let islands = find_small_islands(self, registry, world_x, world_y,
                                         radius, MIN_ISLAND_AREA);
        let ts = TILE_SIZE as i32;
        for island in islands {
            // The pixels in each tile, in that tile's own coordinates
            let mut by_tile: Vec<((i32, i32), Vec<(usize, usize)>)> = Vec::new();
            let mut described = Vec::with_capacity(island.len());
            for &(x, y) in &island {
                let (col, row) = (x.div_euclid(ts), y.div_euclid(ts));
                let local = (x.rem_euclid(ts) as usize, y.rem_euclid(ts) as usize);
                match by_tile.iter_mut().find(|(key, _)| *key == (col, row)) {
                    Some((_, pixels)) => pixels.push(local),
                    None => by_tile.push(((col, row), vec![local])),
                }
                let tile_id = self.tile(col, row).map_or(0, |t| t.tile_id);
                described.push(IslandPixel { x, y, tile_id });
            }
            for ((col, row), pixels) in by_tile {
                let changed_it = self.tile_mut(col, row)
                    .map_or(false, |t| t.remove_pixels(registry, &pixels));
                let pos = (col as usize, row as usize);
                if changed_it && !result.changed.contains(&pos) {
                    result.changed.push(pos);
                }
            }
            result.islands.push(described);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_level() -> Level {
        let mut level = Level::new("t", 4, 4);
        for row in 0..4 {
            for col in 0..4 {
                level.set_tile(col, row, 1);
            }
        }
        level
    }

    #[test]
    fn bounds_and_lookup() {
        let level = solid_level();
        assert!(level.tile(0, 0).is_some());
        assert!(level.tile(3, 3).is_some());
        assert!(level.tile(4, 0).is_none());
        assert!(level.tile(-1, 0).is_none());
    }

    #[test]
    fn a_cut_over_a_tile_corner_changes_all_four_tiles() {
        let reg = TileRegistry::from_tileset();
        let mut level = solid_level();
        let changed = level.carve_circle(&reg, 64.0, 64.0, 10.0).changed;
        assert_eq!(changed.len(), 4);
        for pos in [(1, 1), (2, 1), (1, 2), (2, 2)] {
            assert!(changed.contains(&pos), "{pos:?}");
        }
        assert!(!level.tile(0, 0).unwrap().is_damaged());
    }

    #[test]
    fn a_cut_in_empty_space_changes_nothing() {
        let reg = TileRegistry::from_tileset();
        let mut level = Level::new("t", 4, 4);
        assert!(level.carve_circle(&reg, 64.0, 64.0, 30.0).changed.is_empty());
    }

    #[test]
    fn a_cut_hanging_off_the_edge_is_fine() {
        let reg = TileRegistry::from_tileset();
        let mut level = solid_level();
        assert_eq!(level.carve_circle(&reg, -5.0, -5.0, 20.0).changed, vec![(0, 0)]);
    }
}
