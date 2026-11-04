//! Explosions: a circle that grows to its full size and shrinks away. Purely
//! visual - nothing here affects the physics. The game crate draws whatever
//! is in `explosions`.

use crate::constants::*;

pub struct Explosion {
    pub x: f64,
    pub y: f64,
    pub max_radius: f64,
    pub duration: f64,
    /// The fraction of its life at which it is biggest
    pub peak_at: f64,
    pub age: f64,
}

impl Explosion {
    /// 0.0 at the start of its life, 1.0 at the end
    pub fn progress(&self) -> f64 {
        (self.age / self.duration).min(1.0)
    }

    pub fn alive(&self) -> bool {
        self.age < self.duration
    }

    /// Current radius in world pixels. Eased, so it bursts out quickly and
    /// then shrinks away more slowly.
    pub fn radius(&self) -> f64 {
        let p = self.progress();
        if p <= 0.0 || p >= 1.0 {
            return 0.0;
        }
        if p < self.peak_at {
            self.max_radius * (p / self.peak_at * std::f64::consts::FRAC_PI_2).sin()
        } else {
            let u = (p - self.peak_at) / (1.0 - self.peak_at);
            self.max_radius * (u * std::f64::consts::FRAC_PI_2).cos()
        }
    }
}

#[derive(Default)]
pub struct ExplosionSystem {
    pub explosions: Vec<Explosion>,
}

impl ExplosionSystem {
    pub fn new() -> Self {
        ExplosionSystem::default()
    }

    /// Start an explosion. A radius and duration of None mean the usual ones.
    pub fn spawn(&mut self, x: f64, y: f64, max_radius: Option<f64>, duration: Option<f64>) {
        if self.explosions.len() >= MAX_EXPLOSIONS {
            self.explosions.remove(0); // drop the oldest
        }
        self.explosions.push(Explosion {
            x,
            y,
            max_radius: max_radius.unwrap_or(EXPLOSION_RADIUS),
            duration: duration.unwrap_or(EXPLOSION_DURATION),
            peak_at: EXPLOSION_PEAK_AT,
            age: 0.0,
        });
    }

    pub fn update(&mut self, dt: f64) {
        for e in self.explosions.iter_mut() {
            e.age += dt;
        }
        self.explosions.retain(|e| e.alive());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grows_to_full_size_then_shrinks_away() {
        let mut s = ExplosionSystem::new();
        s.spawn(0.0, 0.0, Some(10.0), Some(1.0));
        let r = |s: &ExplosionSystem| s.explosions[0].radius();
        assert_eq!(r(&s), 0.0);
        s.update(0.35);
        assert!((r(&s) - 10.0).abs() < 1e-9);
        s.update(0.30);
        assert!(r(&s) < 10.0 && r(&s) > 0.0);
        s.update(0.5);
        assert!(s.explosions.is_empty());
    }

    #[test]
    fn the_oldest_is_dropped_at_the_cap() {
        let mut s = ExplosionSystem::new();
        for i in 0..MAX_EXPLOSIONS + 5 {
            s.spawn(i as f64, 0.0, None, None);
        }
        assert_eq!(s.explosions.len(), MAX_EXPLOSIONS);
        assert_eq!(s.explosions[0].x, 5.0);
    }
}
