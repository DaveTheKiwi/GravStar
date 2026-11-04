//! A ship's collision shape at every angle, baked once at startup: the
//! sprite is scaled to the ship's size and turned in equal steps, and the
//! solid pixels of each turned picture become a mask.

use crate::constants::MASK_ALPHA_SOLID_ABOVE;
use crate::raster::AlphaCanvas;

/// One baked angle
pub struct ShipMask {
    /// One bit per pixel, row by row (bit index = y * width + x, lowest bit
    /// of each byte first), set where the ship is solid
    pub bits: Vec<u8>,
    pub width: usize,
    pub height: usize,
    /// Where this picture's top-left corner is, relative to the top-left
    /// corner of the unturned ship (the turned picture is centred on the
    /// same point)
    pub offset_x: i32,
    pub offset_y: i32,
}

impl ShipMask {
    /// Is the pixel at (x, y) in this mask's own coordinates solid?
    pub fn is_solid(&self, x: i32, y: i32) -> bool {
        if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
            return false;
        }
        let bit = y as usize * self.width + x as usize;
        self.bits[bit >> 3] & (1 << (bit & 7)) != 0
    }
}

pub struct ShipMaskSet {
    masks: Vec<ShipMask>,
}

impl ShipMaskSet {
    /// Bake the masks for a sprite (RGBA pixels, `src_width` x `src_height`)
    /// drawn at `width` x `height` pixels, with `steps` angles around a full
    /// turn. Mask `n` is the ship turned clockwise by n * 360 / steps degrees.
    pub fn bake(rgba: &[u8], src_width: usize, src_height: usize,
                width: usize, height: usize, steps: usize) -> ShipMaskSet {
        // Scale to the ship's size FIRST, then turn that
        let scaled = AlphaCanvas::from_rgba(src_width, src_height, rgba)
            .scale_to(width, height);
        let masks = (0..steps).map(|n| {
            let angle = std::f64::consts::TAU * n as f64 / steps as f64;
            let turned = scaled.rotate(angle);
            let mut bits = vec![0u8; (turned.width() * turned.height() + 7) / 8];
            for (bit, a) in turned.alpha().iter().enumerate() {
                if *a > MASK_ALPHA_SOLID_ABOVE {
                    bits[bit >> 3] |= 1 << (bit & 7);
                }
            }
            ShipMask {
                bits,
                width: turned.width(),
                height: turned.height(),
                offset_x: (turned.width() as i32 - width as i32).div_euclid(2),
                offset_y: (turned.height() as i32 - height as i32).div_euclid(2),
            }
        }).collect();
        ShipMaskSet { masks }
    }

    pub fn steps(&self) -> usize {
        self.masks.len()
    }

    /// Radians between one baked angle and the next
    pub fn step_radians(&self) -> f64 {
        std::f64::consts::TAU / self.masks.len() as f64
    }

    /// Which baked angle is nearest to `angle` (radians, any value)
    pub fn index_for(&self, angle: f64) -> usize {
        let n = self.masks.len() as i64;
        ((angle / self.step_radians()).round() as i64).rem_euclid(n) as usize
    }

    /// The mask for the baked angle nearest to `angle` (radians)
    pub fn mask_for(&self, angle: f64) -> &ShipMask {
        &self.masks[self.index_for(angle)]
    }

    pub fn mask(&self, index: usize) -> &ShipMask {
        &self.masks[index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 40 x 40 sprite shaped like an arrow pointing up: a triangle whose
    /// tip is the top middle
    fn arrow() -> Vec<u8> {
        let mut rgba = vec![0u8; 40 * 40 * 4];
        for y in 0..40 {
            for x in 0..40 {
                let half = (y as f64 + 1.0) / 40.0 * 20.0;
                if (x as f64 + 0.5 - 20.0).abs() <= half {
                    rgba[(y * 40 + x) * 4 + 3] = 255;
                }
            }
        }
        rgba
    }

    fn solid_count(m: &ShipMask) -> u32 {
        m.bits.iter().map(|b| b.count_ones()).sum()
    }

    #[test]
    fn unturned_mask_is_the_scaled_sprite() {
        let set = ShipMaskSet::bake(&arrow(), 40, 40, 48, 32, 180);
        let m = set.mask(0);
        assert_eq!((m.width, m.height, m.offset_x, m.offset_y), (48, 32, 0, 0));
        assert!(m.is_solid(24, 31)); // wide base
        assert!(m.is_solid(1, 31));
        assert!(!m.is_solid(1, 2)); // empty top corner
        assert!(m.is_solid(24, 4)); // the tip is at the top
    }

    #[test]
    fn a_quarter_turn_clockwise_points_the_nose_right() {
        let set = ShipMaskSet::bake(&arrow(), 40, 40, 48, 32, 180);
        let m = set.mask(45); // 90 degrees
        assert_eq!((m.width, m.height), (32, 48));
        assert_eq!((m.offset_x, m.offset_y), (-8, 8));
        assert!(m.is_solid(28, 24)); // nose on the right
        assert!(!m.is_solid(28, 1)); // and nothing at the top right corner
        assert!(m.is_solid(2, 24)); // base on the left
    }

    #[test]
    fn half_a_turn_points_the_nose_down() {
        let set = ShipMaskSet::bake(&arrow(), 40, 40, 48, 32, 180);
        let m = set.mask(90);
        assert_eq!((m.width, m.height), (48, 32));
        assert!(m.is_solid(24, 27)); // tip at the bottom
        assert!(m.is_solid(24, 0));  // base along the top
    }

    #[test]
    fn the_area_hardly_changes_as_the_ship_turns() {
        let set = ShipMaskSet::bake(&arrow(), 40, 40, 48, 32, 180);
        let base = solid_count(set.mask(0)) as f64;
        for n in 0..180 {
            let c = solid_count(set.mask(n)) as f64;
            assert!((c - base).abs() < base * 0.04, "step {n}: {c} vs {base}");
        }
    }

    #[test]
    fn angle_lookup_picks_the_nearest_step_and_wraps() {
        let set = ShipMaskSet::bake(&arrow(), 40, 40, 48, 32, 180);
        let step = set.step_radians();
        assert_eq!(set.index_for(0.0), 0);
        assert_eq!(set.index_for(step * 0.4), 0);
        assert_eq!(set.index_for(step * 0.6), 1);
        assert_eq!(set.index_for(std::f64::consts::TAU - step * 0.4), 0);
        assert_eq!(set.index_for(-step), 179);
        assert_eq!(set.index_for(std::f64::consts::TAU * 3.0 + step), 1);
    }

    #[test]
    fn scaling_averages_and_rotation_keeps_whole_sprites_whole() {
        let solid = AlphaCanvas::from_rgba(2, 2, &[0, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255]);
        assert_eq!(solid.scale_to(1, 1).alpha()[0], 128);
        let block = AlphaCanvas::from_rgba(4, 2, &[0, 0, 0, 255].repeat(8));
        let turned = block.rotate(std::f64::consts::FRAC_PI_2);
        assert_eq!((turned.width(), turned.height()), (2, 4));
        assert!(turned.alpha().iter().all(|a| *a == 255));
    }
}
