//! What defines a KIND of projectile. A new kind is a new function here that
//! fills in the numbers; the code that moves and hits things does not care
//! which kind it is handling.

use crate::constants::*;

#[derive(Clone, Debug)]
pub struct ProjectileType {
    pub name: &'static str,
    pub mass: f64,
    /// Speed leaving the gun, pixels per second
    pub muzzle_speed: f64,
    /// Linear drag: force = coefficient * speed (see SHIP_DRAG_COEFFICIENT)
    pub drag_coefficient: f64,
    /// Slower than this and it vanishes
    pub min_speed: f64,
    /// Add the firing ship's velocity to the bullet's
    pub inherit_ship_velocity: bool,
    /// Damage to a ship it hits
    pub ship_damage: f64,
    /// Radius of the hole it cuts in terrain
    pub crater_radius: f64,
    /// The bullet stops at the surface it hits, so by default the hole is
    /// centred on that surface (a half disc against a flat wall). This pushes
    /// the centre that many pixels into the material for fuller craters.
    pub crater_embed: f64,
    /// Can it hurt the ship that fired it
    pub hits_owner: bool,
}

impl ProjectileType {
    pub fn basic_bullet() -> Self {
        ProjectileType {
            name: "basic",
            mass: BULLET_MASS,
            muzzle_speed: BULLET_MUZZLE_SPEED,
            drag_coefficient: BULLET_DRAG_COEFFICIENT,
            min_speed: BULLET_MIN_SPEED,
            inherit_ship_velocity: true,
            ship_damage: BULLET_SHIP_DAMAGE,
            crater_radius: BULLET_CRATER_RADIUS,
            crater_embed: BULLET_CRATER_EMBED,
            hits_owner: false,
        }
    }
}
