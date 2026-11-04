//! The tile set: every tile's id, name, group and shape. To add a tile,
//! add a line to TILE_DEFS - its picture and mask are generated from the
//! shape (see tile_art).

use crate::constants::DEFAULT_TILE_HARDNESS;
use crate::tile_art::Shape;

pub struct TileDef {
    pub id: u16,
    pub name: &'static str,
    /// Text shown when hovering over the tile in the editor
    pub tooltip: &'static str,
    /// Tiles in the same group rotate into each other in the editor
    pub group: &'static str,
    /// Position within the group's rotation (0 = as drawn, then clockwise)
    pub rotation_order: u8,
    /// How hard to destroy, 0.0 - 1.0
    pub hardness: f32,
    pub shape: Shape,
}

const fn def(id: u16, name: &'static str, tooltip: &'static str,
             group: &'static str, rotation_order: u8, shape: Shape) -> TileDef {
    TileDef { id, name, tooltip, group, rotation_order,
              hardness: DEFAULT_TILE_HARDNESS, shape }
}

/// Look a tile up by id (0 means "no tile" and is never defined)
pub fn find(id: u16) -> Option<&'static TileDef> {
    TILE_DEFS.iter().find(|d| d.id == id)
}

pub static TILE_DEFS: &[TileDef] = &[
    def(1, "full_solid", "Full Solid Block", "solid", 0,
        Shape::Solid),
    def(2, "half_top", "Half Tile", "half", 0,
        Shape::Rect { x: 0, y: 0, w: 2, h: 1 }),
    def(3, "half_bottom", "Half Tile", "half", 2,
        Shape::Rect { x: 0, y: 1, w: 2, h: 1 }),
    def(4, "half_left", "Half Tile", "half", 3,
        Shape::Rect { x: 0, y: 0, w: 1, h: 2 }),
    def(5, "half_right", "Half Tile", "half", 1,
        Shape::Rect { x: 1, y: 0, w: 1, h: 2 }),
    def(6, "corner_top_left", "Corner Triangle", "corner", 0,
        Shape::Polygon(&[(0,0),(2,0),(0,2)])),
    def(7, "corner_top_right", "Corner Triangle", "corner", 1,
        Shape::Polygon(&[(0,0),(2,0),(2,2)])),
    def(8, "corner_bottom_left", "Corner Triangle", "corner", 3,
        Shape::Polygon(&[(0,0),(0,2),(2,2)])),
    def(9, "corner_bottom_right", "Corner Triangle", "corner", 2,
        Shape::Polygon(&[(2,0),(0,2),(2,2)])),
    def(10, "slope_shallow_bottom_right", "Slope Mirrored", "slope_mirrored", 0,
        Shape::Polygon(&[(0,1),(2,0),(2,2),(0,2)])),
    def(11, "slope_shallow_bottom_left", "Slope", "slope", 0,
        Shape::Polygon(&[(0,0),(2,1),(2,2),(0,2)])),
    def(12, "slope_shallow_top_left", "Slope Mirrored", "slope_mirrored", 2,
        Shape::Polygon(&[(0,0),(2,0),(2,1),(0,2)])),
    def(13, "slope_shallow_top_right", "Slope", "slope", 2,
        Shape::Polygon(&[(0,0),(2,0),(2,2),(0,1)])),
    def(14, "slope_steep_bottom_left", "Slope Mirrored", "slope_mirrored", 1,
        Shape::Polygon(&[(0,0),(1,0),(2,2),(0,2)])),
    def(15, "slope_steep_bottom_right", "Slope", "slope", 3,
        Shape::Polygon(&[(1,0),(2,0),(2,2),(0,2)])),
    def(16, "slope_steep_top_left", "Slope", "slope", 1,
        Shape::Polygon(&[(0,0),(2,0),(1,2),(0,2)])),
    def(17, "slope_steep_top_right", "Slope Mirrored", "slope_mirrored", 3,
        Shape::Polygon(&[(0,0),(2,0),(2,2),(1,2)])),
    def(18, "concave_top_left", "Concave Corner", "concave_large", 0,
        Shape::SolidMinusDisc { cx: 0, cy: 0, r: 2 }),
    def(19, "concave_top_right", "Concave Corner", "concave_large", 1,
        Shape::SolidMinusDisc { cx: 2, cy: 0, r: 2 }),
    def(20, "concave_bottom_left", "Concave Corner", "concave_large", 3,
        Shape::SolidMinusDisc { cx: 0, cy: 2, r: 2 }),
    def(21, "concave_bottom_right", "Concave Corner", "concave_large", 2,
        Shape::SolidMinusDisc { cx: 2, cy: 2, r: 2 }),
    def(22, "concave_top_left_small", "Concave Corner Small", "concave_small", 0,
        Shape::SolidMinusDisc { cx: 0, cy: 0, r: 1 }),
    def(23, "concave_top_right_small", "Concave Corner Small", "concave_small", 1,
        Shape::SolidMinusDisc { cx: 2, cy: 0, r: 1 }),
    def(24, "concave_bottom_left_small", "Concave Corner Small", "concave_small", 3,
        Shape::SolidMinusDisc { cx: 0, cy: 2, r: 1 }),
    def(25, "concave_bottom_right_small", "Concave Corner Small", "concave_small", 2,
        Shape::SolidMinusDisc { cx: 2, cy: 2, r: 1 }),
    def(26, "convex_top_left", "Convex Corner", "convex_large", 0,
        Shape::Disc { cx: 0, cy: 0, r: 2 }),
    def(27, "convex_top_right", "Convex Corner", "convex_large", 1,
        Shape::Disc { cx: 2, cy: 0, r: 2 }),
    def(28, "convex_bottom_left", "Convex Corner", "convex_large", 3,
        Shape::Disc { cx: 0, cy: 2, r: 2 }),
    def(29, "convex_bottom_right", "Convex Corner", "convex_large", 2,
        Shape::Disc { cx: 2, cy: 2, r: 2 }),
    def(30, "convex_top_left_small", "Convex Corner Small", "convex_small", 0,
        Shape::Disc { cx: 0, cy: 0, r: 1 }),
    def(31, "convex_top_right_small", "Convex Corner Small", "convex_small", 1,
        Shape::Disc { cx: 2, cy: 0, r: 1 }),
    def(32, "convex_bottom_left_small", "Convex Corner Small", "convex_small", 3,
        Shape::Disc { cx: 0, cy: 2, r: 1 }),
    def(33, "convex_bottom_right_small", "Convex Corner Small", "convex_small", 2,
        Shape::Disc { cx: 2, cy: 2, r: 1 }),
    def(34, "small_corner_top_left", "Small Corner", "small_corner", 0,
        Shape::Polygon(&[(0,0),(1,0),(0,1)])),
    def(35, "small_corner_top_right", "Small Corner", "small_corner", 1,
        Shape::Polygon(&[(2,0),(2,1),(1,0)])),
    def(36, "small_corner_bottom_right", "Small Corner", "small_corner", 2,
        Shape::Polygon(&[(2,2),(1,2),(2,1)])),
    def(37, "small_corner_bottom_left", "Small Corner", "small_corner", 3,
        Shape::Polygon(&[(0,2),(0,1),(1,2)])),
    def(38, "slope_half_shallow_top_right", "New Slope", "half_slope", 2,
        Shape::Polygon(&[(0,0),(2,0),(2,1)])),
    def(39, "slope_half_steep_bottom_right", "New Slope", "half_slope", 3,
        Shape::Polygon(&[(2,0),(2,2),(1,2)])),
    def(40, "slope_half_shallow_bottom_left", "New Slope", "half_slope", 0,
        Shape::Polygon(&[(0,1),(0,2),(2,2)])),
    def(41, "slope_half_steep_top_left", "New Slope", "half_slope", 1,
        Shape::Polygon(&[(0,0),(1,0),(0,2)])),
    def(42, "slope_half_shallow_top_left", "New Slope Mirrored", "half_slope_mirrored", 2,
        Shape::Polygon(&[(0,0),(2,0),(0,1)])),
    def(43, "slope_half_steep_top_right", "New Slope Mirrored", "half_slope_mirrored", 3,
        Shape::Polygon(&[(1,0),(2,0),(2,2)])),
    def(44, "slope_half_shallow_bottom_right", "New Slope Mirrored", "half_slope_mirrored", 0,
        Shape::Polygon(&[(2,1),(0,2),(2,2)])),
    def(45, "slope_half_steep_bottom_left", "New Slope Mirrored", "half_slope_mirrored", 1,
        Shape::Polygon(&[(0,0),(1,2),(0,2)])),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_run_from_1_without_gaps_or_repeats() {
        for (i, d) in TILE_DEFS.iter().enumerate() {
            assert_eq!(d.id as usize, i + 1);
        }
        assert_eq!(TILE_DEFS.len(), 45);
    }

    #[test]
    fn every_group_has_distinct_rotation_orders() {
        for d in TILE_DEFS {
            let same: Vec<_> = TILE_DEFS.iter()
                .filter(|o| o.group == d.group && o.rotation_order == d.rotation_order)
                .collect();
            // slope / slope_mirrored etc. are separate groups, so each
            // (group, order) pair must be unique
            assert_eq!(same.len(), 1, "{} / {}", d.group, d.rotation_order);
        }
    }

    #[test]
    fn tiles_in_a_rotation_group_are_rotations_of_each_other() {
        // Same group = same shape turned 90 degrees, so identical pixel
        // counts (and the mask of one is the other turned)
        let count = |d: &TileDef| -> u32 {
            crate::tile_art::render(&d.shape).mask.iter().map(|b| b.count_ones()).sum()
        };
        for d in TILE_DEFS {
            let first = TILE_DEFS.iter().find(|o| o.group == d.group).unwrap();
            assert_eq!(count(d), count(first), "{} vs {}", d.name, first.name);
        }
    }

    #[test]
    fn every_tile_has_some_solid_pixels() {
        for d in TILE_DEFS {
            let art = crate::tile_art::render(&d.shape);
            assert!(art.mask.iter().any(|b| *b != 0), "tile {} is empty", d.id);
        }
    }

    #[test]
    fn find_works() {
        assert_eq!(find(1).unwrap().name, "full_solid");
        assert!(find(0).is_none());
        assert!(find(46).is_none());
    }
}
