//! Turning the keyboard into each ship's controls, using the player's
//! settings.

use gravstar_core::settings::{Action, PlayerInputSettings};
use gravstar_core::ship::ShipInput;
use sdl2::keyboard::{KeyboardState, Scancode};

/// One player's controls
pub struct KeyBindings {
    /// By action, in the order of Action::ALL; None = unbound
    keys: [Option<Scancode>; 5],
}

impl KeyBindings {
    pub fn from_settings(settings: &PlayerInputSettings) -> KeyBindings {
        let key = |action: Action| {
            settings.keyboard[action.index()].as_deref().and_then(Scancode::from_name)
        };
        KeyBindings { keys: Action::ALL.map(key) }
    }

    /// What this player's keys are asking the ship to do right now
    pub fn read(&self, keys: &KeyboardState) -> ShipInput {
        let held = |action: Action| self.keys[action.index()].map_or(false, |k| keys.is_scancode_pressed(k));
        let axis = |on: bool| if on { 1.0 } else { 0.0 };
        ShipInput {
            thrust: axis(held(Action::ThrustForward)),
            spoiler: axis(held(Action::DeploySpoiler)),
            rotation: axis(held(Action::RotateRight)) - axis(held(Action::RotateLeft)),
            fire: held(Action::Fire),
            pickup_cargo: false,
        }
    }
}
