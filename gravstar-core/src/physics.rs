//! Motion helpers shared by everything that moves.

/// A quantity that is pushed by a constant `acceleration` while a drag slows
/// it at `drag_rate` per second (dv/dt = acceleration - drag_rate * v). After
/// `dt` seconds, returns its new value and how far it has carried whatever
/// it moves (its integral over the step). This is solved exactly: the value
/// heads for the terminal value acceleration / drag_rate, and without drag
/// it is simply v + a * dt.
pub fn settle(value: f64, acceleration: f64, drag_rate: f64, dt: f64) -> (f64, f64) {
    if drag_rate < 1e-12 {
        return (value + acceleration * dt,
                value * dt + 0.5 * acceleration * dt * dt);
    }
    let terminal = acceleration / drag_rate;
    let decay = (-drag_rate * dt).exp();
    (terminal + (value - terminal) * decay,
     terminal * dt + (value - terminal) * (1.0 - decay) / drag_rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_drag_it_is_plain_constant_acceleration() {
        let (v, d) = settle(10.0, 4.0, 0.0, 2.0);
        assert!((v - 18.0).abs() < 1e-12);
        assert!((d - (10.0 * 2.0 + 0.5 * 4.0 * 4.0)).abs() < 1e-12);
    }

    #[test]
    fn heads_for_the_terminal_value() {
        let (v, _) = settle(0.0, 250.0, 0.7, 60.0);
        assert!((v - 250.0 / 0.7).abs() < 1e-9);
    }

    #[test]
    fn a_coasting_value_decays_exponentially() {
        let (v, d) = settle(100.0, 0.0, 2.0, 1.0);
        assert!((v - 100.0 * (-2.0f64).exp()).abs() < 1e-9);
        assert!((d - 100.0 * (1.0 - (-2.0f64).exp()) / 2.0).abs() < 1e-9);
    }

    #[test]
    fn many_small_steps_equal_one_big_step() {
        let (mut v, mut x) = (30.0, 0.0);
        for _ in 0..1000 {
            let (nv, d) = settle(v, 80.0, 1.3, 0.003);
            v = nv;
            x += d;
        }
        let (bv, bx) = settle(30.0, 80.0, 1.3, 3.0);
        assert!((v - bv).abs() < 1e-9 && (x - bx).abs() < 1e-9);
    }
}
