//! Reading game controllers through SDL's joystick interface (the same
//! numbered axes, buttons and D-pads the Python version used, so saved
//! bindings keep their meaning), and plugging them in and out while the game
//! runs. What the readings mean is decided in gravstar-core.

use std::collections::HashMap;

use gravstar_core::controllers::{controller_to_ship_input, detect_binding, ControllerRouter, ControllerState};
use gravstar_core::settings::{Binding, Device, PlayerInputSettings};
use gravstar_core::ship::ShipInput;
use sdl2::event::Event;
use sdl2::joystick::{HatState, Joystick};
use sdl2::JoystickSubsystem;

struct Pad {
    joystick: Joystick,
}

impl Pad {
    /// What this controller is doing right now
    fn snapshot(&self) -> ControllerState {
        let j = &self.joystick;
        ControllerState {
            axes: (0..j.num_axes()).map(|i| j.axis(i).map_or(0.0, |v| v as f64 / 32768.0)).collect(),
            buttons: (0..j.num_buttons()).map(|i| j.button(i).unwrap_or(false)).collect(),
            hats: (0..j.num_hats()).map(|i| hat_direction(j.hat(i).unwrap_or(HatState::Centered))).collect(),
        }
    }
}

/// A D-pad position as (x, y) with right = +1 and up = +1
fn hat_direction(hat: HatState) -> (i8, i8) {
    match hat {
        HatState::Centered => (0, 0),
        HatState::Up => (0, 1),
        HatState::RightUp => (1, 1),
        HatState::Right => (1, 0),
        HatState::RightDown => (1, -1),
        HatState::Down => (0, -1),
        HatState::LeftDown => (-1, -1),
        HatState::Left => (-1, 0),
        HatState::LeftUp => (-1, 1),
    }
}

/// A rebind is waiting for a press on this player's controller
struct Capture {
    player: usize,
    /// How the controller looked when the prompt appeared; whatever was
    /// already held then is ignored
    baseline: ControllerState,
}

pub struct ControllerHub {
    subsystem: JoystickSubsystem,
    /// Open controllers, by SDL's instance id
    pads: HashMap<u32, Pad>,
    router: ControllerRouter,
    capture: Option<Capture>,
}

impl ControllerHub {
    /// Opens every controller already connected
    pub fn new(subsystem: JoystickSubsystem, players: usize) -> Result<Self, String> {
        let count = subsystem.num_joysticks()?;
        let mut hub = ControllerHub {
            subsystem,
            pads: HashMap::new(),
            router: ControllerRouter::new(players),
            capture: None,
        };
        for index in 0..count {
            hub.connect(index);
        }
        Ok(hub)
    }

    fn connect(&mut self, device_index: u32) -> Option<String> {
        let joystick = self.subsystem.open(device_index).ok()?;
        let id = joystick.instance_id();
        let name = joystick.name();
        let message = self.router.connect(id, &name);
        self.pads.entry(id).or_insert(Pad { joystick });
        message
    }

    /// Controllers being plugged in and out. Returns anything worth telling
    /// the player. `players` are their current settings, which decide who
    /// gets which controller.
    pub fn handle_event(&mut self, event: &Event, players: &[PlayerInputSettings]) -> Vec<String> {
        let mut messages = Vec::new();
        match event {
            Event::JoyDeviceAdded { which, .. } => messages.extend(self.connect(*which)),
            Event::JoyDeviceRemoved { which, .. } => {
                self.pads.remove(which);
                messages.extend(self.router.disconnect(*which));
            }
            _ => return messages,
        }
        messages.extend(self.apply_settings(players));
        messages
    }

    /// Call after a player changes their device
    pub fn apply_settings(&mut self, players: &[PlayerInputSettings]) -> Vec<String> {
        let wants: Vec<bool> = players.iter().map(|p| p.device == Device::Controller).collect();
        self.router.rebalance(&wants)
    }

    /// This frame's ship controls for a player using a controller (none
    /// if they have no controller connected)
    pub fn read(&self, player: usize, settings: &PlayerInputSettings) -> ShipInput {
        let Some(pad) = self.router.controller_for(player).and_then(|id| self.pads.get(&id)) else {
            return ShipInput::default();
        };
        controller_to_ship_input(&settings.controller, &pad.snapshot(), settings.deadzone, settings.response_curve)
    }

    /// The name of each player's controller, if they have one
    pub fn names(&self, players: usize) -> Vec<Option<String>> {
        (0..players).map(|p| self.router.name_for(p).map(|n| n.to_string())).collect()
    }

    /// Call every frame. `waiting_for` is the player a rebind is waiting on
    /// (None when no controller rebind is under way). Returns the binding
    /// once the player presses something.
    pub fn poll_capture(&mut self, waiting_for: Option<usize>) -> Option<Binding> {
        let Some(player) = waiting_for else {
            self.capture = None;
            return None;
        };
        let pad = self.router.controller_for(player).and_then(|id| self.pads.get(&id))?;
        let current = pad.snapshot();
        match &self.capture {
            Some(c) if c.player == player => {
                let found = detect_binding(&c.baseline, &current);
                if found.is_some() {
                    self.capture = None;
                }
                found
            }
            _ => {
                self.capture = Some(Capture { player, baseline: current });
                None
            }
        }
    }
}

/// A pretend controller for testing without one: it appears to the game like
/// a real one, and the program moves its sticks and presses its buttons
pub struct VirtualPad {
    raw: *mut sdl2::sys::SDL_Joystick,
}

impl VirtualPad {
    pub fn attach() -> Result<VirtualPad, String> {
        use sdl2::sys::{SDL_JoystickAttachVirtual, SDL_JoystickOpen, SDL_JoystickType};
        // SAFETY: plain calls into SDL; the handle is checked before use
        unsafe {
            let index = SDL_JoystickAttachVirtual(SDL_JoystickType::SDL_JOYSTICK_TYPE_GAMECONTROLLER, 6, 15, 1);
            if index < 0 {
                return Err("could not create a virtual controller".to_string());
            }
            let raw = SDL_JoystickOpen(index);
            if raw.is_null() {
                return Err("could not open the virtual controller".to_string());
            }
            Ok(VirtualPad { raw })
        }
    }

    pub fn set_axis(&self, axis: i32, value: f64) {
        // SAFETY: the handle stays valid while this pad exists
        unsafe { sdl2::sys::SDL_JoystickSetVirtualAxis(self.raw, axis, (value.clamp(-1.0, 1.0) * 32767.0) as i16) };
    }

    pub fn set_button(&self, button: i32, down: bool) {
        // SAFETY: as above
        unsafe { sdl2::sys::SDL_JoystickSetVirtualButton(self.raw, button, if down { 1 } else { 0 }) };
    }

    /// A scripted performance: steer back and forth, hold the stick up for
    /// thrust and pulse button 0 to fire (the last two only while `playing`),
    /// and press button 3 for a few frames (to test rebinding)
    pub fn drive(&self, frame: u64, playing: bool) {
        self.set_axis(0, (frame as f64 / 50.0).sin() * 0.9);
        self.set_axis(1, if playing { -0.8 } else { 0.0 });
        self.set_button(0, playing && frame % 10 < 5);
        self.set_button(3, (20..30).contains(&frame));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gravstar_core::settings::PlayerInputSettings;

    /// Drives a virtual controller and reads it back through the same code
    /// the game uses. (One test only: SDL can only be started once.)
    #[test]
    fn a_virtual_controller_is_read_correctly() {
        let sdl = sdl2::init().expect("SDL");
        let mut pump = sdl.event_pump().expect("events");
        if sdl.joystick().expect("joystick").num_joysticks().unwrap_or(0) > 0 {
            println!("skipped: a real controller is connected and would be used instead");
            return;
        }
        let pad = VirtualPad::attach().expect("virtual pad");
        let mut hub = ControllerHub::new(sdl.joystick().expect("joystick"), 2).expect("hub");
        let mut settings = vec![PlayerInputSettings::new(0), PlayerInputSettings::new(1)];
        settings[0].device = Device::Controller;
        let messages = hub.apply_settings(&settings);
        assert_eq!(messages.len(), 1, "{messages:?}");
        assert_eq!(hub.names(2)[0].as_deref(), Some("Virtual Controller"));
        assert_eq!(hub.names(2)[1], None);

        // everything at rest
        pump.pump_events();
        assert_eq!(hub.read(0, &settings[0]), ShipInput::default());
        // player 2 uses the keyboard, so the controller reads as nothing
        assert_eq!(hub.read(1, &settings[1]), ShipInput::default());

        // stick up thrusts (default binding), stick right turns right, button 0 fires
        pad.set_axis(1, -1.0);
        pad.set_axis(0, 0.5);
        pad.set_button(0, true);
        pump.pump_events();
        let input = hub.read(0, &settings[0]);
        assert!((input.thrust - 1.0).abs() < 1e-3, "{input:?}");
        assert!(input.rotation > 0.3 && input.rotation < 0.5, "{input:?}");
        assert!(input.fire);

        // stick down is the brake
        pad.set_axis(1, 1.0);
        pump.pump_events();
        let input = hub.read(0, &settings[0]);
        assert!(input.spoiler > 0.99 && input.thrust == 0.0, "{input:?}");

        // rebinding: nothing held when the prompt appears, then button 3 is pressed
        pad.set_axis(1, 0.0);
        pad.set_axis(0, 0.0);
        pad.set_button(0, false);
        pump.pump_events();
        assert_eq!(hub.poll_capture(Some(0)), None); // takes the baseline
        assert_eq!(hub.poll_capture(Some(0)), None);
        pad.set_button(3, true);
        pump.pump_events();
        assert_eq!(hub.poll_capture(Some(0)), Some(Binding::Button { index: 3 }));
        assert_eq!(hub.poll_capture(None), None);
    }
}
