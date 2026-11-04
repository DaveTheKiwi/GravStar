//! Minimap maths (pure - the game crate draws it): how solid each tile is,
//! and where a world position lands on the map.

use crate::constants::TILE_SIZE;
use crate::tile_instance::TileInstance;
use crate::tile_registry::TileRegistry;

/// How much of the tile is terrain: 0.0 (empty) to 1.0 (completely solid)
pub fn tile_coverage(tile: &TileInstance, registry: &TileRegistry) -> f64 {
    if tile.tile_id == 0 {
        return 0.0;
    }
    let solid: u32 = tile.mask(registry).iter().map(|b| b.count_ones()).sum();
    (solid as f64 / (TILE_SIZE * TILE_SIZE) as f64).min(1.0)
}

/// World pixels -> a position inside a map of map_width x map_height pixels
/// (held to the map's edges)
pub fn world_to_minimap(x: f64, y: f64, world_width: f64, world_height: f64,
                        map_width: f64, map_height: f64) -> (f64, f64) {
    ((x / world_width * map_width).clamp(0.0, map_width),
     (y / world_height * map_height).clamp(0.0, map_height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_of_empty_full_half_and_damaged_tiles() {
        let reg = TileRegistry::from_tileset();
        assert_eq!(tile_coverage(&TileInstance::new(0), &reg), 0.0);
        assert_eq!(tile_coverage(&TileInstance::new(1), &reg), 1.0);
        assert_eq!(tile_coverage(&TileInstance::new(2), &reg), 0.5);
        let mut t = TileInstance::new(1);
        t.carve_circle(&reg, 16.0, 16.0, 8.0);
        let c = tile_coverage(&t, &reg);
        assert!(c < 0.85 && c > 0.7, "{c}");
    }

    #[test]
    fn positions_scale_and_clamp() {
        assert_eq!(world_to_minimap(3200.0, 2400.0, 6400.0, 4800.0, 200.0, 150.0), (100.0, 75.0));
        assert_eq!(world_to_minimap(-10.0, 99999.0, 6400.0, 4800.0, 200.0, 150.0), (0.0, 150.0));
    }
}
