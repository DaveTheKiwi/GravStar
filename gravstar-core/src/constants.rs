//! Every constant in GravStar lives in this file. If you need a new one, add
//! it here (under the right heading) - never next to the code that uses it.
//!

// ---------------------------------------------------------------------------
// Level
// ---------------------------------------------------------------------------
pub const MIN_LEVEL_WIDTH: usize = 10;
pub const MAX_LEVEL_WIDTH: usize = 500;
pub const MIN_LEVEL_HEIGHT: usize = 10;
pub const MAX_LEVEL_HEIGHT: usize = 500;

/// Level file format version
pub const LEVEL_FORMAT_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Tiles
// ---------------------------------------------------------------------------
/// Width and height of a tile, in pixels (one definition for the whole game)
pub const TILE_SIZE: usize = 32;

/// Tile art is drawn this many times larger, then shrunk to TILE_SIZE, which
/// smooths the edges. The edge pixels get (factor x factor) + 1 possible
/// levels of transparency, so 2 gives 5 levels (rough) and 8 gives 65
/// (smooth). Changing it can move the collision masks by a pixel or two at
/// the edges.
pub const TILE_SUPERSAMPLE: usize = 8;

/// Tile shapes are described on a grid of this many units across, so 0 is
/// the left / top edge, 1 the middle and 2 the right / bottom edge
pub const TILE_ART_GRID: i32 = 2;

/// Colour of generated tile art (RGBA)
pub const TILE_COLOUR: [u8; 4] = [128, 128, 128, 255];

/// A pixel whose alpha is above this counts as solid in a collision mask
/// (tiles and ships)
pub const MASK_ALPHA_SOLID_ABOVE: u8 = 128;

/// Bytes in a tile's collision mask (one bit per pixel, rounded up)
pub const TILE_MASK_BYTES: usize = (TILE_SIZE * TILE_SIZE + 7) / 8;

/// Hardness given to tiles that do not say otherwise (0.0 - 1.0)
pub const DEFAULT_TILE_HARDNESS: f32 = 0.5;

// ---------------------------------------------------------------------------
// Destructible terrain
// ---------------------------------------------------------------------------
/// When damage cuts a piece of terrain free and that piece is smaller than
/// this many pixels, it is deleted. A tile is 32 x 32 = 1024 pixels and the
/// smallest piece of tile art has 120, so keep this well below that. 0 or 1
/// switches the clean-up off.
pub const MIN_ISLAND_AREA: usize = 40;

/// After a cut, the island search starts from solid pixels this far (in
/// pixels) beyond the edge of the hole. Pixels touching a removed pixel are
/// within sqrt(2) of the hole, so this must stay at 1.5 or more.
pub const ISLAND_SEED_RING_WIDTH: f64 = 1.5;

/// How far past the cut's radius (in pixels) the island search looks for
/// those starting pixels, so no row of the ring is missed
pub const ISLAND_SEARCH_MARGIN: f64 = 2.0;

// ---------------------------------------------------------------------------
// World
// ---------------------------------------------------------------------------
/// Gravitational acceleration, in pixels per second squared (downwards)
pub const GRAVITY: f64 = 250.0;

// ---------------------------------------------------------------------------
// Ships
//
// Units: lengths in pixels, time in seconds, mass in "mass units", force in
// mass * pixels / s^2, torque in mass * pixels^2 / s^2. Angles are radians.
// ---------------------------------------------------------------------------
pub const SHIP_MASS: f64 = 1.0;

/// Size of the ship in the world, in pixels. Also the size its collision
/// masks are baked at, and its moment of inertia is worked out from it.
pub const SHIP_WIDTH: usize = 48;
pub const SHIP_HEIGHT: usize = 32;

/// Force of the main engine at full throttle. Acceleration = force / mass.
pub const SHIP_MAX_THRUST_FORCE: f64 = 750.0;

/// Torque of the steering thrusters at full stick. Angular acceleration =
/// torque / moment of inertia.
pub const SHIP_MAX_TURNING_TORQUE: f64 = 3150.0;

/// Air drag opposing the ship's motion: force = coefficient * speed (a
/// linear, "viscous" drag), so the speed bleeds off exponentially. The decay
/// rate is coefficient / mass per second. At 0.7 and a mass of 1 a coasting
/// ship keeps about half its speed after a second.
pub const SHIP_DRAG_COEFFICIENT: f64 = 0.7;

/// How much deploying the spoiler (air brake) multiplies that coefficient by.
/// 2.3 leaves a fully braking ship with about a fifth of its speed after a
/// second. A half-deployed spoiler is half way between.
pub const SHIP_SPOILER_DRAG_MULTIPLIER: f64 = 2.3;

/// Drag torque on the ship's spin: torque = -coefficient * angular speed.
/// Always applies, and is what limits the spin speed while steering.
pub const SHIP_ANGULAR_DRAG_COEFFICIENT: f64 = 340.0;

/// Extra drag torque from the stabilising gyros, applied only while the
/// player is not steering, which brings the ship to rest quickly.
pub const SHIP_STABILISER_DRAG_COEFFICIENT: f64 = 1760.0;

/// Fuel burned per unit of thrust impulse (force * time), if fuel is limited
pub const SHIP_FUEL_PER_THRUST_IMPULSE: f64 = 1.0;

pub const SHIP_MAX_SHIELD: f64 = 100.0;
pub const SHIP_MAX_HULL: f64 = 100.0;

/// Shield points regained per second, once no damage has been taken for
/// SHIP_SHIELD_REGEN_DELAY seconds. The hull never regenerates.
pub const SHIP_SHIELD_REGEN_RATE: f64 = 5.0;
pub const SHIP_SHIELD_REGEN_DELAY: f64 = 3.0;

/// Number of pre-baked collision masks around a full turn (2 degree steps)
pub const SHIP_MASK_STEPS: usize = 180;

/// A ship's input below this size counts as "not steering"
pub const SHIP_STEERING_DEAD_ZONE: f64 = 0.001;

// ---------------------------------------------------------------------------
// Weapons and projectiles
// ---------------------------------------------------------------------------
/// Shots per second from a ship's gun
pub const SHIP_FIRE_RATE: f64 = 4.0;

/// The basic bullet
pub const BULLET_MASS: f64 = 0.1;
pub const BULLET_MUZZLE_SPEED: f64 = 900.0;
/// Linear drag, as for ships: decay rate = coefficient / mass per second
pub const BULLET_DRAG_COEFFICIENT: f64 = 0.06;
/// Slower than this and the bullet vanishes (pixels per second)
pub const BULLET_MIN_SPEED: f64 = 40.0;
pub const BULLET_SHIP_DAMAGE: f64 = 10.0;
/// Radius of the hole a bullet cuts in the terrain (pixels)
pub const BULLET_CRATER_RADIUS: f64 = 8.0;
/// Pixels the crater is pushed into the material along the bullet's path
pub const BULLET_CRATER_EMBED: f64 = 0.0;

// ---------------------------------------------------------------------------
// Match rules
// ---------------------------------------------------------------------------
pub const STARTING_LIVES: u32 = 3;
/// The 3 - 2 - 1 before a ship may move
pub const COUNTDOWN_SECONDS: f64 = 3.0;
/// How long "GO!" stays up after the countdown
pub const GO_MESSAGE_SECONDS: f64 = 0.8;
/// Damage immunity after the countdown ends
pub const IMMUNITY_SECONDS: f64 = 3.0;
/// Seconds between a ship's destruction and its respawn
pub const DEATH_DELAY_SECONDS: f64 = 1.5;
/// Seconds of VICTORY / DEFEAT before the menu comes up
pub const GAME_OVER_DELAY_SECONDS: f64 = 3.0;
/// A respawning ship appears at least this far (pixels) from the other ship,
/// or at the furthest spawn point if none is that far
pub const RESPAWN_MIN_DISTANCE: f64 = 1000.0;

// ---------------------------------------------------------------------------
// Collisions
// ---------------------------------------------------------------------------
/// How much of the speed INTO a surface comes back out (0 = none, 1 = a
/// perfect bounce)
pub const COLLISION_RESTITUTION: f64 = 0.45;

/// Coulomb friction between a ship and what it touches: sliding speed is
/// reduced by at most this multiple of the speed the surface took off the
/// ship's approach (0 = frictionless)
pub const COLLISION_FRICTION: f64 = 0.4;

/// Closing slower than this (pixels per second) is a resting contact, not an
/// impact: there is no bounce, so a ship at rest settles instead of jittering
pub const COLLISION_RESTING_SPEED: f64 = 20.0;

/// Impacts at or below this closing speed (pixels per second) are harmless:
/// a gentle landing does no damage
pub const COLLISION_SAFE_IMPACT_SPEED: f64 = 50.0;

/// Damage per unit of impulse (change in momentum) above the harmless speed
pub const COLLISION_DAMAGE_PER_IMPULSE: f64 = 0.04;

/// A ship's spin is multiplied by this at every contact
pub const COLLISION_SPIN_RETENTION: f64 = 0.5;

/// A ship in contact is moved back out along the surface normal in steps of
/// this many pixels, until it is clear, for at most COLLISION_MAX_PUSH_OUT
/// pixels
pub const COLLISION_PUSH_OUT_STEP: f64 = 0.1;
pub const COLLISION_MAX_PUSH_OUT: f64 = 32.0;

/// The terrain's surface direction is read from solid pixels this many
/// pixels around each overlapping pixel
pub const COLLISION_NORMAL_SAMPLE_RADIUS: i32 = 3;

// ---------------------------------------------------------------------------
// Projectiles
// ---------------------------------------------------------------------------
/// A bullet is tested for hits at least every this many pixels along its
/// path each frame, so however fast it goes it cannot skip over a thin wall
/// or a ship
pub const PROJECTILE_SAMPLE_STEP: f64 = 1.0;

/// Safety cap on bullets in flight
pub const MAX_PROJECTILES: usize = 512;

// ---------------------------------------------------------------------------
// Camera
// ---------------------------------------------------------------------------
/// The zoom values below are for a screen this many pixels high. On other
/// screens they are scaled by height / this, so every player sees the same
/// amount of the level.
pub const REFERENCE_SCREEN_HEIGHT: f64 = 1080.0;

/// The camera zooms out with ship speed, but never further out than this
/// (or further than shows the whole level) ...
pub const MIN_ZOOM_FLOOR: f64 = 0.6;
/// ... and never further in than this
pub const MAX_ZOOM: f64 = 1.0;
/// Seconds for the zoom to cover about 63% of the way to where it is
/// heading, zooming out and zooming back in. Larger = lazier.
pub const ZOOM_OUT_TIME: f64 = 1.0;
pub const ZOOM_IN_TIME: f64 = 1.5;
/// Slower than this (pixels per second) the camera stays at exactly full
/// zoom: a parked ship is never perfectly still
pub const ZOOM_DEAD_ZONE_SPEED: f64 = 80.0;
/// The speed at which the camera is fully zoomed out
pub const ZOOM_FULL_SPEED: f64 = 600.0;
/// The slowest the zoom closes the last sliver of its target, per second
/// (so it really arrives at full zoom, where nothing needs scaling)
pub const ZOOM_SETTLE_RATE: f64 = 0.01;

// ---------------------------------------------------------------------------
// Explosions (visual only)
// ---------------------------------------------------------------------------
/// Seconds from start to gone
pub const EXPLOSION_DURATION: f64 = 0.30;
/// The fraction of that time at which it is biggest
pub const EXPLOSION_PEAK_AT: f64 = 0.35;
/// Radius, in world pixels, of an ordinary hit
pub const EXPLOSION_RADIUS: f64 = 12.0;
/// Safety cap on explosions playing at once
pub const MAX_EXPLOSIONS: usize = 128;

// ---------------------------------------------------------------------------
// Game loop
// ---------------------------------------------------------------------------
/// A frame longer than this many seconds is treated as this long (after a
/// pause or a stall), so the physics never takes a huge step
pub const MAX_FRAME_SECONDS: f64 = 0.05;

// ---------------------------------------------------------------------------
// Controls
// ---------------------------------------------------------------------------
/// Default keyboard keys, by name, in this order: thrust, spoiler, turn left,
/// turn right, fire. (Names, so the settings file stays readable and the
/// core needs no knowledge of any keyboard library.)
pub const DEFAULT_KEYS_PLAYER_1: [&str; 5] = ["w", "s", "a", "d", "left shift"];
pub const DEFAULT_KEYS_PLAYER_2: [&str; 5] = ["up", "down", "left", "right", "right shift"];

// ---------------------------------------------------------------------------
// Window and drawing
// ---------------------------------------------------------------------------
pub const WINDOW_TITLE: &str = "GravStar";

/// The sprites of the two ships, in the assets/ships folder
pub const SHIP_SPRITES: [&str; 2] = ["playerShip1_blue.png", "playerShip2_orange.png"];

/// The level played unless another is asked for, in the assets/levels folder
pub const DEFAULT_LEVEL: &str = "Test_Level.json";

/// Shield bubble radius as a fraction of the ship's width (before zoom)
pub const SHIELD_RADIUS_FACTOR: f64 = 0.8;
/// Shield bubble opacity (0 - 255) when the shield is full
pub const SHIELD_MAX_ALPHA: f64 = 120.0;

/// Engine flame size as a fraction of the ship's height
pub const FLAME_SIZE_FACTOR: f64 = 0.4;

/// Bullet trail: seconds of travel, limited to this range of world pixels
pub const BULLET_TRAIL_SECONDS: f64 = 0.016;
pub const BULLET_TRAIL_MIN: f64 = 2.0;
pub const BULLET_TRAIL_MAX: f64 = 16.0;
/// Bullet thickness and head radius, in world pixels
pub const BULLET_TRAIL_WIDTH: f64 = 3.0;
pub const BULLET_HEAD_RADIUS: f64 = 2.0;

/// Size of the frames-per-second counter (when switched on in the settings)
pub const FPS_TEXT_SIZE: f64 = 26.0;

/// The divider between the two views, in screen pixels
pub const DIVIDER_WIDTH: i32 = 2;

// ---------------------------------------------------------------------------
// Death effects
// ---------------------------------------------------------------------------
/// A destroyed ship: one big, slow explosion and a few smaller ones around it
pub const DEATH_EXPLOSION_RADIUS: f64 = 56.0;
pub const DEATH_EXPLOSION_SECONDS: f64 = 0.8;
pub const DEATH_SMALL_EXPLOSIONS: usize = 6;
pub const DEATH_SMALL_RADIUS_MIN: f64 = 16.0;
pub const DEATH_SMALL_RADIUS_MAX: f64 = 30.0;
pub const DEATH_SMALL_SECONDS_MIN: f64 = 0.4;
pub const DEATH_SMALL_SECONDS_MAX: f64 = 0.7;
/// How far from the ship the small ones can start (each way, pixels)
pub const DEATH_SMALL_SPREAD: f64 = 28.0;

// ---------------------------------------------------------------------------
// HUD (the strip along the bottom of the screen). Sizes are for a screen
// REFERENCE_SCREEN_HEIGHT high and scale with the screen.
// ---------------------------------------------------------------------------
pub const HUD_HEIGHT: f64 = 170.0;
pub const HUD_MARGIN: f64 = 8.0;
/// Widest a minimap may be
pub const HUD_MINIMAP_MAX_WIDTH: f64 = 400.0;
/// Space between a minimap and the player's status block
pub const HUD_MINIMAP_GAP: f64 = 24.0;
/// Height of the status block (name, lives, bars), which is centred in the strip
pub const HUD_STATUS_HEIGHT: f64 = 77.0;
/// The name / lives row, measured down from the top of the status block
pub const HUD_NAME_ROW_Y: f64 = 17.0;
/// The shield bar, measured down from the top of the status block
pub const HUD_BARS_Y: f64 = 43.0;
pub const HUD_NAME_TEXT_SIZE: f64 = 26.0;
pub const HUD_ICON_HEIGHT: f64 = 34.0;
pub const HUD_ICON_GAP: f64 = 10.0;
pub const HUD_NAME_ICON_GAP: f64 = 24.0;
pub const HUD_BAR_WIDTH: f64 = 260.0;
pub const HUD_BAR_HEIGHT: f64 = 14.0;
pub const HUD_BAR_GAP: f64 = 6.0;
pub const HUD_BAR_LABEL_SIZE: f64 = 14.0;
pub const HUD_BAR_LABEL_GAP: f64 = 10.0;
/// Opacity (0 - 255) of the icon of a life already lost
pub const HUD_LOST_LIFE_ALPHA: u8 = 55;

pub const HUD_BACKGROUND: [u8; 3] = [12, 14, 22];
pub const HUD_EDGE: [u8; 3] = [60, 64, 80];
pub const HUD_BAR_BACKGROUND: [u8; 3] = [26, 30, 44];
pub const HUD_LABEL_COLOUR: [u8; 3] = [170, 175, 190];
pub const SHIELD_BAR_COLOUR: [u8; 3] = [90, 200, 255];
/// Each player's colour (name, minimap dot)
pub const PLAYER_COLOURS: [[u8; 3]; 2] = [[120, 190, 255], [255, 170, 90]];

pub const HUD_SHIELD_LABEL: &str = "SHIELD";
pub const HUD_HULL_LABEL: &str = "HULL";
pub const HUD_PLAYER_LABEL: &str = "PLAYER";

// ---------------------------------------------------------------------------
// Minimap
// ---------------------------------------------------------------------------
/// Changed terrain shows on the minimap at most this often (seconds)
pub const MINIMAP_REFRESH_SECONDS: f64 = 0.5;
pub const MINIMAP_SPACE_COLOUR: [u8; 3] = [8, 10, 18];
pub const MINIMAP_TERRAIN_COLOUR: [u8; 3] = [175, 182, 198];
pub const MINIMAP_FRAME_COLOUR: [u8; 3] = [90, 96, 120];
/// Radius of a ship's dot, in pixels at the reference screen height
pub const MINIMAP_MARKER_RADIUS: f64 = 2.5;

// ---------------------------------------------------------------------------
// On-screen messages
// ---------------------------------------------------------------------------
pub const MESSAGE_COUNTDOWN_LABEL: &str = "GET READY";
pub const MESSAGE_GO: &str = "GO!";
pub const MESSAGE_VICTORY: &str = "VICTORY";
pub const MESSAGE_DEFEAT: &str = "DEFEAT";
pub const MESSAGE_DRAW: &str = "DRAW";
pub const MESSAGE_COUNTDOWN_LABEL_SIZE: f64 = 46.0;
pub const MESSAGE_COUNTDOWN_SIZE: f64 = 230.0;
pub const MESSAGE_GO_SIZE: f64 = 200.0;
pub const MESSAGE_RESULT_SIZE: f64 = 130.0;
/// Where each message sits, as a fraction of the view's height from its top
pub const MESSAGE_COUNTDOWN_LABEL_Y: f64 = 0.17;
pub const MESSAGE_COUNTDOWN_Y: f64 = 0.22;
pub const MESSAGE_RESULT_Y: f64 = 0.07;
pub const MESSAGE_COUNTDOWN_COLOUR: [u8; 3] = [255, 236, 150];
pub const MESSAGE_LABEL_COLOUR: [u8; 3] = [225, 228, 238];
pub const MESSAGE_GO_COLOUR: [u8; 3] = [140, 255, 150];
pub const MESSAGE_VICTORY_COLOUR: [u8; 3] = [120, 255, 140];
pub const MESSAGE_DEFEAT_COLOUR: [u8; 3] = [255, 90, 90];
pub const MESSAGE_DRAW_COLOUR: [u8; 3] = [255, 210, 90];
/// Large text gets a dark shadow this fraction of its size, down and right
pub const TEXT_SHADOW_FRACTION: f64 = 0.05;


// ---------------------------------------------------------------------------
// Settings (saved between runs)
// ---------------------------------------------------------------------------
/// The folder (inside the user's configuration folder) the settings file is
/// kept in, and the file's name
pub const SETTINGS_FOLDER: &str = "GravStar";
pub const SETTINGS_FILE: &str = "settings.json";
pub const SETTINGS_VERSION: u32 = 1;

pub const PLAYER_COUNT: usize = 2;

pub const DEFAULT_RESOLUTION: (u32, u32) = (1920, 1080);
pub const DEFAULT_FULLSCREEN: bool = false;
pub const DEFAULT_SMOOTH_SCALING: bool = true;
pub const DEFAULT_VSYNC: bool = true;
pub const DEFAULT_SHOW_FPS: bool = false;

/// Resolutions offered (further limited to what the monitor can show)
pub const RESOLUTION_CANDIDATES: [(u32, u32); 5] =
    [(1280, 720), (1600, 900), (1920, 1080), (2560, 1440), (3840, 2160)];
pub const MIN_RESOLUTION: (u32, u32) = (1280, 720);
pub const MAX_RESOLUTION: (u32, u32) = (7680, 4320);

/// Controller sticks: how far a stick must move before it counts, and how
/// its movement is curved (1.0 = straight, higher = gentler near the centre)
pub const DEADZONE_CHOICES: [f64; 8] = [0.05, 0.10, 0.15, 0.20, 0.25, 0.30, 0.35, 0.40];
pub const DEFAULT_DEADZONE: f64 = 0.15;
pub const CURVE_CHOICES: [(&str, f64); 3] = [("Linear", 1.0), ("Gentle", 1.5), ("Strong", 2.0)];
pub const DEFAULT_RESPONSE_CURVE: f64 = 1.0;

/// The highest controller button / axis / hat number accepted from a file
pub const MAX_BINDING_INDEX: i64 = 64;
/// The standard controller layout's default controls: left stick up / down
/// is thrust / brake, left stick left / right turns, button 0 fires
pub const DEFAULT_CONTROLLER_STICK_X: u8 = 0;
pub const DEFAULT_CONTROLLER_STICK_Y: u8 = 1;
pub const DEFAULT_CONTROLLER_FIRE_BUTTON: u8 = 0;

// ---------------------------------------------------------------------------
// Menus
// ---------------------------------------------------------------------------
/// Seconds a rebinding waits for a key press before giving up
pub const REBIND_TIMEOUT_SECONDS: f64 = 6.0;
/// Seconds a message ("could not save...") stays up
pub const MENU_MESSAGE_SECONDS: f64 = 3.0;
/// Seconds a game-over menu ignores Enter and clicks
pub const GAME_OVER_MENU_LOCK_SECONDS: f64 = 0.8;

pub const MENU_BACKGROUND: [u8; 3] = [14, 16, 28];
pub const MENU_TITLE_COLOUR: [u8; 3] = [255, 210, 90];
pub const MENU_TEXT_COLOUR: [u8; 3] = [225, 228, 238];
pub const MENU_DIM_TEXT_COLOUR: [u8; 3] = [140, 146, 165];
pub const MENU_HEADER_COLOUR: [u8; 3] = [130, 170, 230];
pub const MENU_VALUE_COLOUR: [u8; 3] = [140, 220, 255];
pub const MENU_SELECTED_COLOUR: [u8; 3] = [48, 72, 128];
pub const MENU_BORDER_COLOUR: [u8; 3] = [110, 150, 230];
pub const MENU_WARNING_COLOUR: [u8; 3] = [255, 170, 80];
pub const MENU_MODAL_COLOUR: [u8; 3] = [10, 12, 22];

/// How much the game behind a menu is dimmed (0 - 255)
pub const MENU_DIM_ALPHA: u8 = 175;
pub const GAME_OVER_MENU_DIM_ALPHA: u8 = 120;
pub const MODAL_DIM_ALPHA: u8 = 170;

/// Sizes at the reference screen height; positions as fractions of the height
pub const MENU_TITLE_SIZE: f64 = 84.0;
pub const MENU_TITLE_Y: f64 = 0.12;
pub const MENU_ROWS_TOP: f64 = 0.27;
/// Space kept free at the bottom of the screen (or of the area above the HUD
/// when a game is behind the menu) for the message and key hints
pub const MENU_ROWS_BOTTOM_RESERVE: f64 = 0.13;
pub const MENU_ROW_HEIGHT: f64 = 58.0;
pub const MENU_PANEL_WIDTH_FRACTION: f64 = 0.72;
pub const MENU_PANEL_MAX_WIDTH: f64 = 1180.0;
pub const MENU_ROW_TEXT_FRACTION: f64 = 0.62;
pub const MENU_ROW_PADDING: f64 = 22.0;
pub const MENU_MESSAGE_SIZE: f64 = 34.0;
/// How far above the bottom the message and the key hints sit
pub const MENU_MESSAGE_FROM_BOTTOM: f64 = 0.12;
pub const MENU_HINTS_SIZE: f64 = 28.0;
pub const MENU_HINTS_FROM_BOTTOM: f64 = 0.06;
pub const MENU_HINTS: &str = "Up/Down: select    Left/Right: change    Enter: choose    Esc: back";
pub const MENU_MODAL_WIDTH_FRACTION: f64 = 0.55;
pub const MENU_MODAL_HEIGHT: f64 = 210.0;
