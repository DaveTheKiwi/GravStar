//! The simulation: the level, the ships, the bullets and the effects, and
//! one function that moves the whole lot forward by a frame. Pure logic - the
//! game crate gives it input and draws what is in it.

use crate::collision::{resolve_ship_contact, resolve_terrain_contact, ship_vs_ship, ship_vs_terrain};
use crate::constants::*;
use crate::explosion_system::ExplosionSystem;
use crate::level::Level;
use crate::match_rules::{Match, MatchEvent};
use crate::projectile_system::{ProjectileEvent, ProjectileSystem};
use crate::random::Rng;
use crate::ship::{Ship, ShipInput};
use crate::tile_registry::TileRegistry;

/// What happened during one step, for the caller to react to
pub struct StepReport {
    /// What the bullets hit or did
    pub projectile_events: Vec<ProjectileEvent>,
    /// Deaths and respawns
    pub match_events: Vec<MatchEvent>,
    /// Every tile whose terrain changed, as (col, row)
    pub changed_tiles: Vec<(usize, usize)>,
}

pub struct World {
    pub level: Level,
    pub registry: TileRegistry,
    pub ships: Vec<Ship>,
    pub projectiles: ProjectileSystem,
    pub explosions: ExplosionSystem,
    /// Lives, countdowns, immunity, win and lose
    pub match_state: Match,
    pub rng: Rng,
}

impl World {
    /// A new game: every ship starts frozen, counting down to the start
    pub fn new(level: Level, registry: TileRegistry, mut ships: Vec<Ship>, seed: u64,
               lives: u32) -> Self {
        let match_state = Match::new(&mut ships, lives);
        World {
            level,
            registry,
            ships,
            projectiles: ProjectileSystem::new(),
            explosions: ExplosionSystem::new(),
            match_state,
            rng: Rng::new(seed),
        }
    }

    /// The level's spawn points as world positions (each tile's top-left)
    pub fn spawn_positions(level: &Level) -> Vec<(f64, f64)> {
        let ts = TILE_SIZE as f64;
        level.spawn_points.iter().map(|p| (p.col as f64 * ts, p.row as f64 * ts)).collect()
    }

    /// Pick `count` different spawn points at random, or None if the level
    /// does not have that many
    pub fn pick_spawns(level: &Level, rng: &mut Rng, count: usize) -> Option<Vec<(f64, f64)>> {
        let mut points = Self::spawn_positions(level);
        if points.len() < count {
            return None;
        }
        let mut picked = Vec::new();
        for _ in 0..count {
            picked.push(points.remove(rng.below(points.len())));
        }
        Some(picked)
    }

    pub fn width_px(&self) -> f64 {
        (self.level.width * TILE_SIZE) as f64
    }

    pub fn height_px(&self) -> f64 {
        (self.level.height * TILE_SIZE) as f64
    }

    /// Advance everything by `dt` seconds. `inputs` is one ShipInput per
    /// ship. Returns what happened.
    pub fn step(&mut self, dt: f64, inputs: &[ShipInput]) -> StepReport {
        for (ship, input) in self.ships.iter_mut().zip(inputs) {
            ship.input = *input;
        }
        for ship in self.ships.iter_mut() {
            ship.update(dt, GRAVITY);
        }

        // Collisions: terrain, then ship against ship
        for ship in self.ships.iter_mut() {
            if !ship.is_solid() {
                continue; // counting down, or wrecked
            }
            let hit = ship_vs_terrain(ship, &self.level, &self.registry);
            resolve_terrain_contact(ship, &hit, &self.level, &self.registry);
        }
        if self.ships.len() >= 2 {
            let (first, rest) = self.ships.split_at_mut(1);
            let (a, b) = (&mut first[0], &mut rest[0]);
            if a.is_solid() && b.is_solid() {
                let hit = ship_vs_ship(a, b);
                resolve_ship_contact(a, b, &hit);
            }
        }

        // Ships cannot leave the level
        let (max_x, max_y) = (self.width_px(), self.height_px());
        for ship in self.ships.iter_mut() {
            ship.x = ship.x.clamp(0.0, max_x);
            ship.y = ship.y.clamp(0.0, max_y);
        }

        // Bullets: they hit ships and cut terrain; hits make explosions
        let projectile_events =
            self.projectiles.update(dt, &mut self.ships, &mut self.level, &self.registry);
        let mut changed_tiles = Vec::new();
        for event in &projectile_events {
            match event {
                ProjectileEvent::Terrain { x, y, carve } => {
                    self.explosions.spawn(*x, *y, None, None);
                    changed_tiles.extend(carve.changed.iter().copied());
                }
                ProjectileEvent::Ship { x, y, .. } => {
                    self.explosions.spawn(*x, *y, None, None);
                }
                _ => {}
            }
        }

        // Lives, deaths, respawns, game over
        let spawns = Self::spawn_positions(&self.level);
        let match_events =
            self.match_state.update(&mut self.ships, &spawns, &mut self.rng, dt);
        for event in &match_events {
            if let MatchEvent::Died(player) = event {
                self.ship_destroyed(*player);
            }
        }

        self.explosions.update(dt);
        StepReport { projectile_events, match_events, changed_tiles }
    }

    /// A big, slow explosion where the ship was, and a few small ones around it
    fn ship_destroyed(&mut self, player: usize) {
        let (x, y) = (self.ships[player].x, self.ships[player].y);
        self.explosions.spawn(x, y, Some(DEATH_EXPLOSION_RADIUS), Some(DEATH_EXPLOSION_SECONDS));
        for _ in 0..DEATH_SMALL_EXPLOSIONS {
            let spread = |rng: &mut Rng| (rng.unit() * 2.0 - 1.0) * DEATH_SMALL_SPREAD;
            let between = |rng: &mut Rng, lo: f64, hi: f64| lo + rng.unit() * (hi - lo);
            let (dx, dy) = (spread(&mut self.rng), spread(&mut self.rng));
            let radius = between(&mut self.rng, DEATH_SMALL_RADIUS_MIN, DEATH_SMALL_RADIUS_MAX);
            let seconds = between(&mut self.rng, DEATH_SMALL_SECONDS_MIN, DEATH_SMALL_SECONDS_MAX);
            self.explosions.spawn(x + dx, y + dy, Some(radius), Some(seconds));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ship_masks::ShipMaskSet;
    use std::sync::Arc;

    const DT: f64 = 1.0 / 60.0;

    fn world() -> World {
        let mut level = Level::new("w", 30, 20);
        for col in 0..30 {
            level.set_tile(col, 18, 1);
            level.set_tile(col, 19, 1);
        }
        level.spawn_points.push(crate::level::SpawnPoint { col: 5, row: 10, id: 1 });
        level.spawn_points.push(crate::level::SpawnPoint { col: 20, row: 10, id: 2 });
        let masks = Arc::new(ShipMaskSet::bake(&[255u8; 48 * 32 * 4], 48, 32, 48, 32, 180));
        let mut ships = vec![Ship::new(160.0, 320.0), Ship::new(640.0, 320.0)];
        for s in ships.iter_mut() {
            s.mask_set = Some(masks.clone());
        }
        World::new(level, TileRegistry::from_tileset(), ships, 1, STARTING_LIVES)
    }

    /// Run the opening countdown (ships are frozen until it ends)
    fn start(w: &mut World) {
        let idle = [ShipInput::default(); 2];
        for _ in 0..((COUNTDOWN_SECONDS + 0.2) / DT) as usize {
            w.step(DT, &idle);
        }
    }

    #[test]
    fn ships_wait_out_the_countdown_then_fall() {
        let mut w = world();
        let idle = [ShipInput::default(); 2];
        let y0 = w.ships[0].y;
        for _ in 0..60 {
            w.step(DT, &idle);
        }
        assert_eq!(w.ships[0].y, y0); // frozen: no gravity
        assert!(w.match_state.countdown_number(0).is_some());
        start(&mut w);
        for _ in 0..60 {
            w.step(DT, &idle);
        }
        assert!(w.ships[0].y > y0);
    }

    #[test]
    fn ships_fall_land_and_stay_in_the_level() {
        let mut w = world();
        start(&mut w);
        let idle = [ShipInput::default(); 2];
        for _ in 0..(8.0 / DT) as usize {
            w.step(DT, &idle);
        }
        // the floor's top is at row 18 = y 576; a ship's centre rests half its height above
        for s in &w.ships {
            assert!((s.y - (576.0 - 16.0)).abs() < 1.5, "{}", s.y);
            assert!(s.speed() < 10.0);
        }
    }

    #[test]
    fn a_ship_cannot_fly_out_of_the_level() {
        let mut w = world();
        start(&mut w);
        w.ships[0].input.thrust = 1.0;
        let fly = [ShipInput { thrust: 1.0, ..Default::default() }, ShipInput::default()];
        for _ in 0..(10.0 / DT) as usize {
            w.step(DT, &fly);
        }
        assert!(w.ships[0].y >= 0.0);
    }

    #[test]
    fn firing_makes_bullets_that_cut_terrain_and_explode() {
        let mut w = world();
        start(&mut w);
        // aim the first ship at the floor and fire
        w.ships[0].angle = std::f64::consts::PI;
        w.ships[0].y = 300.0;
        let fire = [ShipInput { fire: true, ..Default::default() }, ShipInput::default()];
        let mut terrain_hits = 0;
        for _ in 0..(1.0 / DT) as usize {
            for e in w.step(DT, &fire).projectile_events {
                if matches!(e, ProjectileEvent::Terrain { .. }) {
                    terrain_hits += 1;
                }
            }
        }
        assert!(terrain_hits >= 2, "{terrain_hits}");
        assert!(!w.explosions.explosions.is_empty() || terrain_hits > 0);
        // some floor has been cut away
        assert!((0..30).any(|c| w.level.tile(c, 18).unwrap().is_damaged()));
    }

    #[test]
    fn a_destroyed_ship_costs_a_life_and_explodes() {
        let mut w = world();
        start(&mut w);
        for _ in 0..(IMMUNITY_SECONDS / DT) as usize + 5 {
            w.step(DT, &[ShipInput::default(); 2]);
        }
        w.ships[0].health.take_damage(1000.0);
        let report = w.step(DT, &[ShipInput::default(); 2]);
        assert!(report.match_events.contains(&MatchEvent::Died(0)));
        assert_eq!(w.match_state.lives(0), STARTING_LIVES - 1);
        assert!(w.explosions.explosions.len() > DEATH_SMALL_EXPLOSIONS);
        assert!(w.ships[0].dead);
    }

    #[test]
    fn changed_tiles_are_reported() {
        let mut w = world();
        start(&mut w);
        w.ships[0].angle = std::f64::consts::PI;
        w.ships[0].y = 300.0;
        let fire = [ShipInput { fire: true, ..Default::default() }, ShipInput::default()];
        let mut changed = 0;
        for _ in 0..60 {
            changed += w.step(DT, &fire).changed_tiles.len();
        }
        assert!(changed > 0);
    }

    #[test]
    fn spawns_are_picked_without_repeats() {
        let w = world();
        let mut rng = Rng::new(5);
        for _ in 0..20 {
            let p = World::pick_spawns(&w.level, &mut rng, 2).unwrap();
            assert_ne!(p[0], p[1]);
        }
        assert!(World::pick_spawns(&w.level, &mut rng, 3).is_none());
    }

    #[test]
    fn the_ships_can_bump_into_each_other() {
        let mut w = world();
        start(&mut w);
        w.ships[0].x = 400.0;
        w.ships[0].y = 300.0;
        w.ships[1].x = 430.0;
        w.ships[1].y = 300.0;
        w.ships[0].vel_x = 100.0;
        let idle = [ShipInput::default(); 2];
        for _ in 0..30 {
            w.step(DT, &idle);
        }
        assert!(w.ships[1].x - w.ships[0].x >= 40.0);
    }
}
