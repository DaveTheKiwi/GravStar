//! Collisions: finding where ships touch terrain, other ships and points,
//! and what happens to a ship when they do.
//!
//! Detection works on pixels: a ship's baked mask for its current angle is
//! laid over the tile masks (or the other ship's mask) and every pixel
//! solid in both is an overlap. The overlap pixels give the collision point
//! and, for terrain, the surface normal.
//!
//! The response treats a contact like a real one:
//!  * only the speed INTO the surface matters (a ship already moving away
//!    is left alone)
//!  * the normal component reverses and shrinks by the restitution; a slow
//!    approach (a ship resting) does not bounce at all
//!  * the sliding component is slowed by Coulomb friction only
//!  * damage comes from the impulse (change in momentum) of a hard impact,
//!    so grazing a wall or landing gently does none
//!  * two ships exchange equal and opposite impulses, using both masses

use crate::constants::*;
use crate::level::Level;
use crate::ship::Ship;
use crate::ship_masks::ShipMask;
use crate::tile_registry::TileRegistry;

/// Where and how two things are touching
#[derive(Clone, Debug, Default)]
pub struct CollisionResult {
    pub hit: bool,
    /// Average position of the overlapping pixels
    pub point_x: f64,
    pub point_y: f64,
    /// Unit vector pointing away from what was hit (for ship against ship:
    /// from the second ship towards the first)
    pub normal_x: f64,
    pub normal_y: f64,
    pub overlap_count: usize,
}

// -------------------------
// Detection
// -------------------------

fn bit_set(mask: &[u8], x: i32, y: i32, width: usize) -> bool {
    if x < 0 || y < 0 || x as usize >= width {
        return false;
    }
    let bit = y as usize * width + x as usize;
    mask.get(bit >> 3).map_or(false, |b| b & (1 << (bit & 7)) != 0)
}

/// The ship's current mask and the world pixel its top-left corner is on
fn placed_mask(ship: &Ship) -> Option<(&ShipMask, i32, i32)> {
    let set = ship.mask_set.as_ref()?;
    let mask = set.mask_for(ship.angle);
    let left = (ship.x - mask.width as f64 / 2.0).floor() as i32;
    let top = (ship.y - mask.height as f64 / 2.0).floor() as i32;
    Some((mask, left, top))
}

/// Every world pixel where the ship overlaps terrain, tile by tile (rows
/// then columns, as pixels are met). With `stop_at_first` it gives up as
/// soon as it finds one. Returns whether there was any overlap.
fn terrain_overlap(ship: &Ship, level: &Level, registry: &TileRegistry,
                   stop_at_first: bool, out: &mut Vec<(i32, i32)>) -> bool {
    let Some((mask, left, top)) = placed_mask(ship) else { return false };
    let (mw, mh) = (mask.width as i32, mask.height as i32);
    let ts = TILE_SIZE as i32;

    let first_col = left.div_euclid(ts).max(0);
    let last_col = (left + mw - 1).div_euclid(ts).min(level.width as i32 - 1);
    let first_row = top.div_euclid(ts).max(0);
    let last_row = (top + mh - 1).div_euclid(ts).min(level.height as i32 - 1);

    let mut found = false;
    for row in first_row..=last_row {
        for col in first_col..=last_col {
            let Some(tile) = level.tile(col, row) else { continue };
            if tile.tile_id == 0 || registry.get(tile.tile_id).is_none() {
                continue;
            }
            let tile_mask = tile.mask(registry);
            let (tx, ty) = (col * ts, row * ts);
            let (x0, x1) = (left.max(tx), (left + mw).min(tx + ts));
            let (y0, y1) = (top.max(ty), (top + mh).min(ty + ts));
            for y in y0..y1 {
                for x in x0..x1 {
                    if mask.is_solid(x - left, y - top)
                        && bit_set(tile_mask, x - tx, y - ty, TILE_SIZE)
                    {
                        found = true;
                        if stop_at_first {
                            return true;
                        }
                        out.push((x, y));
                    }
                }
            }
        }
    }
    found
}

/// Does the ship overlap any terrain at all?
pub fn ship_touches_terrain(ship: &Ship, level: &Level, registry: &TileRegistry) -> bool {
    terrain_overlap(ship, level, registry, true, &mut Vec::new())
}

/// Check the ship's pixel mask against the terrain
pub fn ship_vs_terrain(ship: &Ship, level: &Level, registry: &TileRegistry) -> CollisionResult {
    let mut pixels = Vec::new();
    terrain_overlap(ship, level, registry, false, &mut pixels);
    if pixels.is_empty() {
        return CollisionResult::default();
    }
    let n = pixels.len() as f64;
    let (normal_x, normal_y) = terrain_normal(&pixels, level, registry);
    CollisionResult {
        hit: true,
        point_x: pixels.iter().map(|p| p.0 as f64).sum::<f64>() / n,
        point_y: pixels.iter().map(|p| p.1 as f64).sum::<f64>() / n,
        normal_x,
        normal_y,
        overlap_count: pixels.len(),
    }
}

/// Check two ships' pixel masks against each other
pub fn ship_vs_ship(ship1: &Ship, ship2: &Ship) -> CollisionResult {
    let (Some((m1, left1, top1)), Some((m2, left2, top2))) =
        (placed_mask(ship1), placed_mask(ship2)) else { return CollisionResult::default() };

    let x0 = left1.max(left2);
    let x1 = (left1 + m1.width as i32).min(left2 + m2.width as i32);
    let y0 = top1.max(top2);
    let y1 = (top1 + m1.height as i32).min(top2 + m2.height as i32);

    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let mut count = 0usize;
    for y in y0..y1 {
        for x in x0..x1 {
            if m1.is_solid(x - left1, y - top1) && m2.is_solid(x - left2, y - top2) {
                sum_x += x as f64;
                sum_y += y as f64;
                count += 1;
            }
        }
    }
    if count == 0 {
        return CollisionResult::default();
    }

    // The normal points from ship 2's centre to ship 1's
    let (dx, dy) = (ship1.x - ship2.x, ship1.y - ship2.y);
    let dist = dx.hypot(dy);
    let (normal_x, normal_y) = if dist > 0.0 { (dx / dist, dy / dist) } else { (1.0, 0.0) };
    CollisionResult {
        hit: true,
        point_x: sum_x / count as f64,
        point_y: sum_y / count as f64,
        normal_x,
        normal_y,
        overlap_count: count,
    }
}

/// Is this world point on a solid pixel of the terrain (so it respects
/// damage)? Points outside the level are not solid.
pub fn point_vs_terrain(level: &Level, registry: &TileRegistry, x: f64, y: f64) -> bool {
    let ts = TILE_SIZE as i32;
    let (px, py) = (x.floor() as i32, y.floor() as i32);
    let (col, row) = (px.div_euclid(ts), py.div_euclid(ts));
    let Some(tile) = level.tile(col, row) else { return false };
    if tile.tile_id == 0 || registry.get(tile.tile_id).is_none() {
        return false;
    }
    bit_set(tile.mask(registry), px - col * ts, py - row * ts, TILE_SIZE)
}

/// Is this world point on a solid pixel of the ship's current mask?
pub fn point_vs_ship(ship: &Ship, x: f64, y: f64) -> bool {
    let Some((mask, left, top)) = placed_mask(ship) else { return false };
    mask.is_solid(x.floor() as i32 - left, y.floor() as i32 - top)
}

/// The direction of the terrain's surface at the overlap, as a unit vector
/// pointing away from the terrain. It is the opposite of the average
/// direction to the solid pixels around the overlapping ones (a gradient of
/// the terrain's density).
fn terrain_normal(pixels: &[(i32, i32)], level: &Level, registry: &TileRegistry) -> (f64, f64) {
    let radius = COLLISION_NORMAL_SAMPLE_RADIUS;
    let ts = TILE_SIZE as i32;

    // (dx, dy, dx / distance, dy / distance) for every neighbour
    let mut offsets = Vec::new();
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx == 0 && dy == 0 {
                continue;
            }
            let weight = 1.0 / ((dx * dx + dy * dy) as f64).sqrt();
            offsets.push((dx, dy, dx as f64 * weight, dy as f64 * weight));
        }
    }

    // Many overlapping pixels share neighbours, so each place is looked up
    // once: 0 = not looked at yet, 1 = solid, 2 = empty
    let min_x = pixels.iter().map(|p| p.0).min().unwrap() - radius;
    let max_x = pixels.iter().map(|p| p.0).max().unwrap() + radius;
    let min_y = pixels.iter().map(|p| p.1).min().unwrap() - radius;
    let max_y = pixels.iter().map(|p| p.1).max().unwrap() + radius;
    let width = (max_x - min_x + 1) as usize;
    let mut cache = vec![0u8; width * (max_y - min_y + 1) as usize];

    let mut is_solid = |x: i32, y: i32| -> bool {
        let slot = &mut cache[(y - min_y) as usize * width + (x - min_x) as usize];
        if *slot == 0 {
            let (col, row) = (x.div_euclid(ts), y.div_euclid(ts));
            let solid = match level.tile(col, row) {
                Some(tile) if tile.tile_id != 0 => {
                    bit_set(tile.mask(registry), x - col * ts, y - row * ts, TILE_SIZE)
                }
                _ => false,
            };
            *slot = if solid { 1 } else { 2 };
        }
        *slot == 1
    };

    let (mut grad_x, mut grad_y) = (0.0, 0.0);
    for &(x, y) in pixels {
        for &(dx, dy, wx, wy) in &offsets {
            if is_solid(x + dx, y + dy) {
                grad_x += wx;
                grad_y += wy;
            }
        }
    }

    // Away from the terrain: the negative gradient
    let (nx, ny) = (-grad_x, -grad_y);
    let length = nx.hypot(ny);
    if length > 0.0 {
        (nx / length, ny / length)
    } else {
        (0.0, -1.0) // no gradient: push upward
    }
}

// -------------------------
// Response
// -------------------------

/// The ship has hit terrain: bounce, slide, take damage if it was hard, and
/// move back out of the terrain. `hit` is what ship_vs_terrain returned.
pub fn resolve_terrain_contact(ship: &mut Ship, hit: &CollisionResult,
                               level: &Level, registry: &TileRegistry) {
    if !hit.hit {
        return;
    }
    let (nx, ny) = (hit.normal_x, hit.normal_y);

    // Speed INTO the surface; negative or zero means it is moving away
    let closing = -(ship.vel_x * nx + ship.vel_y * ny);
    if closing > 0.0 {
        let restitution = if closing < COLLISION_RESTING_SPEED { 0.0 } else { COLLISION_RESTITUTION };
        let bounce = (1.0 + restitution) * closing; // change in speed along the normal

        // Normal part: reverse and shrink
        ship.vel_x += bounce * nx;
        ship.vel_y += bounce * ny;

        // Sliding part: Coulomb friction, which can only stop the slide
        let (tx, ty) = (-ny, nx);
        let slide = ship.vel_x * tx + ship.vel_y * ty;
        let slowing = (COLLISION_FRICTION * bounce).min(slide.abs());
        ship.vel_x -= slide.signum() * slowing * tx;
        ship.vel_y -= slide.signum() * slowing * ty;

        // Damage from the impulse of a hard impact (momentum changed, times
        // how far above the harmless speed it was)
        let excess = (closing - COLLISION_SAFE_IMPACT_SPEED).max(0.0);
        let impulse = ship.physics.mass * (1.0 + restitution) * excess;
        ship.take_damage(COLLISION_DAMAGE_PER_IMPULSE * impulse);
    }
    ship.angular_velocity *= COLLISION_SPIN_RETENTION;

    // Move out along the normal, just far enough to be clear
    let mut moved = 0.0;
    while moved < COLLISION_MAX_PUSH_OUT {
        ship.x += nx * COLLISION_PUSH_OUT_STEP;
        ship.y += ny * COLLISION_PUSH_OUT_STEP;
        moved += COLLISION_PUSH_OUT_STEP;
        if !ship_touches_terrain(ship, level, registry) {
            break;
        }
    }
}

/// Two ships have collided: they exchange equal and opposite impulses
/// (so total momentum is kept), take damage and are moved apart. `hit` is
/// what ship_vs_ship(a, b) returned, with its normal pointing from b to a.
pub fn resolve_ship_contact(a: &mut Ship, b: &mut Ship, hit: &CollisionResult) {
    if !hit.hit {
        return;
    }
    let (nx, ny) = (hit.normal_x, hit.normal_y);
    let (inv_a, inv_b) = (1.0 / a.physics.mass, 1.0 / b.physics.mass);

    // How fast the ships are closing along the normal
    let (rvx, rvy) = (a.vel_x - b.vel_x, a.vel_y - b.vel_y);
    let closing = -(rvx * nx + rvy * ny);
    if closing > 0.0 {
        let restitution = if closing < COLLISION_RESTING_SPEED { 0.0 } else { COLLISION_RESTITUTION };
        let impulse = (1.0 + restitution) * closing / (inv_a + inv_b);
        a.vel_x += impulse * inv_a * nx;
        a.vel_y += impulse * inv_a * ny;
        b.vel_x -= impulse * inv_b * nx;
        b.vel_y -= impulse * inv_b * ny;

        // Friction along the contact, limited by the normal impulse
        let (tx, ty) = (-ny, nx);
        let slide = (a.vel_x - b.vel_x) * tx + (a.vel_y - b.vel_y) * ty;
        let friction = (COLLISION_FRICTION * impulse).min(slide.abs() / (inv_a + inv_b));
        let direction = slide.signum();
        a.vel_x -= direction * friction * inv_a * tx;
        a.vel_y -= direction * friction * inv_a * ty;
        b.vel_x += direction * friction * inv_b * tx;
        b.vel_y += direction * friction * inv_b * ty;

        // Both ships feel the same hit
        let excess = (closing - COLLISION_SAFE_IMPACT_SPEED).max(0.0);
        let damaging = (1.0 + restitution) * excess / (inv_a + inv_b);
        a.take_damage(COLLISION_DAMAGE_PER_IMPULSE * damaging);
        b.take_damage(COLLISION_DAMAGE_PER_IMPULSE * damaging);
    }
    a.angular_velocity *= COLLISION_SPIN_RETENTION;
    b.angular_velocity *= COLLISION_SPIN_RETENTION;

    // Move apart along the normal, the lighter ship moving further
    let (share_a, share_b) = (inv_a / (inv_a + inv_b), inv_b / (inv_a + inv_b));
    let mut moved = 0.0;
    while moved < COLLISION_MAX_PUSH_OUT {
        a.x += nx * COLLISION_PUSH_OUT_STEP * share_a;
        a.y += ny * COLLISION_PUSH_OUT_STEP * share_a;
        b.x -= nx * COLLISION_PUSH_OUT_STEP * share_b;
        b.y -= ny * COLLISION_PUSH_OUT_STEP * share_b;
        moved += COLLISION_PUSH_OUT_STEP;
        if !ship_vs_ship(a, b).hit {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ship_masks::ShipMaskSet;
    use std::sync::Arc;

    const DT: f64 = 1.0 / 60.0;

    fn solid_masks() -> Arc<ShipMaskSet> {
        // an opaque 48 x 32 block, so its mask is the ship's whole rectangle
        Arc::new(ShipMaskSet::bake(&[255u8; 48 * 32 * 4], 48, 32, 48, 32, 180))
    }

    fn ship_at(x: f64, y: f64, masks: &Arc<ShipMaskSet>) -> Ship {
        let mut s = Ship::new(x, y);
        s.mask_set = Some(masks.clone());
        s
    }

    /// 20 x 10 tiles with a solid floor along row 8 and below
    fn floor_level() -> Level {
        let mut level = Level::new("floor", 20, 10);
        for row in 8..10 {
            for col in 0..20 {
                level.set_tile(col, row, 1);
            }
        }
        level
    }

    fn step(ship: &mut Ship, level: &Level, reg: &TileRegistry) {
        ship.update(DT, GRAVITY);
        let hit = ship_vs_terrain(ship, level, reg);
        resolve_terrain_contact(ship, &hit, level, reg);
    }

    #[test]
    fn touching_the_floor_is_detected_with_an_upward_normal() {
        let reg = TileRegistry::from_tileset();
        let level = floor_level();
        let masks = solid_masks();
        let ship = ship_at(100.0, 256.0 - 16.0 + 3.0, &masks); // sunk 3 px
        let hit = ship_vs_terrain(&ship, &level, &reg);
        assert!(hit.hit);
        assert_eq!(hit.overlap_count, 3 * 48);
        assert!(hit.normal_y < -0.9 && hit.normal_x.abs() < 0.1, "{:?}", hit);
        let clear = ship_at(100.0, 100.0, &masks);
        assert!(!ship_vs_terrain(&clear, &level, &reg).hit);
        assert!(!ship_touches_terrain(&clear, &level, &reg));
    }

    #[test]
    fn a_hard_landing_bounces_a_little_and_hurts() {
        let reg = TileRegistry::from_tileset();
        let level = floor_level();
        let masks = solid_masks();
        let mut ship = ship_at(100.0, 256.0 - 16.0 + 2.0, &masks);
        ship.vel_y = 300.0;
        let hit = ship_vs_terrain(&ship, &level, &reg);
        resolve_terrain_contact(&mut ship, &hit, &level, &reg);
        assert!((ship.vel_y - -300.0 * COLLISION_RESTITUTION).abs() < 1.0, "{}", ship.vel_y);
        // impulse 1.45 * (300 - 50) * 0.04 = 14.5 damage, taken by the shield
        assert!((ship.health.shield - (100.0 - 14.5)).abs() < 0.1, "{}", ship.health.shield);
        assert!(!ship_touches_terrain(&ship, &level, &reg));
    }

    #[test]
    fn sliding_along_the_floor_loses_speed_to_friction_only() {
        let reg = TileRegistry::from_tileset();
        let level = floor_level();
        let masks = solid_masks();
        let mut ship = ship_at(100.0, 256.0 - 16.0 + 1.0, &masks);
        ship.vel_x = 300.0;
        ship.vel_y = 100.0; // a firm landing, but not a crash
        let hit = ship_vs_terrain(&ship, &level, &reg);
        resolve_terrain_contact(&mut ship, &hit, &level, &reg);
        // only the speed into the floor was bounced; friction took at most
        // 0.4 * 1.45 * 100 = 58 off the slide (the old code would have
        // left 135)
        assert!(ship.vel_x > 235.0 && ship.vel_x < 300.0, "{}", ship.vel_x);
        assert!(ship.vel_y < 0.0);
    }

    #[test]
    fn a_ship_moving_away_from_the_surface_is_left_alone() {
        let reg = TileRegistry::from_tileset();
        let level = floor_level();
        let masks = solid_masks();
        let mut ship = ship_at(100.0, 256.0 - 16.0 + 1.0, &masks);
        ship.vel_y = -50.0; // already leaving
        ship.vel_x = 40.0;
        let hit = ship_vs_terrain(&ship, &level, &reg);
        resolve_terrain_contact(&mut ship, &hit, &level, &reg);
        assert_eq!((ship.vel_x, ship.vel_y), (40.0, -50.0));
        assert_eq!(ship.health.shield, 100.0);
        assert!(!ship_touches_terrain(&ship, &level, &reg));
    }

    #[test]
    fn a_ship_dropped_onto_the_floor_comes_to_rest_without_damage_jitter() {
        let reg = TileRegistry::from_tileset();
        let level = floor_level();
        let masks = solid_masks();
        let mut ship = ship_at(320.0, 200.0, &masks);
        for _ in 0..(5.0 / DT) as usize {
            step(&mut ship, &level, &reg);
        }
        let shield_after_landing = ship.health.shield;
        let rest_y = ship.y;
        let mut worst_speed: f64 = 0.0;
        let mut worst_drift: f64 = 0.0;
        for _ in 0..(10.0 / DT) as usize {
            step(&mut ship, &level, &reg);
            worst_speed = worst_speed.max(ship.speed());
            worst_drift = worst_drift.max((ship.y - rest_y).abs());
        }
        assert!(worst_speed < 15.0, "resting speed {worst_speed}");
        assert!(worst_drift < 1.0, "drifted {worst_drift}");
        assert!((ship.health.shield - shield_after_landing).abs() < 1e-9);
        assert!((ship.y - (256.0 - 16.0)).abs() < 1.5, "resting height {}", ship.y);
    }

    #[test]
    fn a_ship_sliding_on_the_floor_stops() {
        let reg = TileRegistry::from_tileset();
        let level = floor_level();
        let masks = solid_masks();
        let mut ship = ship_at(100.0, 256.0 - 16.0, &masks);
        ship.vel_x = 200.0;
        for _ in 0..(6.0 / DT) as usize {
            step(&mut ship, &level, &reg);
        }
        assert!(ship.vel_x.abs() < 5.0, "{}", ship.vel_x);
        assert!(ship.x > 100.0 && ship.x < 600.0, "{}", ship.x);
    }

    #[test]
    fn ships_collide_and_share_momentum() {
        let masks = solid_masks();
        let mut a = ship_at(100.0, 100.0, &masks);
        let mut b = ship_at(140.0, 100.0, &masks); // overlapping by 8 px
        a.vel_x = 200.0;
        let hit = ship_vs_ship(&a, &b);
        assert!(hit.hit && hit.overlap_count == 8 * 32);
        assert!(hit.normal_x < -0.99); // from b towards a: to the left
        // the ships are as the old code treats them: a on the left of b
        let before = a.vel_x * a.physics.mass + b.vel_x * b.physics.mass;
        resolve_ship_contact(&mut a, &mut b, &hit);
        let after = a.vel_x * a.physics.mass + b.vel_x * b.physics.mass;
        assert!((before - after).abs() < 1e-9, "{before} {after}");
        assert!(b.vel_x > 0.0 && a.vel_x < 200.0);
        assert!(!ship_vs_ship(&a, &b).hit);
        // identical ships, a hit at 200: both take the same damage
        assert!(a.health.shield < 100.0);
        assert!((a.health.shield - b.health.shield).abs() < 1e-9);
    }

    #[test]
    fn a_heavy_ship_barely_notices_a_light_one() {
        let masks = solid_masks();
        let mut heavy = ship_at(100.0, 100.0, &masks);
        heavy.physics.mass = 20.0;
        let mut light = ship_at(140.0, 100.0, &masks);
        heavy.vel_x = 100.0;
        let hit = ship_vs_ship(&heavy, &light);
        resolve_ship_contact(&mut heavy, &mut light, &hit);
        assert!(heavy.vel_x > 85.0, "{}", heavy.vel_x);
        assert!(light.vel_x > 100.0, "{}", light.vel_x);
    }

    #[test]
    fn a_ship_with_no_masks_never_collides() {
        let reg = TileRegistry::from_tileset();
        let level = floor_level();
        let bare = Ship::new(100.0, 250.0);
        assert!(!ship_vs_terrain(&bare, &level, &reg).hit);
        assert!(!point_vs_ship(&bare, 100.0, 250.0));
    }

    #[test]
    fn points_respect_damage_and_the_level_edge() {
        let reg = TileRegistry::from_tileset();
        let mut level = floor_level();
        assert!(point_vs_terrain(&level, &reg, 100.5, 260.2));
        assert!(!point_vs_terrain(&level, &reg, 100.5, 100.0));
        assert!(!point_vs_terrain(&level, &reg, -3.0, 260.0));
        assert!(!point_vs_terrain(&level, &reg, 100.0, 9999.0));
        level.carve_circle(&reg, 100.0, 260.0, 10.0);
        assert!(!point_vs_terrain(&level, &reg, 100.0, 260.0));
        let masks = solid_masks();
        let ship = ship_at(100.0, 100.0, &masks);
        assert!(point_vs_ship(&ship, 100.0, 100.0));
        assert!(!point_vs_ship(&ship, 100.0, 130.0));
    }
}
