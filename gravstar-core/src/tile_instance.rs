//! One tile placed in a level. It shares its kind's mask until damage
//! changes it (copy-on-write), so a level full of untouched tiles costs
//! almost nothing.

use crate::constants::TILE_SIZE;
use crate::tile_art::TileMask;
use crate::tile_registry::TileRegistry;

pub struct TileInstance {
    pub tile_id: u16,
    /// Our own mask, once damage has changed it
    damaged: Option<Box<TileMask>>,
    /// Goes up every time the mask changes, so a renderer knows when a
    /// picture it made of this tile is out of date
    damage_version: u32,
}

impl TileInstance {
    pub fn new(tile_id: u16) -> Self {
        TileInstance { tile_id, damaged: None, damage_version: 0 }
    }

    /// The tile's current mask: its own if damaged, else the shared one
    pub fn mask<'a>(&'a self, registry: &'a TileRegistry) -> &'a TileMask {
        match &self.damaged {
            Some(mask) => mask,
            None => registry.mask(self.tile_id),
        }
    }

    pub fn is_damaged(&self) -> bool {
        self.damaged.is_some()
    }

    pub fn damage_version(&self) -> u32 {
        self.damage_version
    }

    pub fn is_empty(&self, registry: &TileRegistry) -> bool {
        registry.get(self.tile_id).map_or(true, |d| d.is_empty)
    }

    /// Remove every solid pixel whose middle is within `radius` of (cx, cy),
    /// given in this tile's own pixel coordinates. The centre may be outside
    /// the tile (a cut spilling over from a neighbour). Returns true if any
    /// solid pixel was removed.
    pub fn carve_circle(&mut self, registry: &TileRegistry, cx: f64, cy: f64,
                        radius: f64) -> bool {
        let size = TILE_SIZE as i32;
        let r2 = radius * radius;
        let min_x = ((cx - radius).floor() as i32).max(0);
        let max_x = ((cx + radius).ceil() as i32).min(size - 1);
        let min_y = ((cy - radius).floor() as i32).max(0);
        let max_y = ((cy + radius).ceil() as i32).min(size - 1);

        let current = self.mask(registry);
        let mut to_clear = Vec::new();
        for y in min_y..=max_y {
            let dy = y as f64 + 0.5 - cy;
            for x in min_x..=max_x {
                let dx = x as f64 + 0.5 - cx;
                if dx * dx + dy * dy <= r2 {
                    let bit = (y * size + x) as usize;
                    if current[bit >> 3] & (1 << (bit & 7)) != 0 {
                        to_clear.push(bit);
                    }
                }
            }
        }
        self.clear_bits(registry, &to_clear)
    }

    /// Remove the given solid pixels, each (x, y) in this tile's own
    /// coordinates. Returns true if any solid pixel was removed.
    pub fn remove_pixels(&mut self, registry: &TileRegistry,
                         pixels: &[(usize, usize)]) -> bool {
        let current = self.mask(registry);
        let to_clear: Vec<usize> = pixels.iter()
            .map(|&(x, y)| y * TILE_SIZE + x)
            .filter(|&bit| current[bit >> 3] & (1 << (bit & 7)) != 0)
            .collect();
        self.clear_bits(registry, &to_clear)
    }

    /// Clear these mask bits (copying the shared mask first if needed)
    fn clear_bits(&mut self, registry: &TileRegistry, bits: &[usize]) -> bool {
        if bits.is_empty() {
            return false;
        }
        let id = self.tile_id;
        let mask = self.damaged.get_or_insert_with(|| Box::new(*registry.mask(id)));
        for &bit in bits {
            mask[bit >> 3] &= !(1 << (bit & 7));
        }
        self.damage_version += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(mask: &TileMask) -> u32 {
        mask.iter().map(|b| b.count_ones()).sum()
    }

    #[test]
    fn untouched_tile_shares_the_kind_mask() {
        let reg = TileRegistry::from_tileset();
        let tile = TileInstance::new(1);
        assert!(!tile.is_damaged());
        assert!(std::ptr::eq(tile.mask(&reg), reg.mask(1)));
    }

    #[test]
    fn carving_copies_the_mask_and_leaves_the_shared_one_alone() {
        let reg = TileRegistry::from_tileset();
        let mut tile = TileInstance::new(1);
        assert!(tile.carve_circle(&reg, 16.0, 16.0, 8.0));
        assert!(tile.is_damaged());
        assert_eq!(tile.damage_version(), 1);
        assert_eq!(count(reg.mask(1)), 1024);
        let left = count(tile.mask(&reg));
        assert!(left < 1024 - 150 && left > 1024 - 250, "{left}");
        // the pixel under the centre is gone, a far corner is not
        assert_eq!(tile.mask(&reg)[(16 * 32 + 16) >> 3] & (1 << ((16 * 32 + 16) & 7)), 0);
        assert_ne!(tile.mask(&reg)[0] & 1, 0);
    }

    #[test]
    fn a_cut_that_removes_nothing_changes_nothing() {
        let reg = TileRegistry::from_tileset();
        let mut full = TileInstance::new(1);
        assert!(!full.carve_circle(&reg, -50.0, -50.0, 5.0));
        assert!(!full.is_damaged());
        let mut empty = TileInstance::new(0);
        assert!(!empty.carve_circle(&reg, 16.0, 16.0, 20.0));
        assert!(!empty.is_damaged());
        assert_eq!(full.damage_version(), 0);
    }

    #[test]
    fn a_cut_spilling_in_from_outside_works() {
        let reg = TileRegistry::from_tileset();
        let mut tile = TileInstance::new(1);
        assert!(tile.carve_circle(&reg, -2.0, 16.0, 6.0));
        assert_eq!(tile.mask(&reg)[(16 * 32) >> 3] & 1, 0);
    }

    #[test]
    fn remove_pixels_only_counts_pixels_that_were_solid() {
        let reg = TileRegistry::from_tileset();
        let mut tile = TileInstance::new(2); // top half only
        assert!(!tile.remove_pixels(&reg, &[(5, 30)])); // bottom: already empty
        assert!(tile.remove_pixels(&reg, &[(5, 3), (5, 30)]));
        assert_eq!(count(tile.mask(&reg)), 512 - 1);
    }
}
