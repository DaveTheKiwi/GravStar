//! Game controllers: turning what a controller is doing into ship controls,
//! finding which control the player just pressed (for rebinding), and sharing
//! connected controllers out between the players. Pure logic - the game
//! crate does the actual reading of the hardware and hands in snapshots.

use std::collections::HashMap;

use crate::constants::*;
use crate::settings::Binding;
use crate::ship::ShipInput;

/// A snapshot of everything one controller is doing right now
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ControllerState {
    /// -1.0 to 1.0
    pub axes: Vec<f64>,
    pub buttons: Vec<bool>,
    /// (x, y) for each D-pad, each -1, 0 or 1; up is y = 1
    pub hats: Vec<(i8, i8)>,
}

impl ControllerState {
    pub fn axis(&self, index: usize) -> f64 {
        self.axes.get(index).copied().unwrap_or(0.0)
    }

    pub fn button(&self, index: usize) -> bool {
        self.buttons.get(index).copied().unwrap_or(false)
    }

    pub fn hat(&self, index: usize) -> (i8, i8) {
        self.hats.get(index).copied().unwrap_or((0, 0))
    }
}

// -------------------------
// Reading a binding
// -------------------------

/// How strongly a binding is pressed right now, 0.0 to 1.0. Buttons and
/// D-pad directions are 0 or 1; a stick or trigger gives everything between.
pub fn binding_activation(binding: Option<&Binding>, state: &ControllerState) -> f64 {
    match binding {
        None => 0.0,
        Some(Binding::Button { index }) => if state.button(*index as usize) { 1.0 } else { 0.0 },
        Some(Binding::Hat { index, axis, direction }) => {
            let (x, y) = state.hat(*index as usize);
            let value = if *axis == 0 { x } else { y };
            if value == *direction { 1.0 } else { 0.0 }
        }
        Some(Binding::Axis { index, direction, rest }) => {
            // Measured from its resting value towards the extreme in the
            // bound direction, so a trigger resting at -1 works as well as
            // a centred stick
            let direction = *direction as f64;
            let span = 1.0 - direction * rest;
            if span < CONTROLLER_MIN_AXIS_SPAN {
                return 0.0;
            }
            (direction * (state.axis(*index as usize) - rest) / span).clamp(0.0, 1.0)
        }
    }
}

/// Ignore anything up to the deadzone, and rescale what is left to run from
/// 0 to 1, so the output starts from zero right at the edge of the deadzone
/// instead of jumping
pub fn apply_deadzone(value: f64, deadzone: f64) -> f64 {
    let deadzone = deadzone.clamp(0.0, CONTROLLER_MAX_DEADZONE);
    let magnitude = value.abs();
    if magnitude <= deadzone {
        return 0.0;
    }
    ((magnitude - deadzone) / (1.0 - deadzone)).min(1.0).copysign(value)
}

/// Shape a value, keeping its sign. 1.0 leaves it unchanged; higher gives
/// finer control near the centre.
pub fn apply_curve(value: f64, exponent: f64) -> f64 {
    value.abs().powf(exponent).copysign(value)
}

/// A controller's state as analog ship controls, using the player's
/// bindings (one per action, in the order of settings::Action::ALL: thrust,
/// brake, left, right, fire). Rotation is right minus left, so one stick
/// axis bound both ways gives a smooth -1 to 1.
pub fn controller_to_ship_input(bindings: &[Option<Binding>; 5], state: &ControllerState,
                                deadzone: f64, curve: f64) -> ShipInput {
    let level = |i: usize| {
        apply_curve(apply_deadzone(binding_activation(bindings[i].as_ref(), state), deadzone), curve)
    };
    ShipInput {
        thrust: level(0),
        spoiler: level(1),
        rotation: (level(3) - level(2)).clamp(-1.0, 1.0),
        // buttons count as pressed once more than half way (a trigger can fire)
        fire: binding_activation(bindings[4].as_ref(), state) >= CONTROLLER_PRESSED_LEVEL,
        pickup_cargo: false,
    }
}

// -------------------------
// Rebinding
// -------------------------

/// Compare what the controller is doing now with how it looked when the
/// prompt appeared and return the binding for whatever changed (None if
/// nothing has yet). Buttons take priority over the D-pad, which takes
/// priority over sticks and triggers.
pub fn detect_binding(baseline: &ControllerState, current: &ControllerState) -> Option<Binding> {
    for (i, pressed) in current.buttons.iter().enumerate() {
        if *pressed && !baseline.button(i) {
            return Some(Binding::Button { index: i as u8 });
        }
    }
    for (i, (x, y)) in current.hats.iter().enumerate() {
        let (base_x, base_y) = baseline.hat(i);
        if *x != 0 && *x != base_x {
            return Some(Binding::Hat { index: i as u8, axis: 0, direction: *x });
        }
        if *y != 0 && *y != base_y {
            return Some(Binding::Hat { index: i as u8, axis: 1, direction: *y });
        }
    }
    for (i, value) in current.axes.iter().enumerate() {
        let base = baseline.axis(i);
        let delta = value - base;
        if delta.abs() >= CONTROLLER_DETECT_THRESHOLD {
            let rest = if base.abs() >= CONTROLLER_TRIGGER_REST_MIN { base.signum() } else { 0.0 };
            return Some(Binding::Axis { index: i as u8, direction: if delta > 0.0 { 1 } else { -1 }, rest });
        }
    }
    None
}

// -------------------------
// Sharing controllers out
// -------------------------

/// Controllers go to the players who have chosen "controller", in the order
/// they connect. A player keeps their controller when another is unplugged.
pub struct ControllerRouter {
    /// Connected controllers' ids, in connection order
    order: Vec<u32>,
    names: HashMap<u32, String>,
    /// The controller each player is using
    slots: Vec<Option<u32>>,
}

impl ControllerRouter {
    pub fn new(players: usize) -> Self {
        ControllerRouter { order: Vec::new(), names: HashMap::new(), slots: vec![None; players] }
    }

    /// A controller was plugged in. Returns a message for the player, or
    /// None if this one is already known (the system can report a
    /// controller twice).
    pub fn connect(&mut self, id: u32, name: &str) -> Option<String> {
        if self.names.contains_key(&id) {
            return None;
        }
        self.names.insert(id, name.to_string());
        self.order.push(id);
        Some(format!("Controller connected: {name}"))
    }

    pub fn disconnect(&mut self, id: u32) -> Option<String> {
        let name = self.names.remove(&id)?;
        self.order.retain(|o| *o != id);
        Some(format!("Controller disconnected: {name}"))
    }

    /// Share the controllers out to the players who want one (`wants[i]`).
    /// Returns messages about anyone who got a different controller.
    pub fn rebalance(&mut self, wants: &[bool]) -> Vec<String> {
        let before = self.slots.clone();
        for (player, slot) in self.slots.iter_mut().enumerate() {
            if let Some(id) = *slot {
                if !wants.get(player).copied().unwrap_or(false) || !self.names.contains_key(&id) {
                    *slot = None;
                }
            }
        }
        let mut taken: Vec<u32> = self.slots.iter().flatten().copied().collect();
        for id in self.order.clone() {
            if taken.contains(&id) {
                continue;
            }
            for player in 0..self.slots.len() {
                if self.slots[player].is_none() && wants.get(player).copied().unwrap_or(false) {
                    self.slots[player] = Some(id);
                    taken.push(id);
                    break;
                }
            }
        }
        let mut messages = Vec::new();
        for (player, slot) in self.slots.iter().enumerate() {
            if let Some(id) = slot {
                if *slot != before[player] {
                    messages.push(format!("Player {} is using controller: {}", player + 1, self.names[id]));
                }
            }
        }
        messages
    }

    /// The controller a player is using
    pub fn controller_for(&self, player: usize) -> Option<u32> {
        self.slots.get(player).copied().flatten()
    }

    pub fn name_for(&self, player: usize) -> Option<&str> {
        self.controller_for(player).and_then(|id| self.names.get(&id)).map(|n| n.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stick_up() -> Binding {
        Binding::Axis { index: 1, direction: -1, rest: 0.0 }
    }

    fn state(axes: &[f64], buttons: &[bool], hats: &[(i8, i8)]) -> ControllerState {
        ControllerState { axes: axes.to_vec(), buttons: buttons.to_vec(), hats: hats.to_vec() }
    }

    #[test]
    fn activation_of_buttons_hats_sticks_and_triggers() {
        let s = state(&[0.0, -0.8, -1.0], &[false, true], &[(0, 1)]);
        assert_eq!(binding_activation(Some(&Binding::Button { index: 1 }), &s), 1.0);
        assert_eq!(binding_activation(Some(&Binding::Button { index: 0 }), &s), 0.0);
        assert_eq!(binding_activation(Some(&Binding::Hat { index: 0, axis: 1, direction: 1 }), &s), 1.0);
        assert_eq!(binding_activation(Some(&Binding::Hat { index: 0, axis: 1, direction: -1 }), &s), 0.0);
        assert!((binding_activation(Some(&stick_up()), &s) - 0.8).abs() < 1e-9);
        assert_eq!(binding_activation(Some(&Binding::Axis { index: 1, direction: 1, rest: 0.0 }), &s), 0.0);
        // a trigger resting at -1: untouched = 0, pulled to +1 = 1, half way = 0.5
        let trigger = Binding::Axis { index: 2, direction: 1, rest: -1.0 };
        assert_eq!(binding_activation(Some(&trigger), &s), 0.0);
        assert_eq!(binding_activation(Some(&trigger), &state(&[0.0, 0.0, 1.0], &[], &[])), 1.0);
        assert!((binding_activation(Some(&trigger), &state(&[0.0, 0.0, 0.0], &[], &[])) - 0.5).abs() < 1e-9);
        // unbound, and a control the controller does not have
        assert_eq!(binding_activation(None, &s), 0.0);
        assert_eq!(binding_activation(Some(&Binding::Button { index: 40 }), &s), 0.0);
        assert_eq!(binding_activation(Some(&Binding::Axis { index: 9, direction: 1, rest: 0.0 }), &s), 0.0);
        // an axis at its far end already cannot be pushed further that way
        assert_eq!(binding_activation(Some(&Binding::Axis { index: 0, direction: 1, rest: 1.0 }), &s), 0.0);
    }

    #[test]
    fn the_deadzone_rescales_what_is_left() {
        assert_eq!(apply_deadzone(0.10, 0.15), 0.0);
        assert_eq!(apply_deadzone(-0.15, 0.15), 0.0);
        assert!((apply_deadzone(0.575, 0.15) - 0.5).abs() < 1e-9);
        assert_eq!(apply_deadzone(1.0, 0.15), 1.0);
        assert!((apply_deadzone(-0.575, 0.15) + 0.5).abs() < 1e-9);
        assert_eq!(apply_deadzone(0.5, 5.0), 0.0); // a silly deadzone is held below 1
    }

    #[test]
    fn the_curve_keeps_the_sign_and_softens_small_movements() {
        assert_eq!(apply_curve(0.5, 1.0), 0.5);
        assert_eq!(apply_curve(-0.5, 2.0), -0.25);
        assert_eq!(apply_curve(1.0, 2.0), 1.0);
    }

    #[test]
    fn a_stick_drives_the_ship_smoothly() {
        let pad = crate::settings::PlayerInputSettings::new(0).controller;
        let idle = controller_to_ship_input(&pad, &state(&[0.0, 0.0], &[false], &[]), 0.15, 1.0);
        assert_eq!(idle, ShipInput::default());
        let go = controller_to_ship_input(&pad, &state(&[0.5, -1.0], &[true], &[]), 0.15, 1.0);
        assert_eq!(go.thrust, 1.0);
        assert!(go.rotation > 0.3 && go.rotation < 0.5, "{}", go.rotation);
        assert!(go.fire && go.spoiler == 0.0);
        let back = controller_to_ship_input(&pad, &state(&[-1.0, 1.0], &[], &[]), 0.15, 1.0);
        assert_eq!((back.rotation, back.spoiler, back.thrust), (-1.0, 1.0, 0.0));
        // a trigger can fire once pressed past half way
        let mut with_trigger = pad.clone();
        with_trigger[4] = Some(Binding::Axis { index: 5, direction: 1, rest: -1.0 });
        let half = controller_to_ship_input(&with_trigger, &state(&[0.0, 0.0, 0.0, 0.0, 0.0, -0.5], &[], &[]), 0.15, 1.0);
        let full = controller_to_ship_input(&with_trigger, &state(&[0.0, 0.0, 0.0, 0.0, 0.0, 1.0], &[], &[]), 0.15, 1.0);
        assert!(!half.fire && full.fire);
    }

    #[test]
    fn rebinding_notices_what_changed_not_what_was_already_held() {
        let base = state(&[0.0, 0.0, -1.0], &[true, false], &[(0, 0)]);
        assert_eq!(detect_binding(&base, &base), None);
        // button 0 was already held when the prompt appeared: ignored
        let b = state(&[0.0, 0.0, -1.0], &[true, true], &[(0, 0)]);
        assert_eq!(detect_binding(&base, &b), Some(Binding::Button { index: 1 }));
        // the D-pad, ahead of sticks
        let d = state(&[0.0, 0.9, -1.0], &[true, false], &[(-1, 0)]);
        assert_eq!(detect_binding(&base, &d), Some(Binding::Hat { index: 0, axis: 0, direction: -1 }));
        // a stick, with its rest value
        let s = state(&[0.0, -0.9, -1.0], &[true, false], &[(0, 0)]);
        assert_eq!(detect_binding(&base, &s), Some(Binding::Axis { index: 1, direction: -1, rest: 0.0 }));
        // a trigger pulled from -1 to +1 reports its resting value
        let t = state(&[0.0, 0.0, 1.0], &[true, false], &[(0, 0)]);
        assert_eq!(detect_binding(&base, &t), Some(Binding::Axis { index: 2, direction: 1, rest: -1.0 }));
        // a small nudge is not enough
        assert_eq!(detect_binding(&base, &state(&[0.4, 0.0, -1.0], &[true, false], &[(0, 0)])), None);
    }

    #[test]
    fn controllers_go_to_the_players_who_want_them_in_connection_order() {
        let mut r = ControllerRouter::new(2);
        assert!(r.connect(10, "Pad A").unwrap().contains("connected: Pad A"));
        assert_eq!(r.connect(10, "Pad A"), None); // a repeat report is ignored
        assert!(r.rebalance(&[false, false]).is_empty()); // nobody wants one
        assert_eq!(r.controller_for(0), None);
        // player 2 wants one, so the first pad goes to them
        let msg = r.rebalance(&[false, true]);
        assert_eq!(msg, vec!["Player 2 is using controller: Pad A".to_string()]);
        assert_eq!(r.name_for(1), Some("Pad A"));
        // a second pad, and now player 1 wants one too: they get the new one
        r.connect(11, "Pad B");
        r.rebalance(&[true, true]);
        assert_eq!((r.controller_for(0), r.controller_for(1)), (Some(11), Some(10)));
    }

    #[test]
    fn a_player_keeps_their_controller_when_another_is_unplugged() {
        let mut r = ControllerRouter::new(2);
        r.connect(1, "A");
        r.connect(2, "B");
        r.rebalance(&[true, true]);
        assert_eq!((r.controller_for(0), r.controller_for(1)), (Some(1), Some(2)));
        assert!(r.disconnect(1).unwrap().contains("disconnected: A"));
        assert_eq!(r.disconnect(1), None);
        r.rebalance(&[true, true]);
        assert_eq!((r.controller_for(0), r.controller_for(1)), (None, Some(2)));
        // plugged back in, it goes to the player who is without
        r.connect(3, "A again");
        r.rebalance(&[true, true]);
        assert_eq!((r.controller_for(0), r.controller_for(1)), (Some(3), Some(2)));
        // choosing the keyboard hands the controller back
        r.rebalance(&[false, true]);
        assert_eq!(r.controller_for(0), None);
    }
}
