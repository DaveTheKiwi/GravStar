//! A ship: its physics, health, input and state.
//!
//! The physics uses real-world quantities. Lengths are pixels, time is
//! seconds, angles are radians (0 = pointing up, turning clockwise), and:
//!   * thrust is a FORCE, so acceleration = force / mass
//!   * gravity is an acceleration, so it pulls every mass the same
//!   * air drag is a linear drag force, F = -b * v, whose effect on a
//!     coasting ship is exponential decay of its speed at rate b / mass
//!   * steering is a TORQUE, so angular acceleration = torque / moment of
//!     inertia, and the ship's spin has its own drag torque
//! Over each frame the forces are treated as constant, and the motion is
//! worked out exactly (see `settle`): a ship speeds up or slows down towards
//! the speed where drag balances the other forces, which gives the same
//! result at any frame rate.

use std::sync::Arc;

use crate::constants::*;
use crate::physics::settle;
use crate::projectile::ProjectileType;
use crate::ship_masks::ShipMaskSet;
use crate::weapon::Weapon;

/// Physical properties of a ship type. These are the base stats before any
/// upgrades are applied.
#[derive(Clone, Debug)]
pub struct ShipPhysics {
    pub mass: f64,
    /// Size in the world, in pixels
    pub width: f64,
    pub height: f64,
    pub max_thrust_force: f64,
    pub max_turning_torque: f64,
    /// Linear air drag coefficient (see SHIP_DRAG_COEFFICIENT)
    pub drag_coefficient: f64,
    pub spoiler_drag_multiplier: f64,
    pub angular_drag_coefficient: f64,
    pub stabiliser_drag_coefficient: f64,
    /// None = unlimited
    pub max_fuel: Option<f64>,
}

impl Default for ShipPhysics {
    fn default() -> Self {
        ShipPhysics {
            mass: SHIP_MASS,
            width: SHIP_WIDTH as f64,
            height: SHIP_HEIGHT as f64,
            max_thrust_force: SHIP_MAX_THRUST_FORCE,
            max_turning_torque: SHIP_MAX_TURNING_TORQUE,
            drag_coefficient: SHIP_DRAG_COEFFICIENT,
            spoiler_drag_multiplier: SHIP_SPOILER_DRAG_MULTIPLIER,
            angular_drag_coefficient: SHIP_ANGULAR_DRAG_COEFFICIENT,
            stabiliser_drag_coefficient: SHIP_STABILISER_DRAG_COEFFICIENT,
            max_fuel: None,
        }
    }
}

impl ShipPhysics {
    /// Resistance to turning: a flat rectangle spinning about its centre,
    /// I = m (w^2 + h^2) / 12
    pub fn moment_of_inertia(&self) -> f64 {
        self.mass * (self.width * self.width + self.height * self.height) / 12.0
    }

    /// Distance from the ship's centre to its nose, where the gun is
    pub fn nose_distance(&self) -> f64 {
        self.height / 2.0
    }
}

/// Shield and hull. The shield regenerates after a quiet spell; the hull
/// never does.
#[derive(Clone, Debug)]
pub struct ShipHealth {
    pub max_shield: f64,
    pub max_hull: f64,
    pub shield: f64,
    pub hull: f64,
    /// Shield points regained per second
    pub shield_regen_rate: f64,
    /// Seconds without damage before the shield starts to regenerate
    pub shield_regen_delay: f64,
    time_since_damage: f64,
}

impl ShipHealth {
    pub fn new(max_shield: f64, max_hull: f64) -> Self {
        ShipHealth {
            max_shield,
            max_hull,
            shield: max_shield,
            hull: max_hull,
            shield_regen_rate: SHIP_SHIELD_REGEN_RATE,
            shield_regen_delay: SHIP_SHIELD_REGEN_DELAY,
            time_since_damage: 0.0,
        }
    }

    /// Any hit - even one the shield absorbs completely, or one that lands
    /// with the shield already empty - restarts the wait before the shield
    /// regenerates.
    pub fn take_damage(&mut self, mut amount: f64) {
        self.time_since_damage = 0.0;
        if self.shield > 0.0 {
            let absorbed = self.shield.min(amount);
            self.shield -= absorbed;
            amount -= absorbed;
        }
        self.hull = (self.hull - amount).max(0.0);
    }

    /// Back to full shield and hull (a fresh ship after a respawn)
    pub fn restore(&mut self) {
        self.shield = self.max_shield;
        self.hull = self.max_hull;
        self.time_since_damage = 0.0;
    }

    pub fn update(&mut self, dt: f64) {
        self.time_since_damage += dt;
        if self.time_since_damage >= self.shield_regen_delay {
            self.shield = self.max_shield.min(self.shield + self.shield_regen_rate * dt);
        }
    }

    pub fn is_alive(&self) -> bool {
        self.hull > 0.0
    }

    pub fn shield_fraction(&self) -> f64 {
        if self.max_shield > 0.0 { self.shield / self.max_shield } else { 0.0 }
    }

    pub fn hull_fraction(&self) -> f64 {
        if self.max_hull > 0.0 { self.hull / self.max_hull } else { 0.0 }
    }
}

/// Current input state for a ship this frame.
///
/// The controls are ANALOG so every input device drives the ship the same
/// way: a keyboard only ever produces 0 / 1 (or -1 / 1), while a game
/// controller stick produces anything in between.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShipInput {
    /// 0..1: engine throttle
    pub thrust: f64,
    /// -1..1: negative turns left, positive turns right
    pub rotation: f64,
    /// 0..1: how far the spoiler (air brake) is deployed
    pub spoiler: f64,
    pub fire: bool,
    pub pickup_cargo: bool,
}

impl ShipInput {
    /// Combine two devices controlling the same ship (e.g. keyboard and
    /// controller): for each analog control the stronger input wins, and
    /// buttons are pressed if either is. Neither device can cancel the other
    /// out.
    pub fn merged_with(&self, other: &ShipInput) -> ShipInput {
        fn stronger(a: f64, b: f64) -> f64 {
            if a.abs() >= b.abs() { a } else { b }
        }
        ShipInput {
            thrust: stronger(self.thrust, other.thrust),
            rotation: stronger(self.rotation, other.rotation),
            spoiler: stronger(self.spoiler, other.spoiler),
            fire: self.fire || other.fire,
            pickup_cargo: self.pickup_cargo || other.pickup_cargo,
        }
    }
}

pub struct Ship {
    // Position and orientation
    pub x: f64,
    pub y: f64,
    /// Radians, 0 = pointing up, increasing clockwise
    pub angle: f64,

    // Velocity
    pub vel_x: f64,
    pub vel_y: f64,
    /// Radians per second
    pub angular_velocity: f64,

    pub physics: ShipPhysics,
    pub health: ShipHealth,

    /// None = unlimited
    pub fuel: Option<f64>,
    pub is_landed: bool,
    /// Placeholder until the cargo system exists: what is being carried
    pub cargo: Option<u32>,

    pub weapon: Weapon,
    pub input: ShipInput,

    /// Pre-baked rotation masks, shared by ships with the same sprite
    pub mask_set: Option<Arc<ShipMaskSet>>,

    // Match state (set by the match rules, see match_rules)
    /// Countdown: only rotation works
    pub frozen: bool,
    /// Destroyed, waiting to respawn
    pub dead: bool,
    /// Seconds of damage immunity left (may be infinite)
    pub immune_time: f64,
}

impl Ship {
    pub fn new(x: f64, y: f64) -> Self {
        Ship::with_physics(x, y, ShipPhysics::default())
    }

    pub fn with_physics(x: f64, y: f64, physics: ShipPhysics) -> Self {
        let fuel = physics.max_fuel;
        Ship {
            x,
            y,
            angle: 0.0,
            vel_x: 0.0,
            vel_y: 0.0,
            angular_velocity: 0.0,
            physics,
            health: ShipHealth::new(SHIP_MAX_SHIELD, SHIP_MAX_HULL),
            fuel,
            is_landed: false,
            cargo: None,
            weapon: Weapon::new(ProjectileType::basic_bullet(), SHIP_FIRE_RATE),
            input: ShipInput::default(),
            mask_set: None,
            frozen: false,
            dead: false,
            immune_time: 0.0,
        }
    }

    // -------------------------
    // Physics update
    // -------------------------
    pub fn update(&mut self, dt: f64, gravity: f64) {
        if self.dead {
            return;
        }
        self.immune_time = (self.immune_time - dt).max(0.0);
        let turned = self.apply_steering(dt);
        if self.frozen {
            // Countdown: you can turn, but no engine, no gravity, no drift
            self.input.thrust = 0.0;
            self.health.update(dt);
            return;
        }
        // The engine pushes along the heading half way through the turn
        self.apply_translation(dt, gravity, self.angle - turned / 2.0);
        self.health.update(dt);
    }

    /// Steering torque against the spin drag. Returns how far the ship
    /// turned this frame (radians).
    fn apply_steering(&mut self, dt: f64) -> f64 {
        let stick = self.input.rotation.clamp(-1.0, 1.0);
        let inertia = self.physics.moment_of_inertia();

        // angular acceleration = torque / moment of inertia
        let angular_acceleration = self.physics.max_turning_torque * stick / inertia;

        // Spin drag always acts; while the player is not steering the
        // stabilising gyros add to it and bring the ship to rest
        let mut drag_coefficient = self.physics.angular_drag_coefficient;
        if stick.abs() <= SHIP_STEERING_DEAD_ZONE {
            drag_coefficient += self.physics.stabiliser_drag_coefficient;
        }

        let (spin, turned) = settle(self.angular_velocity, angular_acceleration,
                                    drag_coefficient / inertia, dt);
        self.angular_velocity = spin;
        self.angle = (self.angle + turned).rem_euclid(std::f64::consts::TAU);
        turned
    }

    /// Engine, gravity and air drag together change the velocity
    fn apply_translation(&mut self, dt: f64, gravity: f64, heading: f64) {
        // Engine: a force along the ship's facing, so acceleration =
        // force / mass. Angle 0 points up, so x uses sin and y uses -cos.
        let force = self.burn_engine(dt);
        let thrust_acceleration = force / self.physics.mass;
        let (fx, fy) = (heading.sin(), -heading.cos());

        // Gravity: weight is mass * g, so the acceleration is g whatever the
        // mass
        let gravity_acceleration = if self.is_landed { 0.0 } else { gravity };

        // Air drag: a linear force -b * v, so speed decays at rate b / mass.
        // Deploying the spoiler raises b, from the base value (stowed) up to
        // the base times the spoiler multiplier (fully deployed).
        let spoiler = self.input.spoiler.clamp(0.0, 1.0);
        let b = self.physics.drag_coefficient
            * (1.0 + spoiler * (self.physics.spoiler_drag_multiplier - 1.0));
        let drag_rate = b / self.physics.mass;

        let (vx, dx) = settle(self.vel_x, fx * thrust_acceleration, drag_rate, dt);
        let (vy, dy) = settle(self.vel_y, fy * thrust_acceleration + gravity_acceleration,
                              drag_rate, dt);
        self.vel_x = vx;
        self.vel_y = vy;
        self.x += dx;
        self.y += dy;
    }

    /// Run the engine for this frame: returns the force it produced (zero
    /// with the throttle closed or the tank empty) and burns the fuel
    fn burn_engine(&mut self, dt: f64) -> f64 {
        if matches!(self.fuel, Some(f) if f <= 0.0) {
            return 0.0;
        }
        let throttle = self.input.thrust.clamp(0.0, 1.0);
        if throttle <= 0.0 {
            return 0.0;
        }
        let force = self.physics.max_thrust_force * throttle;
        if let Some(fuel) = self.fuel.as_mut() {
            *fuel = (*fuel - force * dt * SHIP_FUEL_PER_THRUST_IMPULSE).max(0.0);
        }
        force
    }

    // -------------------------
    // Convenience
    // -------------------------
    pub fn speed(&self) -> f64 {
        self.vel_x.hypot(self.vel_y)
    }

    pub fn position(&self) -> (f64, f64) {
        (self.x, self.y)
    }

    /// The angle in degrees, for drawing
    pub fn angle_degrees(&self) -> f64 {
        self.angle.to_degrees()
    }

    /// Unit vector in the direction the ship is facing
    pub fn facing_vector(&self) -> (f64, f64) {
        (self.angle.sin(), -self.angle.cos())
    }

    /// World position of the gun - the ship's nose
    pub fn muzzle_position(&self) -> (f64, f64) {
        let (fx, fy) = self.facing_vector();
        let d = self.physics.nose_distance();
        (self.x + fx * d, self.y + fy * d)
    }

    pub fn is_alive(&self) -> bool {
        !self.dead && self.health.is_alive()
    }

    pub fn is_invulnerable(&self) -> bool {
        self.dead || self.frozen || self.immune_time > 0.0
    }

    /// In the timed protection after a respawn (not permanent)
    pub fn spawn_protected(&self) -> bool {
        self.immune_time > 0.0 && self.immune_time.is_finite()
    }

    /// Takes part in collisions (not frozen at a spawn, not wrecked)
    pub fn is_solid(&self) -> bool {
        !self.dead && !self.frozen
    }

    pub fn can_fire(&self) -> bool {
        self.is_alive() && !self.frozen
    }

    // -------------------------
    // Actions
    // -------------------------
    /// All damage goes through here so immunity applies to it
    pub fn take_damage(&mut self, amount: f64) {
        if !self.is_invulnerable() {
            self.health.take_damage(amount);
        }
    }

    /// Hold the ship still (countdown). It can still rotate.
    pub fn freeze(&mut self) {
        self.frozen = true;
        self.vel_x = 0.0;
        self.vel_y = 0.0;
    }

    /// Release the ship, immune to damage for `immunity` seconds
    pub fn unfreeze(&mut self, immunity: f64) {
        self.frozen = false;
        self.immune_time = immunity;
    }

    /// Wreck the ship where it is (hidden until respawn)
    pub fn kill(&mut self) {
        self.dead = true;
        self.vel_x = 0.0;
        self.vel_y = 0.0;
        self.angular_velocity = 0.0;
        self.input = ShipInput::default();
    }

    /// A fresh ship at (x, y): full health, pointing up, not moving
    pub fn respawn(&mut self, x: f64, y: f64) {
        self.x = x;
        self.y = y;
        self.angle = 0.0;
        self.vel_x = 0.0;
        self.vel_y = 0.0;
        self.angular_velocity = 0.0;
        self.health.restore();
        self.fuel = self.physics.max_fuel;
        self.cargo = None;
        self.dead = false;
        self.immune_time = 0.0;
        self.input = ShipInput::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f64 = 1.0 / 60.0;

    fn run(ship: &mut Ship, seconds: f64) {
        for _ in 0..(seconds / DT).round() as usize {
            ship.update(DT, GRAVITY);
        }
    }

    #[test]
    fn a_ship_in_free_fall_reaches_terminal_speed() {
        let mut ship = Ship::new(0.0, 0.0);
        run(&mut ship, 20.0);
        // gravity = drag force / mass at terminal speed: v = g * m / b
        let expected = GRAVITY * ship.physics.mass / ship.physics.drag_coefficient;
        assert!((ship.vel_y - expected).abs() < expected * 0.01, "{}", ship.vel_y);
        assert!(ship.vel_x.abs() < 1e-9);
    }

    #[test]
    fn gravity_pulls_every_mass_the_same() {
        let mut light = Ship::new(0.0, 0.0);
        light.physics.drag_coefficient = 0.0;
        let mut heavy = Ship::new(0.0, 0.0);
        heavy.physics.drag_coefficient = 0.0;
        heavy.physics.mass = 5.0;
        run(&mut light, 2.0);
        run(&mut heavy, 2.0);
        assert!((light.vel_y - heavy.vel_y).abs() < 1e-9);
        assert!((light.vel_y - GRAVITY * 2.0).abs() < 1e-6);
    }

    #[test]
    fn a_heavier_ship_accelerates_less_for_the_same_thrust() {
        let thrust = |mass: f64| {
            let mut s = Ship::new(0.0, 0.0);
            s.physics.mass = mass;
            s.is_landed = true; // no gravity, to isolate the engine
            s.physics.drag_coefficient = 0.0;
            s.input.thrust = 1.0;
            run(&mut s, 0.5);
            s.speed()
        };
        let (light, heavy) = (thrust(1.0), thrust(2.0));
        assert!((light - 750.0 * 0.5).abs() < 1.0, "{light}");
        assert!((heavy - light / 2.0).abs() < 1.0, "{heavy}");
    }

    #[test]
    fn thrust_goes_the_way_the_nose_points() {
        let mut s = Ship::new(0.0, 0.0);
        s.is_landed = true;
        s.input.thrust = 1.0;
        run(&mut s, 0.2); // pointing up
        assert!(s.vel_y < 0.0 && s.vel_x.abs() < 1e-9);
        let mut s = Ship::new(0.0, 0.0);
        s.is_landed = true;
        s.angle = std::f64::consts::FRAC_PI_2; // a quarter turn clockwise: right
        s.input.thrust = 1.0;
        s.update(DT, GRAVITY);
        assert!(s.vel_x > 0.0 && s.vel_y.abs() < 1e-6);
    }

    #[test]
    fn spoiler_brakes_harder_and_half_is_in_between() {
        let speed_after = |spoiler: f64| {
            let mut s = Ship::new(0.0, 0.0);
            s.is_landed = true;
            s.vel_x = 500.0;
            s.input.spoiler = spoiler;
            run(&mut s, 1.0);
            s.vel_x
        };
        let (open, half, full) = (speed_after(0.0), speed_after(0.5), speed_after(1.0));
        assert!(full < half && half < open);
        assert!((open - 500.0 * (-0.7f64).exp()).abs() < 0.5, "{open}");
        assert!((full - 500.0 * 0.2).abs() < 3.0, "{full}"); // about a fifth
    }

    #[test]
    fn steering_accelerates_the_spin_and_the_stabiliser_stops_it() {
        let mut s = Ship::new(0.0, 0.0);
        s.is_landed = true;
        s.input.rotation = 1.0;
        run(&mut s, 0.5);
        assert!(s.angular_velocity > 3.0, "{}", s.angular_velocity);
        s.input.rotation = 0.0;
        run(&mut s, 1.0);
        assert!(s.angular_velocity.abs() < 0.01, "{}", s.angular_velocity);
        // steering the other way turns the other way
        s.input.rotation = -1.0;
        run(&mut s, 0.2);
        assert!(s.angular_velocity < 0.0);
    }

    #[test]
    fn a_frozen_ship_rotates_but_does_not_move_or_fall() {
        let mut s = Ship::new(100.0, 100.0);
        s.freeze();
        s.input.thrust = 1.0;
        s.input.rotation = 1.0;
        run(&mut s, 1.0);
        assert_eq!((s.x, s.y), (100.0, 100.0));
        assert_eq!((s.vel_x, s.vel_y), (0.0, 0.0));
        assert!(s.angle != 0.0);
        assert_eq!(s.input.thrust, 0.0); // no engine flame either
    }

    #[test]
    fn speed_is_the_same_at_any_frame_rate() {
        let speed_after = |fps: f64| {
            let mut s = Ship::new(0.0, 0.0);
            s.input.thrust = 1.0;
            s.angle = 0.7;
            for _ in 0..(fps as usize * 2) { s.update(1.0 / fps, GRAVITY); }
            (s.vel_x, s.vel_y)
        };
        let (a, b) = (speed_after(30.0), speed_after(240.0));
        assert!((a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6, "{a:?} {b:?}");
    }

    #[test]
    fn spin_is_the_same_at_any_frame_rate() {
        let spin_after = |fps: f64| {
            let mut s = Ship::new(0.0, 0.0);
            s.is_landed = true;
            s.input.rotation = 1.0;
            for _ in 0..(fps as usize) { s.update(1.0 / fps, GRAVITY); }
            s.angular_velocity
        };
        assert!((spin_after(30.0) - spin_after(240.0)).abs() < 1e-9);
    }

    #[test]
    fn limited_fuel_runs_out() {
        let mut physics = ShipPhysics::default();
        physics.max_fuel = Some(1000.0);
        let mut s = Ship::with_physics(0.0, 0.0, physics);
        s.input.thrust = 1.0;
        run(&mut s, 3.0);
        assert_eq!(s.fuel, Some(0.0));
        let speed = s.speed();
        s.update(DT, 0.0);
        assert!(s.speed() < speed); // no more thrust: drag only slows it
    }

    #[test]
    fn immunity_blocks_damage_and_a_hit_restarts_the_shield_wait() {
        let mut s = Ship::new(0.0, 0.0);
        s.unfreeze(3.0);
        s.take_damage(50.0);
        assert_eq!(s.health.shield, 100.0);
        run(&mut s, 3.1);
        s.take_damage(30.0);
        assert_eq!(s.health.shield, 70.0);
        let mut h = s.health.clone();
        h.update(2.9);
        assert_eq!(h.shield, 70.0);
        h.update(0.2);
        assert!(h.shield > 70.0);
        // hull never regenerates, and a hit with the shield empty resets the wait
        let mut h = ShipHealth::new(100.0, 100.0);
        h.take_damage(100.0);
        h.update(2.9);
        h.take_damage(5.0);
        assert_eq!(h.hull, 95.0);
        h.update(2.9);
        assert_eq!(h.shield, 0.0);
        h.update(10.0);
        assert!(h.shield > 0.0 && h.hull == 95.0);
    }

    #[test]
    fn moment_of_inertia_of_the_default_ship() {
        let i = ShipPhysics::default().moment_of_inertia();
        assert!((i - (48.0 * 48.0 + 32.0 * 32.0) / 12.0).abs() < 1e-9);
    }

    #[test]
    fn merged_input_takes_the_stronger_of_each_control() {
        let a = ShipInput { thrust: 1.0, rotation: -0.3, ..Default::default() };
        let b = ShipInput { thrust: 0.2, rotation: 0.8, fire: true, ..Default::default() };
        let m = a.merged_with(&b);
        assert_eq!((m.thrust, m.rotation, m.fire), (1.0, 0.8, true));
    }

    #[test]
    fn respawn_gives_a_fresh_ship() {
        let mut s = Ship::new(5.0, 5.0);
        s.health.take_damage(500.0);
        s.kill();
        assert!(!s.is_alive() && s.dead);
        s.respawn(10.0, 20.0);
        assert!(s.is_alive() && !s.dead);
        assert_eq!((s.x, s.y, s.angle), (10.0, 20.0, 0.0));
        assert_eq!(s.health.hull, 100.0);
    }
}
