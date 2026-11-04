//! The player's settings: display options and each player's controls. They
//! are saved between runs in the same file, with the same layout, as the
//! Python version used, so existing settings carry over.
//!
//! Reading is forgiving: anything missing or invalid falls back to its
//! default, so a damaged file never stops the game starting.

use std::fs;
use std::path::PathBuf;

use serde_json::{json, Map, Value};

use crate::constants::*;

// -------------------------
// What can be configured
// -------------------------

/// The ship controls a player can rebind
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    ThrustForward,
    DeploySpoiler,
    RotateLeft,
    RotateRight,
    Fire,
}

impl Action {
    /// In the order they are shown
    pub const ALL: [Action; 5] = [Action::ThrustForward, Action::DeploySpoiler,
                                  Action::RotateLeft, Action::RotateRight, Action::Fire];

    /// The name used in the settings file
    pub fn id(self) -> &'static str {
        match self {
            Action::ThrustForward => "thrust_forward",
            Action::DeploySpoiler => "deploy_spoiler",
            Action::RotateLeft => "rotate_left",
            Action::RotateRight => "rotate_right",
            Action::Fire => "fire",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Action::ThrustForward => "Thrust",
            Action::DeploySpoiler => "Brake",
            Action::RotateLeft => "Rotate Left",
            Action::RotateRight => "Rotate Right",
            Action::Fire => "Fire",
        }
    }

    pub fn index(self) -> usize {
        Action::ALL.iter().position(|a| *a == self).unwrap()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Device {
    Keyboard,
    Controller,
}

impl Device {
    pub const ALL: [Device; 2] = [Device::Keyboard, Device::Controller];

    pub fn id(self) -> &'static str {
        match self {
            Device::Keyboard => "keyboard",
            Device::Controller => "controller",
        }
    }
}

/// One controller input: a stick (or trigger) direction, a button, or a
/// D-pad direction
#[derive(Clone, Debug, PartialEq)]
pub enum Binding {
    /// `direction` is -1 or 1; `rest` is the value the axis sits at
    /// untouched: 0 for a stick, +/-1 for a trigger
    Axis { index: u8, direction: i8, rest: f64 },
    Button { index: u8 },
    /// `axis` 0 = left / right, 1 = up / down
    Hat { index: u8, axis: u8, direction: i8 },
}

impl Binding {
    /// Do the two press the same physical thing? (An axis's rest value is
    /// ignored.)
    pub fn same_input(&self, other: &Binding) -> bool {
        match (self, other) {
            (Binding::Button { index: a }, Binding::Button { index: b }) => a == b,
            (Binding::Axis { index: a, direction: da, .. }, Binding::Axis { index: b, direction: db, .. }) => a == b && da == db,
            (Binding::Hat { index: a, axis: xa, direction: da }, Binding::Hat { index: b, axis: xb, direction: db }) => a == b && xa == xb && da == db,
            _ => false,
        }
    }

    /// Words for a menu
    pub fn label(&self) -> String {
        match self {
            Binding::Button { index } => {
                let hint = match index { 0 => " (A)", 1 => " (B)", 2 => " (X)", 3 => " (Y)", _ => "" };
                format!("Button {index}{hint}")
            }
            Binding::Hat { axis, direction, .. } => {
                let word = match (axis, direction > &0) {
                    (0, true) => "Right",
                    (0, false) => "Left",
                    (_, true) => "Up",
                    (_, false) => "Down",
                };
                format!("D-pad {word}")
            }
            Binding::Axis { index, direction, rest } => {
                if rest.abs() >= 0.5 {
                    return format!("Axis {index} (trigger)");
                }
                match (index, direction) {
                    (0, -1) => "Left Stick Left".to_string(),
                    (0, _) => "Left Stick Right".to_string(),
                    (1, -1) => "Left Stick Up".to_string(),
                    (1, _) => "Left Stick Down".to_string(),
                    _ => format!("Axis {index} {}", if *direction > 0 { "+" } else { "-" }),
                }
            }
        }
    }

    fn to_value(&self) -> Value {
        match self {
            Binding::Axis { index, direction, rest } =>
                json!({"type": "axis", "index": index, "direction": direction, "rest": rest}),
            Binding::Button { index } => json!({"type": "button", "index": index}),
            Binding::Hat { index, axis, direction } =>
                json!({"type": "hat", "index": index, "axis": axis, "direction": direction}),
        }
    }

    /// Check something loaded from a file: a clean binding, or None if it
    /// is not a valid one
    fn from_value(value: &Value) -> Option<Binding> {
        let object = value.as_object()?;
        let index = object.get("index")?.as_i64()?;
        if !(0..=MAX_BINDING_INDEX).contains(&index) {
            return None;
        }
        let index = index as u8;
        let kind = object.get("type")?.as_str()?;
        if kind == "button" {
            return Some(Binding::Button { index });
        }
        let direction = match object.get("direction")?.as_i64()? {
            -1 => -1i8,
            1 => 1i8,
            _ => return None,
        };
        match kind {
            "axis" => {
                let rest = match object.get("rest") {
                    None => 0.0,
                    Some(v) => v.as_f64()?,
                };
                Some(Binding::Axis { index, direction, rest: rest.clamp(-1.0, 1.0) })
            }
            "hat" => {
                let axis = object.get("axis")?.as_i64()?;
                if axis != 0 && axis != 1 {
                    return None;
                }
                Some(Binding::Hat { index, axis: axis as u8, direction })
            }
            _ => None,
        }
    }
}

// -------------------------
// Small helpers
// -------------------------

fn nearest(value: f64, choices: &[f64]) -> f64 {
    *choices.iter().min_by(|a, b| (*a - value).abs().partial_cmp(&(*b - value).abs()).unwrap()).unwrap()
}

fn as_resolution(value: Option<&Value>, default: (u32, u32)) -> (u32, u32) {
    let Some(list) = value.and_then(|v| v.as_array()) else { return default };
    if list.len() != 2 {
        return default;
    }
    let (Some(w), Some(h)) = (list[0].as_u64(), list[1].as_u64()) else { return default };
    let (w, h) = (w as u32, h as u32);
    if w < MIN_RESOLUTION.0 || h < MIN_RESOLUTION.1 || w > MAX_RESOLUTION.0 || h > MAX_RESOLUTION.1 {
        return default;
    }
    (w, h)
}

fn as_bool(value: Option<&Value>, default: bool) -> bool {
    value.and_then(|v| v.as_bool()).unwrap_or(default)
}

/// The resolutions to offer: the standard ones that fit the monitor, the
/// monitor's own size, and whatever is currently selected (so the current
/// choice is never missing from the list). Smallest first.
pub fn available_resolutions(desktop: (u32, u32), current: Option<(u32, u32)>) -> Vec<(u32, u32)> {
    let mut found: Vec<(u32, u32)> = RESOLUTION_CANDIDATES.iter().copied()
        .filter(|c| c.0 <= desktop.0 && c.1 <= desktop.1).collect();
    if desktop.0 >= MIN_RESOLUTION.0 && desktop.1 >= MIN_RESOLUTION.1 && desktop.0 <= MAX_RESOLUTION.0 {
        found.push(desktop);
    }
    if let Some(c) = current {
        found.push(c);
    }
    if found.is_empty() {
        found.push(RESOLUTION_CANDIDATES[2]);
    }
    found.sort_by_key(|r| (r.0 * r.1, r.0));
    found.dedup();
    found
}

// -------------------------
// Settings
// -------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct GraphicsSettings {
    pub resolution: (u32, u32),
    pub fullscreen: bool,
    pub smooth_scaling: bool,
    pub vsync: bool,
    pub show_fps: bool,
}

impl Default for GraphicsSettings {
    fn default() -> Self {
        GraphicsSettings {
            resolution: DEFAULT_RESOLUTION,
            fullscreen: DEFAULT_FULLSCREEN,
            smooth_scaling: DEFAULT_SMOOTH_SCALING,
            vsync: DEFAULT_VSYNC,
            show_fps: DEFAULT_SHOW_FPS,
        }
    }
}

impl GraphicsSettings {
    fn to_value(&self) -> Value {
        json!({"resolution": [self.resolution.0, self.resolution.1],
               "fullscreen": self.fullscreen, "smooth_scaling": self.smooth_scaling,
               "vsync": self.vsync, "show_fps": self.show_fps})
    }

    fn from_value(value: Option<&Value>) -> GraphicsSettings {
        let mut result = GraphicsSettings::default();
        let Some(data) = value.and_then(|v| v.as_object()) else { return result };
        result.resolution = as_resolution(data.get("resolution"), result.resolution);
        result.fullscreen = as_bool(data.get("fullscreen"), result.fullscreen);
        result.smooth_scaling = as_bool(data.get("smooth_scaling"), result.smooth_scaling);
        result.vsync = as_bool(data.get("vsync"), result.vsync);
        result.show_fps = as_bool(data.get("show_fps"), result.show_fps);
        result
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerInputSettings {
    pub device: Device,
    /// Key names (lower case, e.g. "w", "left shift"), by action; None =
    /// deliberately unbound
    pub keyboard: [Option<String>; 5],
    pub controller: [Option<Binding>; 5],
    pub deadzone: f64,
    pub response_curve: f64,
}

impl PlayerInputSettings {
    pub fn new(player: usize) -> Self {
        let keys = if player == 0 { DEFAULT_KEYS_PLAYER_1 } else { DEFAULT_KEYS_PLAYER_2 };
        let (x, y) = (DEFAULT_CONTROLLER_STICK_X, DEFAULT_CONTROLLER_STICK_Y);
        PlayerInputSettings {
            device: Device::Keyboard,
            // the default key list is in the order of Action::ALL
            keyboard: keys.map(|k| Some(k.to_string())),
            controller: [
                Some(Binding::Axis { index: y, direction: -1, rest: 0.0 }),
                Some(Binding::Axis { index: y, direction: 1, rest: 0.0 }),
                Some(Binding::Axis { index: x, direction: -1, rest: 0.0 }),
                Some(Binding::Axis { index: x, direction: 1, rest: 0.0 }),
                Some(Binding::Button { index: DEFAULT_CONTROLLER_FIRE_BUTTON }),
            ],
            deadzone: DEFAULT_DEADZONE,
            response_curve: DEFAULT_RESPONSE_CURVE,
        }
    }

    fn to_value(&self) -> Value {
        let mut keyboard = Map::new();
        let mut controller = Map::new();
        for action in Action::ALL {
            keyboard.insert(action.id().to_string(),
                            self.keyboard[action.index()].clone().map_or(Value::Null, Value::String));
            controller.insert(action.id().to_string(),
                              self.controller[action.index()].as_ref().map_or(Value::Null, |b| b.to_value()));
        }
        json!({"device": self.device.id(), "keyboard": keyboard, "controller": controller,
               "deadzone": self.deadzone, "response_curve": self.response_curve})
    }

    fn from_value(value: Option<&Value>, player: usize) -> PlayerInputSettings {
        let mut result = PlayerInputSettings::new(player);
        let Some(data) = value.and_then(|v| v.as_object()) else { return result };
        match data.get("device").and_then(|v| v.as_str()) {
            Some("keyboard") => result.device = Device::Keyboard,
            Some("controller") => result.device = Device::Controller,
            _ => {}
        }
        // A missing action keeps its default; an explicit null means the
        // player deliberately left it unbound.
        if let Some(keys) = data.get("keyboard").and_then(|v| v.as_object()) {
            for action in Action::ALL {
                if let Some(name) = keys.get(action.id()) {
                    result.keyboard[action.index()] =
                        name.as_str().filter(|n| !n.is_empty()).map(|n| n.to_string());
                }
            }
        }
        if let Some(pad) = data.get("controller").and_then(|v| v.as_object()) {
            for action in Action::ALL {
                if let Some(binding) = pad.get(action.id()) {
                    result.controller[action.index()] = Binding::from_value(binding);
                }
            }
        }
        if let Some(d) = data.get("deadzone").and_then(|v| v.as_f64()) {
            result.deadzone = nearest(d, &DEADZONE_CHOICES);
        }
        if let Some(c) = data.get("response_curve").and_then(|v| v.as_f64()) {
            let curves: Vec<f64> = CURVE_CHOICES.iter().map(|c| c.1).collect();
            result.response_curve = nearest(c, &curves);
        }
        result
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub graphics: GraphicsSettings,
    pub players: Vec<PlayerInputSettings>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            graphics: GraphicsSettings::default(),
            players: (0..PLAYER_COUNT).map(PlayerInputSettings::new).collect(),
        }
    }
}

impl Settings {
    pub fn reset_graphics(&mut self) {
        self.graphics = GraphicsSettings::default();
    }

    pub fn reset_player(&mut self, player: usize) {
        self.players[player] = PlayerInputSettings::new(player);
    }

    pub fn to_value(&self) -> Value {
        let mut root = Map::new();
        root.insert("version".into(), json!(SETTINGS_VERSION));
        root.insert("graphics".into(), self.graphics.to_value());
        root.insert("input".into(), json!({"players": self.players.iter().map(|p| p.to_value()).collect::<Vec<_>>()}));
        Value::Object(root)
    }

    /// Settings from loaded data. Anything missing or invalid quietly falls
    /// back to its default.
    pub fn from_value(value: &Value) -> Settings {
        let mut result = Settings::default();
        let Some(data) = value.as_object() else { return result };
        result.graphics = GraphicsSettings::from_value(data.get("graphics"));
        let players = data.get("input").and_then(|v| v.get("players")).and_then(|v| v.as_array());
        if let Some(list) = players {
            for i in 0..PLAYER_COUNT {
                if i < list.len() {
                    result.players[i] = PlayerInputSettings::from_value(Some(&list[i]), i);
                }
            }
        }
        result
    }
}

// -------------------------
// Saving and loading
// -------------------------

/// Where the settings file lives: the standard per-user configuration folder
/// for the operating system
pub fn default_settings_path() -> PathBuf {
    let var = |name: &str| std::env::var_os(name).map(PathBuf::from);
    let base = if cfg!(target_os = "windows") {
        var("APPDATA").unwrap_or_else(|| var("USERPROFILE").unwrap_or_default().join("AppData").join("Roaming"))
    } else if cfg!(target_os = "macos") {
        var("HOME").unwrap_or_default().join("Library").join("Application Support")
    } else {
        var("XDG_CONFIG_HOME").unwrap_or_else(|| var("HOME").unwrap_or_default().join(".config"))
    };
    base.join(SETTINGS_FOLDER).join(SETTINGS_FILE)
}

pub struct SettingsStore {
    pub path: PathBuf,
    /// Text of the last problem, for reporting
    pub last_error: Option<String>,
}

impl SettingsStore {
    pub fn new(path: PathBuf) -> Self {
        SettingsStore { path, last_error: None }
    }

    /// Read the settings file. A missing, empty or damaged file simply gives
    /// the defaults.
    pub fn load(&mut self) -> Settings {
        self.last_error = None;
        match fs::read_to_string(&self.path) {
            Ok(text) => match serde_json::from_str::<Value>(&text) {
                Ok(value) => Settings::from_value(&value),
                Err(e) => {
                    self.last_error = Some(format!("could not read settings ({e}); using defaults"));
                    Settings::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
            Err(e) => {
                self.last_error = Some(format!("could not read settings ({e}); using defaults"));
                Settings::default()
            }
        }
    }

    /// Write the settings file (to a temporary file first, then swapped in,
    /// so a crash part way through cannot leave a half-written file)
    pub fn save(&mut self, settings: &Settings) -> bool {
        self.last_error = None;
        let mut temp = self.path.clone().into_os_string();
        temp.push(".tmp");
        let temp = PathBuf::from(temp);
        let result = (|| -> std::io::Result<()> {
            if let Some(folder) = self.path.parent() {
                if !folder.as_os_str().is_empty() {
                    fs::create_dir_all(folder)?;
                }
            }
            let text = serde_json::to_string_pretty(&settings.to_value())
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            fs::write(&temp, text)?;
            fs::rename(&temp, &self.path)
        })();
        match result {
            Ok(()) => true,
            Err(e) => {
                self.last_error = Some(format!("could not save settings ({e})"));
                let _ = fs::remove_file(&temp);
                false
            }
        }
    }
}

/// Key names in the form the settings file uses, for showing to the player
pub fn key_label(name: Option<&str>) -> String {
    match name {
        None => "Unbound".to_string(),
        Some(n) => n.split(' ').map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
                None => String::new(),
            }
        }).collect::<Vec<_>>().join(" "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_documented_ones() {
        let s = Settings::default();
        assert_eq!(s.graphics.resolution, (1920, 1080));
        assert_eq!(s.players[0].keyboard[Action::Fire.index()].as_deref(), Some("left shift"));
        assert_eq!(s.players[1].keyboard[Action::ThrustForward.index()].as_deref(), Some("up"));
        assert_eq!(s.players.len(), 2);
    }

    #[test]
    fn settings_survive_a_save_and_load() {
        let mut s = Settings::default();
        s.graphics.resolution = (2560, 1440);
        s.graphics.fullscreen = true;
        s.players[0].keyboard[Action::Fire.index()] = Some("space".into());
        s.players[1].keyboard[Action::RotateLeft.index()] = None;
        s.players[1].device = Device::Controller;
        s.players[1].controller[Action::Fire.index()] = Some(Binding::Hat { index: 0, axis: 1, direction: 1 });
        s.players[0].deadzone = 0.30;
        s.players[0].response_curve = 2.0;
        let again = Settings::from_value(&s.to_value());
        assert_eq!(again, s);
    }

    #[test]
    fn damaged_or_odd_data_falls_back_to_defaults() {
        let d = Settings::default();
        assert_eq!(Settings::from_value(&json!(42)), d);
        assert_eq!(Settings::from_value(&json!({"graphics": "nonsense"})), d);
        let weird = json!({"graphics": {"resolution": [10, 10], "fullscreen": "yes", "vsync": false},
                           "input": {"players": [{"device": "joystick", "deadzone": 0.27,
                                     "keyboard": {"fire": "", "thrust_forward": 5, "rotate_left": "q"}}]}});
        let s = Settings::from_value(&weird);
        assert_eq!(s.graphics.resolution, (1920, 1080)); // too small
        assert_eq!(s.graphics.fullscreen, DEFAULT_FULLSCREEN); // not a bool: default
        assert!(!s.graphics.vsync);
        assert_eq!(s.players[0].device, Device::Keyboard);
        assert_eq!(s.players[0].deadzone, 0.25); // snapped to the nearest choice
        assert_eq!(s.players[0].keyboard[Action::Fire.index()], None); // "" = unbound
        assert_eq!(s.players[0].keyboard[Action::ThrustForward.index()], None); // not a string
        assert_eq!(s.players[0].keyboard[Action::RotateLeft.index()].as_deref(), Some("q"));
        assert_eq!(s.players[0].keyboard[Action::RotateRight.index()].as_deref(), Some("d")); // missing: default
        assert_eq!(s.players[1], PlayerInputSettings::new(1)); // not given: default
    }

    #[test]
    fn a_file_written_by_the_python_version_loads() {
        let text = r#"{"version": 1,
          "graphics": {"resolution": [1600, 900], "fullscreen": false, "smooth_scaling": false, "vsync": true, "show_fps": true},
          "input": {"players": [
            {"device": "keyboard", "keyboard": {"thrust_forward": "w", "deploy_spoiler": "s", "rotate_left": "a", "rotate_right": "d", "fire": "left shift"},
             "controller": {"thrust_forward": {"type": "axis", "index": 1, "direction": -1, "rest": 0.0},
                            "deploy_spoiler": {"type": "axis", "index": 1, "direction": 1, "rest": 0.0},
                            "rotate_left": {"type": "axis", "index": 0, "direction": -1, "rest": 0.0},
                            "rotate_right": {"type": "axis", "index": 0, "direction": 1, "rest": 0.0},
                            "fire": {"type": "button", "index": 0}}, "deadzone": 0.15, "response_curve": 1.0},
            {"device": "controller", "keyboard": {"thrust_forward": "up", "deploy_spoiler": "down", "rotate_left": "left", "rotate_right": "right", "fire": "right shift"},
             "controller": {"fire": {"type": "button", "index": 7}}, "deadzone": 0.2, "response_curve": 1.5}]}}"#;
        let s = Settings::from_value(&serde_json::from_str(text).unwrap());
        assert_eq!(s.graphics.resolution, (1600, 900));
        assert!(s.graphics.show_fps);
        assert_eq!(s.players[1].device, Device::Controller);
        assert_eq!(s.players[1].controller[Action::Fire.index()], Some(Binding::Button { index: 7 }));
        assert_eq!(s.players[1].response_curve, 1.5);
    }

    #[test]
    fn bindings_describe_and_compare_themselves() {
        let up = Binding::Axis { index: 1, direction: -1, rest: 0.0 };
        assert_eq!(up.label(), "Left Stick Up");
        assert_eq!(Binding::Button { index: 0 }.label(), "Button 0 (A)");
        assert_eq!(Binding::Hat { index: 0, axis: 0, direction: -1 }.label(), "D-pad Left");
        assert_eq!(Binding::Axis { index: 5, direction: 1, rest: -1.0 }.label(), "Axis 5 (trigger)");
        assert!(up.same_input(&Binding::Axis { index: 1, direction: -1, rest: 1.0 }));
        assert!(!up.same_input(&Binding::Axis { index: 1, direction: 1, rest: 0.0 }));
        assert!(!up.same_input(&Binding::Button { index: 1 }));
    }

    #[test]
    fn invalid_bindings_are_dropped() {
        assert_eq!(Binding::from_value(&json!({"type": "button", "index": 3})), Some(Binding::Button { index: 3 }));
        assert_eq!(Binding::from_value(&json!({"type": "button", "index": 999})), None);
        assert_eq!(Binding::from_value(&json!({"type": "axis", "index": 1, "direction": 2})), None);
        assert_eq!(Binding::from_value(&json!({"type": "hat", "index": 0, "axis": 3, "direction": 1})), None);
        assert_eq!(Binding::from_value(&json!("fire")), None);
    }

    #[test]
    fn resolutions_offered_fit_the_monitor() {
        let r = available_resolutions((2560, 1440), Some((1920, 1080)));
        assert_eq!(r, vec![(1280, 720), (1600, 900), (1920, 1080), (2560, 1440)]);
        let odd = available_resolutions((1366, 768), Some((1920, 1080)));
        assert!(odd.contains(&(1366, 768)) && odd.contains(&(1920, 1080)) && odd.contains(&(1280, 720)));
        assert!(available_resolutions((800, 600), None).contains(&(1920, 1080))); // never empty
    }

    #[test]
    fn the_file_is_saved_and_a_bad_one_is_survived() {
        let dir = std::env::temp_dir().join(format!("gravstar_test_{}", std::process::id()));
        let path = dir.join("nested").join("settings.json");
        let mut store = SettingsStore::new(path.clone());
        assert_eq!(store.load(), Settings::default()); // missing file
        assert!(store.last_error.is_none());
        let mut s = Settings::default();
        s.graphics.show_fps = true;
        assert!(store.save(&s));
        assert_eq!(store.load(), s);
        assert!(!path.with_extension("json.tmp").exists());
        fs::write(&path, "{ not json").unwrap();
        assert_eq!(store.load(), Settings::default());
        assert!(store.last_error.is_some());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn key_names_are_shown_nicely() {
        assert_eq!(key_label(Some("left shift")), "Left Shift");
        assert_eq!(key_label(Some("w")), "W");
        assert_eq!(key_label(None), "Unbound");
    }
}
