//! Turns a tile's shape description into its picture (alpha values) and its
//! collision mask. Pure logic: the game crate decides how to draw the alpha.

use crate::constants::*;
use crate::raster::AlphaCanvas;

/// A tile's shape, on the half-tile grid (see TILE_ART_GRID): coordinates
/// run 0 (left / top), 1 (middle), 2 (right / bottom).
pub enum Shape {
    /// The whole tile
    Solid,
    /// A rectangle: x, y, width, height
    Rect { x: i32, y: i32, w: i32, h: i32 },
    /// A filled polygon through these points
    Polygon(&'static [(i32, i32)]),
    /// A filled circle (only the part inside the tile shows)
    Disc { cx: i32, cy: i32, r: i32 },
    /// The whole tile with a circle cut out of it
    SolidMinusDisc { cx: i32, cy: i32, r: i32 },
}

/// A collision mask: one bit per pixel (bit index = y * TILE_SIZE + x, lowest
/// bit of each byte first), set where the tile is solid
pub type TileMask = [u8; TILE_MASK_BYTES];

/// The mask of a tile with nothing in it
pub const EMPTY_MASK: TileMask = [0; TILE_MASK_BYTES];

/// A finished tile
pub struct TileArt {
    /// TILE_SIZE x TILE_SIZE alpha values, row by row
    pub alpha: Vec<u8>,
    pub mask: TileMask,
}

/// Grid units -> pixels on the enlarged canvas
fn px(units: i32) -> i32 {
    units * (TILE_SIZE * TILE_SUPERSAMPLE) as i32 / TILE_ART_GRID
}

pub fn render(shape: &Shape) -> TileArt {
    let big = TILE_SIZE * TILE_SUPERSAMPLE;
    let mut canvas = AlphaCanvas::new(big, big);

    match shape {
        Shape::Solid => canvas.fill(255),
        Shape::Rect { x, y, w, h } => {
            canvas.fill_rect(px(*x), px(*y), px(*w), px(*h), 255);
        }
        Shape::Polygon(points) => {
            let scaled: Vec<(i32, i32)> = points.iter().map(|p| (px(p.0), px(p.1))).collect();
            canvas.fill_polygon(&scaled, 255);
        }
        Shape::Disc { cx, cy, r } => {
            canvas.fill_circle(px(*cx), px(*cy), px(*r), 255);
        }
        Shape::SolidMinusDisc { cx, cy, r } => {
            canvas.fill(255);
            canvas.fill_circle(px(*cx), px(*cy), px(*r), 0);
        }
    }

    let alpha = canvas.downsample(TILE_SUPERSAMPLE).alpha().to_vec();
    let mask = mask_from_alpha(&alpha);
    TileArt { alpha, mask }
}

/// The collision mask for a TILE_SIZE x TILE_SIZE picture
pub fn mask_from_alpha(alpha: &[u8]) -> TileMask {
    let mut mask = EMPTY_MASK;
    for (bit, a) in alpha.iter().enumerate() {
        if *a > MASK_ALPHA_SOLID_ABOVE {
            mask[bit / 8] |= 1 << (bit % 8);
        }
    }
    mask
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_pixels(mask: &[u8]) -> u32 {
        mask.iter().map(|b| b.count_ones()).sum()
    }

    fn is_solid(art: &TileArt, x: usize, y: usize) -> bool {
        let bit = y * TILE_SIZE + x;
        art.mask[bit / 8] & (1 << (bit % 8)) != 0
    }

    #[test]
    fn full_tile_is_all_solid() {
        let art = render(&Shape::Solid);
        assert_eq!(art.mask.len(), 128);
        assert_eq!(solid_pixels(&art.mask), (TILE_SIZE * TILE_SIZE) as u32);
    }

    #[test]
    fn top_half_covers_exactly_the_top_half() {
        let art = render(&Shape::Rect { x: 0, y: 0, w: 2, h: 1 });
        for y in 0..TILE_SIZE {
            for x in 0..TILE_SIZE {
                assert_eq!(is_solid(&art, x, y), y < TILE_SIZE / 2, "({x},{y})");
            }
        }
    }

    #[test]
    fn corner_triangle_is_about_half_a_tile() {
        let art = render(&Shape::Polygon(&[(0, 0), (2, 0), (0, 2)]));
        let n = solid_pixels(&art.mask);
        assert!(n > 480 && n < 560, "{n}");
        assert!(is_solid(&art, 0, 0));
        assert!(!is_solid(&art, TILE_SIZE - 1, TILE_SIZE - 1));
    }

    #[test]
    fn concave_and_convex_corners_are_roughly_complementary() {
        let concave = render(&Shape::SolidMinusDisc { cx: 0, cy: 0, r: 2 });
        let convex = render(&Shape::Disc { cx: 0, cy: 0, r: 2 });
        let total = solid_pixels(&concave.mask) + solid_pixels(&convex.mask);
        let tile = (TILE_SIZE * TILE_SIZE) as u32;
        assert!(total > tile - 40 && total < tile + 40, "{total}");
    }
}
