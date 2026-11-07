//! Debris: terrain pieces crumbling and raining down, and chips thrown off
//! wherever a bullet cuts terrain. Pure logic - the game crate draws
//! `islands` (pixels still hanging in place) and `particles` (falling).
//! Purely visual: nothing here touches the physics, and falling pixels pass
//! straight through everything.
//!
//! A cut-free piece lets go of its pixels bottom edge first: every pixel
//! with nothing directly beneath it goes (in random order), then the pieces
//! left have a new bottom edge, which goes next, and so on up. The layers
//! are worked out once, when the piece is added, so crumbling costs nothing
//! but picking a random pixel from the current layer.

use std::collections::HashSet;

use crate::collision::point_vs_terrain;
use crate::constants::*;
use crate::level::{IslandPixel, Level};
use crate::random::Rng;
use crate::tile_registry::TileRegistry;

/// A pixel of terrain that has broken off and is falling and fading
#[derive(Clone, Copy, Debug)]
pub struct FallingPixel {
    pub x: f32,
    pub y: f32,
    pub vel_x: f32,
    pub vel_y: f32,
    pub age: f32,
    /// Seconds until it has faded away
    pub life: f32,
    /// The kind of tile it came from (so it can be drawn in its colour)
    pub tile_id: u16,
}

impl FallingPixel {
    /// 1.0 when it breaks off, fading to 0.0 as it falls
    pub fn alpha(&self) -> f32 {
        (1.0 - self.age / self.life).max(0.0)
    }
}

/// A small piece of terrain that has been cut free. It is no longer part of
/// the level (the physics has already forgotten it) but is still drawn where
/// it was, then lets go of its pixels one at a time.
pub struct CrumblingIsland {
    /// Seconds left before it starts to go
    delay: f64,
    /// Fractional pixels owed
    budget: f64,
    /// The pixels still in place, grouped by the pass they will go in:
    /// layers[0] is the bottom edge, and so on up
    layers: Vec<Vec<IslandPixel>>,
    /// The layer being released now
    current: usize,
}

impl CrumblingIsland {
    fn new(pixels: &[IslandPixel], delay: f64) -> Self {
        let mut left: HashSet<(i32, i32)> = pixels.iter().map(|p| (p.x, p.y)).collect();
        let mut remaining: Vec<IslandPixel> = pixels.to_vec();
        let mut layers = Vec::new();
        while !remaining.is_empty() {
            // The current bottom edge: nothing directly beneath
            let (free, rest): (Vec<IslandPixel>, Vec<IslandPixel>) = remaining
                .into_iter()
                .partition(|p| !left.contains(&(p.x, p.y + 1)));
            for p in &free {
                left.remove(&(p.x, p.y));
            }
            layers.push(free);
            remaining = rest;
        }
        CrumblingIsland { delay, budget: 0.0, layers, current: 0 }
    }

    pub fn finished(&self) -> bool {
        self.current >= self.layers.len()
    }

    /// The passes the piece will crumble in, bottom edge first (including
    /// the ones already done, which are empty by now)
    pub fn layers(&self) -> &[Vec<IslandPixel>] {
        &self.layers
    }

    /// The pixels still in place (for drawing)
    pub fn pending(&self) -> impl Iterator<Item = &IslandPixel> {
        self.layers[self.current.min(self.layers.len())..].iter().flatten()
    }

    /// Let go of one random pixel of the current bottom edge
    fn release_one(&mut self, rng: &mut Rng) -> IslandPixel {
        let layer = &mut self.layers[self.current];
        let pixel = layer.swap_remove(rng.below(layer.len()));
        if layer.is_empty() {
            self.current += 1; // that edge is all gone: on to the next
        }
        pixel
    }
}

pub struct DebrisSystem {
    rng: Rng,
    /// Pieces still crumbling
    pub islands: Vec<CrumblingIsland>,
    /// Pixels falling
    pub particles: Vec<FallingPixel>,
}

impl DebrisSystem {
    pub fn new(seed: u64) -> Self {
        DebrisSystem { rng: Rng::new(seed ^ DEBRIS_SEED_SALT), islands: Vec::new(),
                       particles: Vec::new() }
    }

    fn between(&mut self, lo: f64, hi: f64) -> f64 {
        lo + self.rng.unit() * (hi - lo)
    }

    /// A piece of terrain has been cut free (and is already gone from the level)
    pub fn add_island(&mut self, pixels: &[IslandPixel]) {
        if pixels.is_empty() || self.islands.len() >= MAX_DEBRIS_ISLANDS {
            return;
        }
        self.islands.push(CrumblingIsland::new(pixels, DEBRIS_RELEASE_DELAY));
    }

    /// A bullet has just cut a hole of `radius` centred on (x, y). Throw a
    /// few chips off the rim: from random places on the rim, but only where
    /// terrain really surrounds the hole (just outside is solid), and
    /// pointing away from the centre.
    pub fn spray_impact(&mut self, level: &Level, registry: &TileRegistry, x: f64, y: f64,
                        radius: f64) {
        let ts = TILE_SIZE as i32;
        let mut thrown = 0;
        for _ in 0..DEBRIS_IMPACT_PIXELS * 3 {
            if thrown >= DEBRIS_IMPACT_PIXELS || self.particles.len() >= MAX_DEBRIS_PARTICLES {
                break;
            }
            let angle = self.rng.unit() * std::f64::consts::TAU;
            let (dx, dy) = (angle.cos(), angle.sin());
            let (out_x, out_y) = (x + dx * (radius + 1.5), y + dy * (radius + 1.5));
            if !point_vs_terrain(level, registry, out_x, out_y) {
                continue;
            }
            let tile_id = level
                .tile((out_x.floor() as i32).div_euclid(ts), (out_y.floor() as i32).div_euclid(ts))
                .map_or(1, |t| t.tile_id);
            // Where terrain has just been cut away, near the rim
            let from = radius * self.between(0.55, 1.0);
            let speed = DEBRIS_IMPACT_SPEED * self.between(0.3, 1.0);
            let life = self.between(DEBRIS_LIFE_MIN, DEBRIS_LIFE_MAX);
            let sideways = self.between(-DEBRIS_SPREAD_X, DEBRIS_SPREAD_X);
            let fall = self.between(0.0, DEBRIS_START_FALL);
            self.particles.push(FallingPixel {
                x: (x + dx * from) as f32,
                y: (y + dy * from) as f32,
                vel_x: (dx * speed + sideways) as f32,
                vel_y: (dy * speed + fall) as f32,
                age: 0.0,
                life: life as f32,
                tile_id,
            });
            thrown += 1;
        }
    }

    pub fn update(&mut self, dt: f64) {
        self.release(dt);
        self.move_particles(dt);
    }

    fn release(&mut self, dt: f64) {
        let mut islands = std::mem::take(&mut self.islands);
        for island in islands.iter_mut() {
            let dt_left = if island.delay > 0.0 {
                island.delay -= dt;
                if island.delay > 0.0 {
                    continue;
                }
                let over = -island.delay; // time past the delay
                island.delay = 0.0;
                over
            } else {
                dt
            };
            island.budget += DEBRIS_RELEASE_RATE * dt_left;
            while island.budget >= 1.0 && !island.finished() {
                if self.particles.len() >= MAX_DEBRIS_PARTICLES {
                    island.budget = 0.0;
                    break;
                }
                island.budget -= 1.0;
                let pixel = island.release_one(&mut self.rng);
                let vel_x = self.between(-DEBRIS_SPREAD_X, DEBRIS_SPREAD_X);
                let vel_y = self.between(0.0, DEBRIS_START_FALL);
                let life = self.between(DEBRIS_LIFE_MIN, DEBRIS_LIFE_MAX);
                self.particles.push(FallingPixel {
                    x: pixel.x as f32,
                    y: pixel.y as f32,
                    vel_x: vel_x as f32,
                    vel_y: vel_y as f32,
                    age: 0.0,
                    life: life as f32,
                    tile_id: pixel.tile_id,
                });
            }
        }
        islands.retain(|i| !i.finished());
        self.islands = islands;
    }

    fn move_particles(&mut self, dt: f64) {
        let (dt, gravity) = (dt as f32, DEBRIS_GRAVITY as f32);
        let mut i = 0;
        while i < self.particles.len() {
            let p = &mut self.particles[i];
            p.vel_y += gravity * dt;
            p.x += p.vel_x * dt;
            p.y += p.vel_y * dt;
            p.age += dt;
            if p.age >= p.life {
                self.particles.swap_remove(i);
            } else {
                i += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(w: i32, h: i32) -> Vec<IslandPixel> {
        let mut v = Vec::new();
        for y in 0..h {
            for x in 0..w {
                v.push(IslandPixel { x: 10 + x, y: 20 + y, tile_id: 1 });
            }
        }
        v
    }

    fn layer_xy(island: &CrumblingIsland) -> Vec<Vec<(i32, i32)>> {
        island.layers.iter().map(|l| {
            let mut v: Vec<_> = l.iter().map(|p| (p.x, p.y)).collect();
            v.sort();
            v
        }).collect()
    }

    #[test]
    fn a_block_peels_from_the_bottom_row_up() {
        let island = CrumblingIsland::new(&block(3, 3), 0.0);
        let layers = layer_xy(&island);
        assert_eq!(layers.len(), 3);
        assert_eq!(layers[0], vec![(10, 22), (11, 22), (12, 22)]);
        assert_eq!(layers[2], vec![(10, 20), (11, 20), (12, 20)]);
    }

    #[test]
    fn an_uneven_piece_has_its_edges_in_the_right_order() {
        // A column of 3 with a single pixel stuck out to the right at mid height:
        //   X.
        //   XX
        //   X.
        let mut px = block(1, 3);
        px.push(IslandPixel { x: 11, y: 21, tile_id: 1 });
        let layers = layer_xy(&CrumblingIsland::new(&px, 0.0));
        // bottom edge: the column's foot and the stuck-out pixel (nothing under either)
        assert_eq!(layers[0], vec![(10, 22), (11, 21)]);
        assert_eq!(layers[1], vec![(10, 21)]);
        assert_eq!(layers[2], vec![(10, 20)]);
    }

    #[test]
    fn every_pixel_is_released_exactly_once() {
        let mut d = DebrisSystem::new(7);
        d.add_island(&block(6, 5));
        let mut released = Vec::new();
        let mut guard = 0;
        while !d.islands.is_empty() {
            let before = d.particles.len();
            d.update(1.0 / 60.0);
            for p in d.particles.iter().skip(before) {
                released.push((p.x as i32, p.y as i32));
            }
            guard += 1;
            assert!(guard < 10_000);
        }
        assert_eq!(released.len(), 30);
    }

    #[test]
    fn release_order_never_skips_a_pixel_that_is_still_supported() {
        let mut rng = Rng::new(3);
        let mut island = CrumblingIsland::new(&block(7, 6), 0.0);
        let mut gone: HashSet<(i32, i32)> = HashSet::new();
        let all: HashSet<(i32, i32)> = block(7, 6).iter().map(|p| (p.x, p.y)).collect();
        while !island.finished() {
            let p = island.release_one(&mut rng);
            // everything below it (in its column) is already gone
            assert!(!all.contains(&(p.x, p.y + 1)) || gone.contains(&(p.x, p.y + 1)),
                    "pixel {:?} went while supported", (p.x, p.y));
            gone.insert((p.x, p.y));
        }
        assert_eq!(gone.len(), 42);
    }

    #[test]
    fn the_piece_hangs_for_the_delay_then_all_of_it_falls_and_fades() {
        let mut d = DebrisSystem::new(1);
        d.add_island(&block(4, 4));
        let dt = 1.0 / 60.0;
        for _ in 0..((DEBRIS_RELEASE_DELAY / dt) as usize - 1) {
            d.update(dt);
        }
        assert!(d.particles.is_empty(), "nothing should go during the delay");
        assert_eq!(d.islands[0].pending().count(), 16);
        for _ in 0..600 {
            d.update(dt);
        }
        assert!(d.islands.is_empty());
        assert!(d.particles.is_empty(), "everything fades away in the end");
    }

    #[test]
    fn same_seed_same_debris() {
        let run = |seed| {
            let mut d = DebrisSystem::new(seed);
            d.add_island(&block(5, 5));
            for _ in 0..40 {
                d.update(1.0 / 60.0);
            }
            d.particles.iter().map(|p| (p.x.to_bits(), p.y.to_bits())).collect::<Vec<_>>()
        };
        assert_eq!(run(5), run(5));
        assert_ne!(run(5), run(6));
    }

    #[test]
    fn the_particle_cap_holds() {
        let mut d = DebrisSystem::new(1);
        for _ in 0..MAX_DEBRIS_ISLANDS {
            d.add_island(&block(6, 6));
        }
        d.add_island(&block(6, 6)); // over the island cap: ignored
        assert_eq!(d.islands.len(), MAX_DEBRIS_ISLANDS);
        for _ in 0..200 {
            d.update(1.0 / 60.0);
            assert!(d.particles.len() <= MAX_DEBRIS_PARTICLES);
        }
    }

    #[test]
    fn a_pixel_fades_from_solid_to_gone() {
        let mut p = FallingPixel { x: 0.0, y: 0.0, vel_x: 0.0, vel_y: 0.0, age: 0.0, life: 1.0,
                                   tile_id: 1 };
        assert_eq!(p.alpha(), 1.0);
        p.age = 0.5;
        assert!((p.alpha() - 0.5).abs() < 1e-6);
        p.age = 2.0;
        assert_eq!(p.alpha(), 0.0);
    }

    #[test]
    fn a_hit_on_solid_ground_throws_chips_and_one_in_the_open_throws_none() {
        let reg = TileRegistry::from_tileset();
        let mut level = Level::new("t", 6, 6);
        for col in 0..6 {
            for row in 3..6 {
                level.set_tile(col, row, 1);
            }
        }
        let mut d = DebrisSystem::new(2);
        // a hit well up in the air: nothing round it is solid
        d.spray_impact(&level, &reg, 96.0, 20.0, 8.0);
        assert!(d.particles.is_empty());
        // a hit deep in the ground: solid all round
        d.spray_impact(&level, &reg, 96.0, 150.0, 8.0);
        assert!(!d.particles.is_empty() && d.particles.len() <= DEBRIS_IMPACT_PIXELS);
        for p in &d.particles {
            assert!(p.tile_id == 1);
            let (dx, dy) = (p.x as f64 - 96.0, p.y as f64 - 150.0);
            assert!((dx * dx + dy * dy).sqrt() <= 8.0 + 0.5, "chips start inside the hole");
        }
    }
}
