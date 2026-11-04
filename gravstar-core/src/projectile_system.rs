//! Owns every live projectile: spawns them from ships' weapons, moves them,
//! applies drag and works out what they hit.

use crate::collision::{point_vs_ship, point_vs_terrain};
use crate::constants::*;
use crate::level::{CarveResult, Level};
use crate::physics::settle;
use crate::projectile::ProjectileType;
use crate::ship::Ship;
use crate::tile_registry::TileRegistry;

pub struct Projectile {
    pub ptype: ProjectileType,
    /// The index of the ship that fired it
    pub owner: usize,
    pub x: f64,
    pub y: f64,
    /// Pixels per second
    pub vel_x: f64,
    pub vel_y: f64,
    /// Seconds in flight
    pub age: f64,
    pub alive: bool,
}

impl Projectile {
    pub fn speed(&self) -> f64 {
        self.vel_x.hypot(self.vel_y)
    }
}

/// What happened to a projectile this frame. update() returns these so
/// effects, sound and scoring can react without touching the physics.
pub enum ProjectileEvent {
    /// Hit the terrain at (x, y) and cut a hole. `carve` says which tiles
    /// changed and which small pieces were cut free (for crumbling debris).
    Terrain { x: f64, y: f64, carve: CarveResult },
    /// Hit a ship (damage is already applied, unless it was immune)
    Ship { x: f64, y: f64, target: usize },
    /// Flew out of the level
    Bounds { x: f64, y: f64 },
    /// Slowed down too much and vanished
    Expired { x: f64, y: f64 },
}

#[derive(Default)]
pub struct ProjectileSystem {
    pub projectiles: Vec<Projectile>,
}

impl ProjectileSystem {
    pub fn new() -> Self {
        ProjectileSystem::default()
    }

    /// Fire weapons, advance projectiles. Returns an event for everything
    /// that hit or expired this frame.
    pub fn update(&mut self, dt: f64, ships: &mut [Ship], level: &mut Level,
                  registry: &TileRegistry) -> Vec<ProjectileEvent> {
        let mut events = Vec::new();

        for index in 0..ships.len() {
            let trigger = ships[index].input.fire && ships[index].can_fire();
            if ships[index].weapon.update(dt, trigger) {
                self.spawn(index, ships, level, registry, &mut events);
            }
        }

        for i in 0..self.projectiles.len() {
            if self.projectiles[i].alive {
                self.advance(i, dt, ships, level, registry, &mut events);
            }
        }

        self.projectiles.retain(|p| p.alive);
        events
    }

    fn spawn(&mut self, owner: usize, ships: &mut [Ship], level: &mut Level,
             registry: &TileRegistry, events: &mut Vec<ProjectileEvent>) {
        if self.projectiles.len() >= MAX_PROJECTILES {
            return;
        }
        let ship = &ships[owner];
        let ptype = ship.weapon.projectile_type.clone();
        let (fx, fy) = ship.facing_vector();
        let mut vel_x = fx * ptype.muzzle_speed;
        let mut vel_y = fy * ptype.muzzle_speed;
        if ptype.inherit_ship_velocity {
            vel_x += ship.vel_x;
            vel_y += ship.vel_y;
        }
        let (muzzle_x, muzzle_y) = ship.muzzle_position();
        let (centre_x, centre_y) = (ship.x, ship.y);

        self.projectiles.push(Projectile {
            ptype, owner, x: muzzle_x, y: muzzle_y, vel_x, vel_y, age: 0.0, alive: true,
        });
        let index = self.projectiles.len() - 1;

        // Sweep from the ship's centre out to the gun, so firing with the
        // nose against a wall hits the wall instead of passing through it
        Self::sweep(&mut self.projectiles[index], centre_x, centre_y, muzzle_x, muzzle_y,
                    ships, level, registry, events);
    }

    fn advance(&mut self, index: usize, dt: f64, ships: &mut [Ship], level: &mut Level,
               registry: &TileRegistry, events: &mut Vec<ProjectileEvent>) {
        let p = &mut self.projectiles[index];

        // Linear drag, exactly as for ships: the speed decays at rate
        // coefficient / mass, and the bullet travels the matching distance
        let rate = p.ptype.drag_coefficient / p.ptype.mass;
        let (vx, dx) = settle(p.vel_x, 0.0, rate, dt);
        let (vy, dy) = settle(p.vel_y, 0.0, rate, dt);
        p.vel_x = vx;
        p.vel_y = vy;

        if p.speed() < p.ptype.min_speed {
            p.alive = false;
            events.push(ProjectileEvent::Expired { x: p.x, y: p.y });
            return;
        }

        let (x0, y0) = (p.x, p.y);
        let (x1, y1) = (x0 + dx, y0 + dy);
        p.age += dt;
        if !Self::sweep(p, x0, y0, x1, y1, ships, level, registry, events) {
            p.x = x1;
            p.y = y1;
        }
    }

    /// Walk from (x0, y0) to (x1, y1) in small steps, stopping at the first
    /// thing hit. Returns true if the projectile hit something.
    #[allow(clippy::too_many_arguments)]
    fn sweep(p: &mut Projectile, x0: f64, y0: f64, x1: f64, y1: f64,
             ships: &mut [Ship], level: &mut Level, registry: &TileRegistry,
             events: &mut Vec<ProjectileEvent>) -> bool {
        let distance = (x1 - x0).hypot(y1 - y0);
        let steps = ((distance / PROJECTILE_SAMPLE_STEP).ceil() as usize).max(1);
        for i in 1..=steps {
            let t = i as f64 / steps as f64;
            if Self::test_point(p, x0 + (x1 - x0) * t, y0 + (y1 - y0) * t,
                                ships, level, registry, events) {
                return true;
            }
        }
        false
    }

    fn test_point(p: &mut Projectile, x: f64, y: f64, ships: &mut [Ship],
                  level: &mut Level, registry: &TileRegistry,
                  events: &mut Vec<ProjectileEvent>) -> bool {
        let ts = TILE_SIZE as f64;

        // Left the level
        if !level.in_bounds((x / ts).floor() as i32, (y / ts).floor() as i32) {
            Self::finish(p, x, y);
            events.push(ProjectileEvent::Bounds { x, y });
            return true;
        }

        // Terrain: cut a hole where it landed (no bounce - it just stops)
        if point_vs_terrain(level, registry, x, y) {
            let (mut cut_x, mut cut_y) = (x, y);
            let speed = p.speed();
            if p.ptype.crater_embed != 0.0 && speed > 1e-6 {
                cut_x += p.vel_x / speed * p.ptype.crater_embed;
                cut_y += p.vel_y / speed * p.ptype.crater_embed;
            }
            let carve = level.carve_circle(registry, cut_x, cut_y, p.ptype.crater_radius);
            Self::finish(p, x, y);
            events.push(ProjectileEvent::Terrain { x, y, carve });
            return true;
        }

        // Ships
        for (index, ship) in ships.iter_mut().enumerate() {
            if index == p.owner && !p.ptype.hits_owner {
                continue;
            }
            if !ship.is_alive() {
                continue;
            }
            if point_vs_ship(ship, x, y) {
                ship.take_damage(p.ptype.ship_damage); // nothing if immune
                Self::finish(p, x, y);
                events.push(ProjectileEvent::Ship { x, y, target: index });
                return true;
            }
        }
        false
    }

    fn finish(p: &mut Projectile, x: f64, y: f64) {
        p.x = x;
        p.y = y;
        p.alive = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ship_masks::ShipMaskSet;
    use std::sync::Arc;

    const DT: f64 = 1.0 / 60.0;

    fn masks() -> Arc<ShipMaskSet> {
        Arc::new(ShipMaskSet::bake(&[255u8; 48 * 32 * 4], 48, 32, 48, 32, 180))
    }

    fn ship_at(x: f64, y: f64) -> Ship {
        let mut s = Ship::new(x, y);
        s.mask_set = Some(masks());
        s
    }

    fn fire_one(system: &mut ProjectileSystem, ships: &mut [Ship], level: &mut Level,
                reg: &TileRegistry) -> Vec<ProjectileEvent> {
        ships[0].input.fire = true;
        let events = system.update(DT, ships, level, reg);
        ships[0].input.fire = false;
        events
    }

    fn run(system: &mut ProjectileSystem, ships: &mut [Ship], level: &mut Level,
           reg: &TileRegistry, seconds: f64) -> Vec<ProjectileEvent> {
        let mut all = Vec::new();
        for _ in 0..(seconds / DT).round() as usize {
            all.extend(system.update(DT, ships, level, reg));
        }
        all
    }

    #[test]
    fn a_fired_bullet_leaves_the_nose_at_muzzle_speed() {
        let reg = TileRegistry::from_tileset();
        let mut level = Level::new("t", 20, 20);
        let mut ships = vec![ship_at(300.0, 300.0)];
        let mut system = ProjectileSystem::new();
        fire_one(&mut system, &mut ships, &mut level, &reg);
        assert_eq!(system.projectiles.len(), 1);
        let p = &system.projectiles[0];
        // pointing up: above the ship, moving up, a little slower after a frame of drag
        assert!(p.y < 300.0 - 10.0 && p.vel_y < -800.0 && p.vel_x.abs() < 1e-9);
        assert_eq!(p.owner, 0);
    }

    #[test]
    fn a_bullet_inherits_the_ships_velocity() {
        let reg = TileRegistry::from_tileset();
        let mut level = Level::new("t", 20, 20);
        let mut ships = vec![ship_at(300.0, 300.0)];
        ships[0].vel_x = 200.0;
        let mut system = ProjectileSystem::new();
        fire_one(&mut system, &mut ships, &mut level, &reg);
        assert!(system.projectiles[0].vel_x > 150.0);
    }

    #[test]
    fn a_bullet_cuts_a_hole_in_a_wall_and_stops() {
        let reg = TileRegistry::from_tileset();
        let mut level = Level::new("t", 20, 20);
        for col in 0..20 {
            level.set_tile(col, 2, 1); // a ceiling
        }
        let mut ships = vec![ship_at(320.0, 200.0)];
        let mut system = ProjectileSystem::new();
        fire_one(&mut system, &mut ships, &mut level, &reg);
        let events = run(&mut system, &mut ships, &mut level, &reg, 0.5);
        let hit = events.iter().find_map(|e| match e {
            ProjectileEvent::Terrain { carve, y, .. } => Some((carve.changed.len(), *y)),
            _ => None,
        }).expect("the bullet should hit the ceiling");
        assert!(hit.0 >= 1);
        assert!(hit.1 > 90.0 && hit.1 < 100.0, "hit at y = {}", hit.1);
        assert!(system.projectiles.is_empty());
        assert!(!point_vs_terrain(&level, &reg, 320.0, 94.0));
    }

    #[test]
    fn firing_with_the_nose_against_a_wall_hits_the_wall() {
        let reg = TileRegistry::from_tileset();
        let mut level = Level::new("t", 20, 20);
        for col in 0..20 {
            level.set_tile(col, 5, 1); // solid from y = 160 to 192
        }
        // the nose is 16 px above the centre: put it 4 px inside the wall
        let mut ships = vec![ship_at(320.0, 192.0 + 12.0)];
        let mut system = ProjectileSystem::new();
        let events = fire_one(&mut system, &mut ships, &mut level, &reg);
        assert!(events.iter().any(|e| matches!(e, ProjectileEvent::Terrain { .. })));
        assert!(system.projectiles.is_empty());
    }

    #[test]
    fn a_fast_bullet_cannot_skip_a_thin_wall() {
        let reg = TileRegistry::from_tileset();
        let mut level = Level::new("t", 20, 20);
        level.set_tile(10, 5, 2); // a half tile (top 16 rows)
        let mut ships = vec![ship_at(10.0 * 32.0 + 16.0, 400.0)];
        let mut system = ProjectileSystem::new();
        fire_one(&mut system, &mut ships, &mut level, &reg);
        system.projectiles[0].vel_y = -20000.0; // absurdly fast
        let events = run(&mut system, &mut ships, &mut level, &reg, 0.2);
        assert!(events.iter().any(|e| matches!(e, ProjectileEvent::Terrain { .. })));
    }

    #[test]
    fn bullets_that_leave_the_level_or_slow_down_vanish() {
        let reg = TileRegistry::from_tileset();
        let mut level = Level::new("t", 4, 4);
        let mut ships = vec![ship_at(64.0, 100.0)];
        let mut system = ProjectileSystem::new();
        fire_one(&mut system, &mut ships, &mut level, &reg);
        let events = run(&mut system, &mut ships, &mut level, &reg, 1.0);
        assert!(events.iter().any(|e| matches!(e, ProjectileEvent::Bounds { .. })));

        // a bullet that is already slow
        system.projectiles.push(Projectile {
            ptype: ProjectileType::basic_bullet(), owner: 0, x: 64.0, y: 64.0,
            vel_x: 10.0, vel_y: 0.0, age: 0.0, alive: true,
        });
        let events = run(&mut system, &mut ships, &mut level, &reg, 0.1);
        assert!(events.iter().any(|e| matches!(e, ProjectileEvent::Expired { .. })));
        assert!(system.projectiles.is_empty());
    }

    #[test]
    fn a_bullet_slows_down_with_drag() {
        let reg = TileRegistry::from_tileset();
        let mut level = Level::new("t", 100, 100);
        let mut ships = vec![ship_at(1600.0, 3000.0)];
        let mut system = ProjectileSystem::new();
        fire_one(&mut system, &mut ships, &mut level, &reg);
        let v0 = system.projectiles[0].speed();
        run(&mut system, &mut ships, &mut level, &reg, 1.0);
        let p = &system.projectiles[0];
        let expected = v0 * (-BULLET_DRAG_COEFFICIENT / BULLET_MASS).exp();
        assert!((p.speed() - expected).abs() < 5.0, "{} vs {}", p.speed(), expected);
    }

    #[test]
    fn bullets_hurt_other_ships_but_not_their_owner_and_immunity_absorbs_them() {
        let reg = TileRegistry::from_tileset();
        let mut level = Level::new("t", 20, 20);
        let mut ships = vec![ship_at(320.0, 400.0), ship_at(320.0, 200.0)];
        let mut system = ProjectileSystem::new();
        fire_one(&mut system, &mut ships, &mut level, &reg);
        let events = run(&mut system, &mut ships, &mut level, &reg, 0.5);
        assert!(events.iter().any(|e| matches!(e, ProjectileEvent::Ship { target: 1, .. })));
        assert_eq!(ships[1].health.shield, 100.0 - BULLET_SHIP_DAMAGE);
        assert_eq!(ships[0].health.shield, 100.0);

        // an immune ship takes nothing, but still stops the bullet
        ships[1].unfreeze(5.0);
        let shield = ships[1].health.shield;
        fire_one(&mut system, &mut ships, &mut level, &reg);
        let events = run(&mut system, &mut ships, &mut level, &reg, 0.5);
        assert!(events.iter().any(|e| matches!(e, ProjectileEvent::Ship { target: 1, .. })));
        assert_eq!(ships[1].health.shield, shield);
        assert!(system.projectiles.is_empty());
    }

    #[test]
    fn a_wrecked_ship_cannot_be_hit_or_fire() {
        let reg = TileRegistry::from_tileset();
        let mut level = Level::new("t", 20, 20);
        let mut ships = vec![ship_at(320.0, 400.0), ship_at(320.0, 200.0)];
        ships[1].kill();
        let mut system = ProjectileSystem::new();
        fire_one(&mut system, &mut ships, &mut level, &reg);
        let events = run(&mut system, &mut ships, &mut level, &reg, 0.5);
        assert!(!events.iter().any(|e| matches!(e, ProjectileEvent::Ship { .. })));
        ships[0].freeze();
        let before = system.projectiles.len();
        fire_one(&mut system, &mut ships, &mut level, &reg);
        assert_eq!(system.projectiles.len(), before); // frozen: cannot fire
    }

    #[test]
    fn the_number_of_bullets_is_capped() {
        let reg = TileRegistry::from_tileset();
        let mut level = Level::new("t", 20, 20);
        let mut ships = vec![ship_at(320.0, 400.0)];
        let mut system = ProjectileSystem::new();
        for _ in 0..MAX_PROJECTILES + 10 {
            system.projectiles.push(Projectile {
                ptype: ProjectileType::basic_bullet(), owner: 0, x: 10.0, y: 10.0,
                vel_x: 0.0, vel_y: -500.0, age: 0.0, alive: true,
            });
        }
        fire_one(&mut system, &mut ships, &mut level, &reg);
        assert!(system.projectiles.len() <= MAX_PROJECTILES + 10);
    }

    #[test]
    fn a_bullet_that_shatters_a_small_piece_reports_the_island() {
        let reg = TileRegistry::from_tileset();
        let mut level = Level::new("t", 6, 6);
        level.set_tile(2, 2, 34); // a small corner triangle floating alone
        let mut ships = vec![ship_at(72.0, 170.0)];
        let mut system = ProjectileSystem::new();
        fire_one(&mut system, &mut ships, &mut level, &reg);
        let events = run(&mut system, &mut ships, &mut level, &reg, 0.5);
        let islands = events.iter().find_map(|e| match e {
            ProjectileEvent::Terrain { carve, .. } => Some(carve.islands.len()),
            _ => None,
        }).expect("the bullet should hit the triangle");
        assert!(islands >= 1, "expected the rubble to be reported as islands");
        // nothing solid is left in the tile
        assert!(level.tile(2, 2).unwrap().mask(&reg).iter().all(|b| *b == 0));
    }
}
