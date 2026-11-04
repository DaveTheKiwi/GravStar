//! A ship's gun. It owns the fire-rate limiting only - it does not create
//! bullets itself (the projectile system does that when update() says fire).

use crate::projectile::ProjectileType;

pub struct Weapon {
    pub projectile_type: ProjectileType,
    pub shots_per_second: f64,
    /// Seconds until the next shot is allowed
    cooldown: f64,
}

impl Weapon {
    pub fn new(projectile_type: ProjectileType, shots_per_second: f64) -> Self {
        Weapon { projectile_type, shots_per_second, cooldown: 0.0 }
    }

    pub fn fire_interval(&self) -> f64 {
        1.0 / self.shots_per_second.max(1e-6)
    }

    /// Advance the cooldown. Returns true if a shot should be fired now.
    /// Holding the trigger fires continuously at the weapon's rate; tapping
    /// faster than that cannot exceed it. At most one frame of unused
    /// cooldown is banked, so there is no burst after sitting idle, and the
    /// average rate stays exact whatever the frame rate.
    pub fn update(&mut self, dt: f64, trigger_held: bool) -> bool {
        self.cooldown = (self.cooldown - dt).max(-dt);
        if trigger_held && self.cooldown <= 0.0 {
            self.cooldown += self.fire_interval();
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_is_exact_at_any_frame_rate() {
        for fps in [30.0, 60.0, 144.0, 240.0] {
            let mut w = Weapon::new(ProjectileType::basic_bullet(), 4.0);
            let dt = 1.0 / fps;
            let shots = (0..(fps as usize * 10)).filter(|_| w.update(dt, true)).count();
            assert!((39..=41).contains(&shots), "{fps}: {shots}");
        }
    }

    #[test]
    fn no_burst_after_idle() {
        let mut w = Weapon::new(ProjectileType::basic_bullet(), 4.0);
        for _ in 0..600 {
            w.update(1.0 / 60.0, false);
        }
        let shots = (0..15).filter(|_| w.update(1.0 / 60.0, true)).count();
        assert_eq!(shots, 1);
    }
}
