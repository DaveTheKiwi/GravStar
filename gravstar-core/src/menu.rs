//! The menus: which pages there are, what is on them, and what choosing or
//! changing a row does. Pure logic - the game crate draws the pages and
//! feeds in what the player pressed.
//!
//! The menu changes the settings itself, and tells the game what to do about
//! it through events (see `MenuEvent`) that the game collects with
//! `take_events()`: start the game, apply the new display mode, save...

use crate::constants::*;
use crate::settings::{key_label, Action, Binding, Device, Settings};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Page {
    Main,
    Pause,
    GameOver,
    Settings,
    Graphics,
    Input,
    Player(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RowKind {
    /// A heading (cannot be selected)
    Header,
    /// A line of read-only text (cannot be selected)
    Info,
    Button,
    /// Left / right picks another value
    Choice,
    /// An action and the key or button bound to it
    Binding,
}

#[derive(Clone, Debug, PartialEq)]
enum RowAction {
    None,
    Play,
    Resume,
    QuitApp,
    QuitToMenu,
    NewGame,
    Open(Page),
    Back,
    Resolution,
    Fullscreen,
    SmoothScaling,
    Vsync,
    ShowFps,
    ResetGraphics,
    Device(usize),
    Bind(usize, Action),
    Deadzone(usize),
    Curve(usize),
    ResetPlayer(usize),
}

/// One row of a page
#[derive(Clone, Debug)]
pub struct MenuRow {
    pub kind: RowKind,
    pub label: String,
    /// Shown at the right: the current setting, the key bound...
    pub value: String,
    action: RowAction,
}

impl MenuRow {
    pub fn selectable(&self) -> bool {
        !matches!(self.kind, RowKind::Header | RowKind::Info)
    }

    fn new(kind: RowKind, label: &str, value: String, action: RowAction) -> MenuRow {
        MenuRow { kind, label: label.to_string(), value, action }
    }

    fn button(label: &str, action: RowAction) -> MenuRow {
        MenuRow::new(RowKind::Button, label, String::new(), action)
    }

    fn choice(label: &str, value: String, action: RowAction) -> MenuRow {
        MenuRow::new(RowKind::Choice, label, value, action)
    }

    fn toggle(label: &str, on: bool, action: RowAction) -> MenuRow {
        MenuRow::choice(label, if on { "On" } else { "Off" }.to_string(), action)
    }

    fn info(text: String) -> MenuRow {
        MenuRow::new(RowKind::Info, &text, String::new(), RowAction::None)
    }
}

/// What the game should do as a result of something in the menu
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MenuEvent {
    /// Start a game (from the main menu)
    Play,
    Resume,
    QuitApp,
    QuitToMenu,
    /// Start another game (from the game-over menu)
    NewGame,
    /// A graphics setting changed: make the display match
    GraphicsChanged,
    /// A control setting changed: make the controls match
    InputChanged,
    /// Write the settings file
    SaveSettings,
}

/// What the menus need to know about the outside world; the game keeps it
/// up to date
#[derive(Clone, Debug, Default)]
pub struct MenuInfo {
    /// The screen sizes this monitor can show
    pub resolutions: Vec<(u32, u32)>,
    /// e.g. "Monitor: 2560 x 1440 at 144 Hz"
    pub display_info: String,
    /// e.g. "Frame rate: matched to the monitor (144 FPS)"
    pub pacing_info: String,
    /// The name of each player's controller, if one is connected
    pub controller_names: Vec<Option<String>>,
}

/// The game is waiting for the player to press the new key / button
#[derive(Clone, Debug)]
pub struct RebindCapture {
    pub player: usize,
    pub action: Action,
    pub device: Device,
    pub time_left: f64,
}

/// How a page is drawn
#[derive(Clone, Copy, Debug)]
pub struct MenuStyle {
    /// How much of the game behind is dimmed (0 - 255)
    pub dim_alpha: u8,
}

pub struct MenuController {
    pub settings: Settings,
    pub info: MenuInfo,
    /// The pages open, newest last, each with its highlighted row
    stack: Vec<(Page, usize)>,
    capture: Option<RebindCapture>,
    message: String,
    message_left: f64,
    /// Seconds before Enter and clicks work (just after a game-over menu opens)
    lock_left: f64,
    events: Vec<MenuEvent>,
}

impl MenuController {
    pub fn new(settings: Settings) -> Self {
        MenuController {
            settings,
            info: MenuInfo::default(),
            stack: Vec::new(),
            capture: None,
            message: String::new(),
            message_left: 0.0,
            lock_left: 0.0,
            events: Vec::new(),
        }
    }

    // -------------------------
    // Pages open / close
    // -------------------------
    pub fn is_open(&self) -> bool {
        !self.stack.is_empty()
    }

    pub fn page(&self) -> Option<Page> {
        self.stack.last().map(|p| p.0)
    }

    pub fn close(&mut self) {
        self.stack.clear();
        self.capture = None;
    }

    pub fn open_main_menu(&mut self) {
        self.close();
        self.stack.push((Page::Main, 0));
    }

    pub fn open_pause_menu(&mut self) {
        self.close();
        self.stack.push((Page::Pause, 0));
    }

    /// The menu after a game ends. For a moment it ignores Enter and clicks,
    /// so a player still tapping fire cannot start a new game by accident.
    pub fn open_game_over_menu(&mut self) {
        self.close();
        self.stack.push((Page::GameOver, 0));
        self.lock_left = GAME_OVER_MENU_LOCK_SECONDS;
    }

    /// Open these pages, one on top of the other (for tests and screenshots)
    pub fn open_pages(&mut self, pages: &[Page]) {
        self.close();
        for page in pages {
            self.stack.push((*page, 0));
        }
        self.fix_selection();
    }

    pub fn take_events(&mut self) -> Vec<MenuEvent> {
        std::mem::take(&mut self.events)
    }

    // -------------------------
    // What to show
    // -------------------------
    pub fn title(&self) -> String {
        match self.page() {
            None => String::new(),
            Some(Page::Main) => "GRAVSTAR".into(),
            Some(Page::Pause) => "PAUSED".into(),
            // the VICTORY / DEFEAT text drawn above this menu is its heading
            Some(Page::GameOver) => String::new(),
            Some(Page::Settings) => "SETTINGS".into(),
            Some(Page::Graphics) => "GRAPHICS".into(),
            Some(Page::Input) => "INPUT".into(),
            Some(Page::Player(p)) => format!("PLAYER {} CONTROLS", p + 1),
        }
    }

    pub fn style(&self) -> MenuStyle {
        match self.page() {
            Some(Page::GameOver) => MenuStyle { dim_alpha: GAME_OVER_MENU_DIM_ALPHA },
            _ => MenuStyle { dim_alpha: MENU_DIM_ALPHA },
        }
    }

    pub fn selected(&self) -> usize {
        self.stack.last().map_or(0, |p| p.1)
    }

    /// The rows of the open page (built afresh, so they always show the
    /// current settings)
    pub fn rows(&self) -> Vec<MenuRow> {
        match self.page() {
            None => Vec::new(),
            Some(Page::Main) => vec![
                MenuRow::button("Play", RowAction::Play),
                MenuRow::button("Settings", RowAction::Open(Page::Settings)),
                MenuRow::button("Quit", RowAction::QuitApp),
            ],
            Some(Page::Pause) => vec![
                MenuRow::button("Resume", RowAction::Resume),
                MenuRow::button("Settings", RowAction::Open(Page::Settings)),
                MenuRow::button("Quit to Main Menu", RowAction::QuitToMenu),
            ],
            Some(Page::GameOver) => vec![
                MenuRow::button("New Game", RowAction::NewGame),
                MenuRow::button("Settings", RowAction::Open(Page::Settings)),
                MenuRow::button("Main Menu", RowAction::QuitToMenu),
                MenuRow::button("Quit", RowAction::QuitApp),
            ],
            Some(Page::Settings) => vec![
                MenuRow::button("Graphics", RowAction::Open(Page::Graphics)),
                MenuRow::button("Input", RowAction::Open(Page::Input)),
                MenuRow::button("Back", RowAction::Back),
            ],
            Some(Page::Graphics) => self.graphics_rows(),
            Some(Page::Input) => {
                let mut rows: Vec<MenuRow> = (0..PLAYER_COUNT)
                    .map(|i| MenuRow::button(&format!("Player {} controls", i + 1), RowAction::Open(Page::Player(i))))
                    .collect();
                rows.push(MenuRow::button("Back", RowAction::Back));
                rows
            }
            Some(Page::Player(p)) => self.player_rows(p),
        }
    }

    fn resolution_list(&self) -> Vec<(u32, u32)> {
        let mut list = self.info.resolutions.clone();
        let current = self.settings.graphics.resolution;
        if !list.contains(&current) {
            list.push(current);
        }
        list.sort_by_key(|r| (r.0 * r.1, r.0));
        list.dedup();
        list
    }

    fn graphics_rows(&self) -> Vec<MenuRow> {
        let g = &self.settings.graphics;
        let (w, h) = g.resolution;
        vec![
            MenuRow::choice("Resolution", format!("{w} x {h}"), RowAction::Resolution),
            MenuRow::toggle("Fullscreen", g.fullscreen, RowAction::Fullscreen),
            MenuRow::toggle("Smooth scaling", g.smooth_scaling, RowAction::SmoothScaling),
            MenuRow::toggle("VSync", g.vsync, RowAction::Vsync),
            MenuRow::toggle("Show FPS", g.show_fps, RowAction::ShowFps),
            MenuRow::info(self.info.display_info.clone()),
            MenuRow::info(self.info.pacing_info.clone()),
            MenuRow::button("Reset graphics to defaults", RowAction::ResetGraphics),
            MenuRow::button("Back", RowAction::Back),
        ]
    }

    fn controller_name(&self, player: usize) -> Option<&String> {
        self.info.controller_names.get(player).and_then(|n| n.as_ref())
    }

    fn player_rows(&self, player: usize) -> Vec<MenuRow> {
        let p = &self.settings.players[player];
        let controller = p.device == Device::Controller;
        let mut rows = vec![MenuRow::choice(
            "Device", if controller { "Controller" } else { "Keyboard" }.to_string(),
            RowAction::Device(player))];
        if controller {
            rows.push(MenuRow::info(match self.controller_name(player) {
                Some(name) => format!("Controller: {name}"),
                None => "No controller connected".to_string(),
            }));
        }
        rows.push(MenuRow::new(RowKind::Header, "Bindings  (Enter to change, Delete to clear)",
                               String::new(), RowAction::None));
        for action in Action::ALL {
            let value = match &self.capture {
                Some(c) if c.player == player && c.action == action => self.capture_prompt(),
                _ if controller => p.controller[action.index()].as_ref().map_or("Unbound".to_string(), |b| b.label()),
                _ => key_label(p.keyboard[action.index()].as_deref()),
            };
            rows.push(MenuRow::new(RowKind::Binding, action.label(), value, RowAction::Bind(player, action)));
        }
        if controller {
            rows.push(MenuRow::choice("Stick deadzone", format!("{}%", (p.deadzone * 100.0).round() as i32),
                                      RowAction::Deadzone(player)));
            let curve = CURVE_CHOICES.iter().find(|c| c.1 == p.response_curve).map_or("", |c| c.0);
            rows.push(MenuRow::choice("Stick response", curve.to_string(), RowAction::Curve(player)));
        }
        rows.push(MenuRow::button("Reset to defaults", RowAction::ResetPlayer(player)));
        rows.push(MenuRow::button("Back", RowAction::Back));
        rows
    }

    // -------------------------
    // Navigation (called by the game with what the player pressed)
    // -------------------------

    /// Move the highlight, skipping rows that cannot be selected
    pub fn navigate(&mut self, delta: i32) {
        if self.capture.is_some() {
            return;
        }
        let rows = self.rows();
        if !rows.iter().any(|r| r.selectable()) {
            return;
        }
        let step = if delta >= 0 { 1 } else { -1 };
        let count = rows.len() as i32;
        let mut index = self.selected() as i32;
        for _ in 0..count {
            index = (index + step).rem_euclid(count);
            if rows[index as usize].selectable() {
                self.set_selected(index as usize);
                return;
            }
        }
    }

    /// Highlight a row (the mouse is over it)
    pub fn select_row(&mut self, index: usize) {
        if self.capture.is_some() {
            return;
        }
        if self.rows().get(index).map_or(false, |r| r.selectable()) {
            self.set_selected(index);
        }
    }

    fn set_selected(&mut self, index: usize) {
        if let Some(top) = self.stack.last_mut() {
            top.1 = index;
        }
    }

    /// Keep the highlight on a real row after the page changes
    fn fix_selection(&mut self) {
        let rows = self.rows();
        if rows.is_empty() {
            return;
        }
        let index = self.selected().min(rows.len() - 1);
        self.set_selected(index);
        if !rows[index].selectable() {
            self.navigate(1);
        }
    }

    fn current_row(&self) -> Option<MenuRow> {
        self.rows().get(self.selected()).cloned()
    }

    fn push(&mut self, page: Page) {
        self.stack.push((page, 0));
        self.fix_selection();
    }

    /// Back one page; on the first page, whatever "back" means there
    pub fn back(&mut self) {
        if self.capture.is_some() {
            self.cancel_capture();
        } else if self.stack.len() > 1 {
            self.stack.pop();
            self.fix_selection();
        } else if self.page() == Some(Page::Pause) {
            self.events.push(MenuEvent::Resume);
        }
    }

    pub fn update(&mut self, dt: f64) {
        if self.lock_left > 0.0 {
            self.lock_left -= dt;
        }
        if let Some(c) = self.capture.as_mut() {
            c.time_left -= dt;
            if c.time_left <= 0.0 {
                self.capture = None;
            }
        }
        if self.message_left > 0.0 {
            self.message_left -= dt;
            if self.message_left <= 0.0 {
                self.message.clear();
            }
        }
    }

    /// Show a short message to the player
    pub fn say(&mut self, text: &str) {
        self.message = text.to_string();
        self.message_left = MENU_MESSAGE_SECONDS;
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    // -------------------------
    // Choosing and changing
    // -------------------------
    pub fn activate(&mut self) {
        if self.capture.is_some() || self.lock_left > 0.0 {
            return;
        }
        let Some(row) = self.current_row() else { return };
        match row.action {
            RowAction::Play => self.events.push(MenuEvent::Play),
            RowAction::Resume => self.events.push(MenuEvent::Resume),
            RowAction::QuitApp => self.events.push(MenuEvent::QuitApp),
            RowAction::QuitToMenu => self.events.push(MenuEvent::QuitToMenu),
            RowAction::NewGame => self.events.push(MenuEvent::NewGame),
            RowAction::Open(page) => self.push(page),
            RowAction::Back => self.back(),
            RowAction::ResetGraphics => {
                self.settings.reset_graphics();
                self.graphics_changed();
            }
            RowAction::Bind(player, action) => self.begin_capture(player, action),
            RowAction::ResetPlayer(player) => {
                self.settings.reset_player(player);
                self.capture = None;
                self.input_changed();
            }
            // a choice row: choosing it moves on to the next value
            _ => self.adjust(1),
        }
    }

    /// Left (-1) / right (+1) on a row that offers values
    pub fn adjust(&mut self, delta: i32) {
        if self.capture.is_some() {
            return;
        }
        let Some(row) = self.current_row() else { return };
        let next = |index: usize, count: usize| (index as i32 + delta).rem_euclid(count as i32) as usize;
        match row.action {
            RowAction::Resolution => {
                let list = self.resolution_list();
                let at = list.iter().position(|r| *r == self.settings.graphics.resolution).unwrap_or(0);
                self.settings.graphics.resolution = list[next(at, list.len())];
                self.graphics_changed();
            }
            RowAction::Fullscreen => {
                self.settings.graphics.fullscreen = !self.settings.graphics.fullscreen;
                self.graphics_changed();
            }
            RowAction::SmoothScaling => {
                self.settings.graphics.smooth_scaling = !self.settings.graphics.smooth_scaling;
                self.graphics_changed();
            }
            RowAction::Vsync => {
                self.settings.graphics.vsync = !self.settings.graphics.vsync;
                self.graphics_changed();
            }
            RowAction::ShowFps => {
                self.settings.graphics.show_fps = !self.settings.graphics.show_fps;
                self.graphics_changed();
            }
            RowAction::Device(player) => {
                let p = &mut self.settings.players[player];
                p.device = Device::ALL[next(Device::ALL.iter().position(|d| *d == p.device).unwrap(), 2)];
                self.capture = None;
                self.input_changed();
            }
            RowAction::Deadzone(player) => {
                let p = &mut self.settings.players[player];
                let at = DEADZONE_CHOICES.iter().position(|d| *d == p.deadzone).unwrap_or(0);
                p.deadzone = DEADZONE_CHOICES[next(at, DEADZONE_CHOICES.len())];
                self.input_changed();
            }
            RowAction::Curve(player) => {
                let p = &mut self.settings.players[player];
                let at = CURVE_CHOICES.iter().position(|c| c.1 == p.response_curve).unwrap_or(0);
                p.response_curve = CURVE_CHOICES[next(at, CURVE_CHOICES.len())].1;
                self.input_changed();
            }
            _ => {}
        }
    }

    /// Delete / Backspace: leave a binding unbound
    pub fn clear_selected(&mut self) {
        if self.capture.is_some() {
            return;
        }
        if let Some(MenuRow { action: RowAction::Bind(player, action), .. }) = self.current_row() {
            let p = &mut self.settings.players[player];
            match p.device {
                Device::Keyboard => p.keyboard[action.index()] = None,
                Device::Controller => p.controller[action.index()] = None,
            }
            self.input_changed();
        }
    }

    // -------------------------
    // Rebinding
    // -------------------------
    pub fn is_capturing(&self) -> bool {
        self.capture.is_some()
    }

    pub fn capture(&self) -> Option<&RebindCapture> {
        self.capture.as_ref()
    }

    pub fn capture_prompt(&self) -> String {
        match &self.capture {
            None => String::new(),
            Some(c) if c.device == Device::Keyboard => "Press a key...".to_string(),
            Some(_) => "Press a button or move a stick...".to_string(),
        }
    }

    fn begin_capture(&mut self, player: usize, action: Action) {
        let device = self.settings.players[player].device;
        if device == Device::Controller && self.controller_name(player).is_none() {
            self.say("No controller connected for this player");
            return;
        }
        self.capture = Some(RebindCapture { player, action, device, time_left: REBIND_TIMEOUT_SECONDS });
    }

    pub fn cancel_capture(&mut self) {
        self.capture = None;
    }

    /// A key was pressed (its lower-case name). Returns true if the rebind
    /// used it up.
    pub fn handle_key(&mut self, key_name: &str) -> bool {
        let Some(capture) = self.capture.clone() else { return false };
        if key_name == "escape" {
            self.cancel_capture();
            return true;
        }
        if capture.device != Device::Keyboard {
            return true; // waiting for a controller input
        }
        let keys = &mut self.settings.players[capture.player].keyboard;
        for (i, existing) in keys.iter_mut().enumerate() {
            if i != capture.action.index() && existing.as_deref() == Some(key_name) {
                *existing = None; // the key moves to the new action
            }
        }
        keys[capture.action.index()] = Some(key_name.to_string());
        self.cancel_capture();
        self.input_changed();
        true
    }

    /// A controller input was detected while waiting for one
    pub fn handle_controller_binding(&mut self, binding: Binding) -> bool {
        let Some(capture) = self.capture.clone() else { return false };
        if capture.device != Device::Controller {
            return false;
        }
        let pad = &mut self.settings.players[capture.player].controller;
        for (i, existing) in pad.iter_mut().enumerate() {
            if i != capture.action.index() && existing.as_ref().map_or(false, |e| e.same_input(&binding)) {
                *existing = None;
            }
        }
        pad[capture.action.index()] = Some(binding);
        self.cancel_capture();
        self.input_changed();
        true
    }

    // -------------------------
    // Telling the game
    // -------------------------
    fn graphics_changed(&mut self) {
        self.events.push(MenuEvent::GraphicsChanged);
        self.events.push(MenuEvent::SaveSettings);
    }

    fn input_changed(&mut self) {
        self.events.push(MenuEvent::InputChanged);
        self.events.push(MenuEvent::SaveSettings);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menus() -> MenuController {
        let mut m = MenuController::new(Settings::default());
        m.info.resolutions = vec![(1280, 720), (1600, 900), (1920, 1080), (2560, 1440)];
        m.open_main_menu();
        m
    }

    fn labels(m: &MenuController) -> Vec<String> {
        m.rows().iter().map(|r| r.label.clone()).collect()
    }

    /// Open a page by choosing the named rows in turn
    fn go(m: &mut MenuController, path: &[&str]) {
        for name in path {
            let at = labels(m).iter().position(|l| l == name).unwrap_or_else(|| panic!("no row {name}"));
            m.select_row(at);
            m.activate();
        }
    }

    #[test]
    fn the_main_menu_starts_the_game_or_quits() {
        let mut m = menus();
        assert_eq!(m.title(), "GRAVSTAR");
        assert_eq!(labels(&m), ["Play", "Settings", "Quit"]);
        m.activate();
        assert_eq!(m.take_events(), vec![MenuEvent::Play]);
        m.navigate(-1); // wraps up to the last row
        m.activate();
        assert_eq!(m.take_events(), vec![MenuEvent::QuitApp]);
    }

    #[test]
    fn the_pause_menu_resumes_with_escape() {
        let mut m = menus();
        m.open_pause_menu();
        m.back();
        assert_eq!(m.take_events(), vec![MenuEvent::Resume]);
        m.back(); // the main menu has no "back"
        let mut m = menus();
        m.back();
        assert!(m.take_events().is_empty() && m.is_open());
    }

    #[test]
    fn pages_open_and_back_returns_to_the_row_you_left() {
        let mut m = menus();
        go(&mut m, &["Settings", "Input"]);
        assert_eq!(m.title(), "INPUT");
        m.back();
        assert_eq!(m.title(), "SETTINGS");
        assert_eq!(m.rows()[m.selected()].label, "Input");
        go(&mut m, &["Back"]);
        assert_eq!(m.title(), "GRAVSTAR");
    }

    #[test]
    fn the_highlight_skips_rows_that_cannot_be_selected() {
        let mut m = menus();
        go(&mut m, &["Settings", "Input", "Player 1 controls"]);
        // the first rows are Device (selectable) then a header, then bindings
        let rows = m.rows();
        assert!(rows[0].selectable() && !rows[1].selectable());
        m.navigate(1);
        assert_eq!(m.rows()[m.selected()].label, "Thrust");
        m.navigate(-1);
        assert_eq!(m.selected(), 0);
    }

    #[test]
    fn graphics_choices_change_the_settings_and_tell_the_game() {
        let mut m = menus();
        go(&mut m, &["Settings", "Graphics"]);
        assert_eq!(m.rows()[0].value, "1920 x 1080");
        m.adjust(1);
        assert_eq!(m.settings.graphics.resolution, (2560, 1440));
        assert_eq!(m.take_events(), vec![MenuEvent::GraphicsChanged, MenuEvent::SaveSettings]);
        m.adjust(1); // wraps round to the smallest
        assert_eq!(m.settings.graphics.resolution, (1280, 720));
        m.select_row(1);
        m.activate(); // choosing a toggle flips it
        assert!(m.settings.graphics.fullscreen);
        assert_eq!(m.rows()[1].value, "On");
        m.take_events();
        go(&mut m, &["Reset graphics to defaults"]);
        assert_eq!(m.settings.graphics, crate::settings::GraphicsSettings::default());
        assert!(m.take_events().contains(&MenuEvent::GraphicsChanged));
    }

    #[test]
    fn the_current_resolution_is_always_on_offer() {
        let mut m = menus();
        m.settings.graphics.resolution = (1366, 768);
        go(&mut m, &["Settings", "Graphics"]);
        m.adjust(1);
        assert_eq!(m.settings.graphics.resolution, (1600, 900)); // next above 1366 x 768
        // an unusual current size is offered only while it is the current one
        m.adjust(-1);
        assert_eq!(m.settings.graphics.resolution, (1280, 720));
    }

    #[test]
    fn rebinding_a_key_takes_it_from_the_action_that_had_it() {
        let mut m = menus();
        go(&mut m, &["Settings", "Input", "Player 1 controls", "Fire"]);
        assert!(m.is_capturing());
        assert_eq!(m.capture_prompt(), "Press a key...");
        assert_eq!(m.rows().iter().find(|r| r.label == "Fire").unwrap().value, "Press a key...");
        assert!(m.handle_key("w")); // w was Thrust
        assert!(!m.is_capturing());
        let keys = &m.settings.players[0].keyboard;
        assert_eq!(keys[Action::Fire.index()].as_deref(), Some("w"));
        assert_eq!(keys[Action::ThrustForward.index()], None);
        assert!(m.take_events().contains(&MenuEvent::InputChanged));
        assert!(!m.handle_key("x")); // nothing waiting: not used up
    }

    #[test]
    fn a_rebind_can_be_cancelled_or_times_out() {
        let mut m = menus();
        go(&mut m, &["Settings", "Input", "Player 1 controls", "Fire"]);
        assert!(m.handle_key("escape"));
        assert!(!m.is_capturing());
        go(&mut m, &["Fire"]);
        m.update(REBIND_TIMEOUT_SECONDS + 0.1);
        assert!(!m.is_capturing());
        assert_eq!(m.settings.players[0].keyboard[Action::Fire.index()].as_deref(), Some("left shift"));
        go(&mut m, &["Fire"]);
        m.back(); // Esc via the menu also cancels
        assert!(!m.is_capturing() && m.title() == "PLAYER 1 CONTROLS");
    }

    #[test]
    fn delete_clears_a_binding_and_reset_restores_the_defaults() {
        let mut m = menus();
        go(&mut m, &["Settings", "Input", "Player 2 controls"]);
        let at = labels(&m).iter().position(|l| l == "Rotate Left").unwrap();
        m.select_row(at);
        m.clear_selected();
        assert_eq!(m.settings.players[1].keyboard[Action::RotateLeft.index()], None);
        assert_eq!(m.rows()[at].value, "Unbound");
        go(&mut m, &["Reset to defaults"]);
        assert_eq!(m.settings.players[1].keyboard[Action::RotateLeft.index()].as_deref(), Some("left"));
    }

    #[test]
    fn the_controller_page_needs_a_controller_to_rebind() {
        let mut m = menus();
        go(&mut m, &["Settings", "Input", "Player 1 controls"]);
        m.adjust(1); // Device: Controller
        assert_eq!(m.settings.players[0].device, Device::Controller);
        assert!(labels(&m).contains(&"No controller connected".to_string()));
        assert!(labels(&m).contains(&"Stick deadzone".to_string()));
        go(&mut m, &["Fire"]);
        assert!(!m.is_capturing());
        assert_eq!(m.message(), "No controller connected for this player");
        m.update(MENU_MESSAGE_SECONDS + 0.1);
        assert_eq!(m.message(), "");

        // with a controller, a button press is taken
        m.info.controller_names = vec![Some("Test Pad".into()), None];
        assert!(labels(&m).contains(&"Controller: Test Pad".to_string()));
        go(&mut m, &["Fire"]);
        assert!(m.is_capturing());
        assert!(m.handle_key("q")); // keys are ignored, but used up
        assert!(m.handle_controller_binding(Binding::Axis { index: 1, direction: -1, rest: 0.0 }));
        let pad = &m.settings.players[0].controller;
        assert_eq!(pad[Action::Fire.index()], Some(Binding::Axis { index: 1, direction: -1, rest: 0.0 }));
        assert_eq!(pad[Action::ThrustForward.index()], None); // it moved from Thrust
    }

    #[test]
    fn deadzone_and_response_cycle_through_their_choices() {
        let mut m = menus();
        go(&mut m, &["Settings", "Input", "Player 1 controls"]);
        m.adjust(1);
        go(&mut m, &["Stick deadzone"]);
        assert_eq!(m.settings.players[0].deadzone, 0.20);
        assert_eq!(m.rows().iter().find(|r| r.label == "Stick deadzone").unwrap().value, "20%");
        let at = labels(&m).iter().position(|l| l == "Stick response").unwrap();
        m.select_row(at);
        m.adjust(1);
        assert_eq!(m.settings.players[0].response_curve, 1.5);
        assert_eq!(m.rows()[at].value, "Gentle");
    }

    #[test]
    fn the_game_over_menu_ignores_input_for_a_moment() {
        let mut m = menus();
        m.open_game_over_menu();
        assert_eq!(m.title(), "");
        assert_eq!(labels(&m), ["New Game", "Settings", "Main Menu", "Quit"]);
        assert_eq!(m.style().dim_alpha, GAME_OVER_MENU_DIM_ALPHA);
        m.activate();
        assert!(m.take_events().is_empty());
        m.update(GAME_OVER_MENU_LOCK_SECONDS + 0.01);
        m.activate();
        assert_eq!(m.take_events(), vec![MenuEvent::NewGame]);
        go(&mut m, &["Settings"]);
        assert_eq!(m.style().dim_alpha, MENU_DIM_ALPHA);
        m.back();
        assert_eq!(m.page(), Some(Page::GameOver));
        m.back(); // no "back" past the game-over menu
        assert!(m.is_open());
    }

    #[test]
    fn closing_leaves_no_menu_open() {
        let mut m = menus();
        m.close();
        assert!(!m.is_open() && m.rows().is_empty() && m.title().is_empty());
        m.activate();
        m.navigate(1);
        m.adjust(1); // all harmless with nothing open
    }
}
