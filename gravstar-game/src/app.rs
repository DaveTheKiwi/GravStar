//! The game program: opens the window, runs the menus and the game loop and
//! draws every frame. All the game logic is in gravstar-core.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use gravstar_core::camera::{Camera, ZoomSettings};
use gravstar_core::constants::*;
use gravstar_core::level_file;
use gravstar_core::menu::{MenuController, MenuEvent, Page};
use gravstar_core::random::Rng;
use gravstar_core::raster::AlphaCanvas;
use gravstar_core::settings::{available_resolutions, default_settings_path, GraphicsSettings, SettingsStore};
use gravstar_core::ship::{Ship, ShipInput};
use gravstar_core::ship_masks::ShipMaskSet;
use gravstar_core::tile_registry::TileRegistry;
use gravstar_core::tileset::TILE_DEFS;
use gravstar_core::world::World;
use sdl2::event::Event;
use sdl2::keyboard::Scancode;
use sdl2::mouse::MouseButton;
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{FullscreenType, Window, WindowContext, WindowPos};

use crate::assets::{find_assets_dir, load_png, Image};
use crate::bmp::Picture;
use crate::font::{Anchor, TextRenderer};
use crate::hud_view::{visible, HudView, MinimapView};
use crate::input::KeyBindings;
use crate::menu_view::MenuView;
use crate::ship_view::{draw_explosions, draw_projectiles, EffectTextures, ShipView};
use crate::terrain_view::TerrainView;

/// What to run, from the command line
pub struct Options {
    pub level: String,
    /// Developer option: use this screen size instead of the saved setting
    pub size: Option<(u32, u32)>,
    /// Developer option: start full screen
    pub fullscreen: bool,
    /// Draw with the CPU instead of the graphics card (needed where there
    /// is no graphics card, e.g. when testing without a screen)
    pub software: bool,
    /// Skip the menus and run a scripted flight (ending in a crash) with a
    /// fixed time step, so every run is identical
    pub demo: bool,
    /// Stop after this many frames
    pub frames: Option<u64>,
    /// Save the last frame as a BMP picture
    pub screenshot: Option<PathBuf>,
    /// Developer option: hold the cameras at this zoom (a fraction of the
    /// usual full zoom), to check how the drawing looks scaled
    pub force_zoom: Option<f64>,
    /// Lives each player starts with
    pub lives: u32,
    /// Keep the settings in this file instead of the usual place
    pub settings_path: Option<PathBuf>,
    /// Developer option: open this menu page at the start (main, settings,
    /// graphics, input, player1, player2, rebind, pause, gameover)
    pub menu: Option<String>,
    /// Developer option: key presses to feed in, one every few frames, as if
    /// typed (names such as "return", "down", "escape")
    pub press: Vec<String>,
}

impl Options {
    pub fn from_args(args: impl Iterator<Item = String>) -> Result<Options, String> {
        let mut o = Options {
            level: DEFAULT_LEVEL.to_string(),
            size: None,
            fullscreen: false,
            software: false,
            demo: false,
            frames: None,
            screenshot: None,
            force_zoom: None,
            lives: STARTING_LIVES,
            settings_path: None,
            menu: None,
            press: Vec::new(),
        };
        let mut args = args;
        while let Some(arg) = args.next() {
            let mut value = |what: &str| args.next().ok_or_else(|| format!("{arg} needs {what}"));
            match arg.as_str() {
                "--level" => o.level = value("a level file name")?,
                "--size" => {
                    let v = value("a size such as 1280x720")?;
                    let (w, h) = v.split_once('x').ok_or("--size needs a size such as 1280x720")?;
                    o.size = Some((w.parse().map_err(|_| "bad width in --size")?,
                                   h.parse().map_err(|_| "bad height in --size")?));
                }
                "--fullscreen" => o.fullscreen = true,
                "--software" => o.software = true,
                "--demo" => o.demo = true,
                "--lives" => o.lives = value("a number")?.parse().map_err(|_| "bad number for --lives")?,
                "--frames" => o.frames = Some(value("a number")?.parse().map_err(|_| "bad number for --frames")?),
                "--force-zoom" => o.force_zoom = Some(value("a number such as 0.7")?.parse().map_err(|_| "bad number for --force-zoom")?),
                "--screenshot" => o.screenshot = Some(PathBuf::from(value("a file name")?)),
                "--settings" => o.settings_path = Some(PathBuf::from(value("a file name")?)),
                "--menu" => o.menu = Some(value("a page name")?),
                "--press" => o.press = value("a list of keys")?.split(',').map(|k| k.trim().to_string()).collect(),
                "--help" | "-h" => return Err(USAGE.to_string()),
                other => return Err(format!("unknown option {other}\n{USAGE}")),
            }
        }
        Ok(o)
    }
}

const USAGE: &str = "Usage: gravstar-game [options]
Normally no options are needed: display and control settings are in the menus.
  --level NAME         level file in assets/levels (default Test_Level.json)
  --lives N            lives each player starts with (default 3)
  --settings FILE      keep the settings in this file
Developer options:
  --size WxH           use this screen size (not saved)
  --fullscreen         start full screen (not saved)
  --software           draw with the CPU
  --demo               skip the menus; scripted flight ending in a crash
  --frames N           quit after N frames
  --screenshot FILE    save the last frame as a BMP
  --force-zoom Z       hold the zoom at Z (e.g. 0.7), to check scaled drawing
  --menu PAGE          open a menu page (main settings graphics input player1
                       player2 rebind pause gameover)
  --press KEYS         feed in key presses, e.g. return,down,escape
Player 1 starts with W S A D and left Shift, player 2 with the arrow keys and
right Shift; both can be changed in Settings > Input.  F11 toggles full screen.";

/// A simple arrow, used if a ship's picture cannot be loaded
fn placeholder_sprite() -> Image {
    let size = 32;
    let mut canvas = AlphaCanvas::new(size, size);
    canvas.fill_polygon(&[(16, 0), (0, 32), (16, 24), (32, 32)], 255);
    let rgba = canvas.alpha().iter().flat_map(|a| [100, 200, 255, *a]).collect();
    Image { width: size, height: size, rgba }
}

/// The scripted flight used by --demo
fn demo_inputs(frame: u64) -> [ShipInput; 2] {
    let f = frame as f64;
    [
        ShipInput {
            thrust: if (frame / 45) % 4 == 3 { 0.0 } else { 1.0 },
            rotation: (f / 70.0).sin(),
            spoiler: 0.0,
            fire: frame % 8 < 4,
            pickup_cargo: false,
        },
        ShipInput {
            thrust: if (frame / 60) % 3 == 0 { 1.0 } else { 0.0 },
            rotation: 0.4,
            spoiler: 0.0,
            fire: frame % 12 < 3,
            pickup_cargo: false,
        },
    ]
}

/// With --press, a key is pressed this often (in frames)
const PRESS_EVERY_FRAMES: u64 = 4;

/// The demo's crash: player 1's ship is wrecked once its protection has run out
const DEMO_CRASH_FRAME: u64 = 420;

/// Everything needed to start a game, loaded once
struct Setup {
    assets: PathBuf,
    registry: TileRegistry,
    sprites: Vec<Image>,
    masks: Vec<Arc<ShipMaskSet>>,
}

impl Setup {
    fn load() -> Result<Setup, String> {
        let assets = find_assets_dir().ok_or(
            "cannot find the assets folder (it should have a levels folder inside it)")?;
        let (mut sprites, mut masks) = (Vec::new(), Vec::new());
        let template = Ship::new(0.0, 0.0);
        for name in SHIP_SPRITES {
            let path = assets.join("ships").join(name);
            let sprite = load_png(&path).unwrap_or_else(|e| {
                eprintln!("warning: {e} - using a plain arrow");
                placeholder_sprite()
            });
            masks.push(Arc::new(ShipMaskSet::bake(
                &sprite.rgba, sprite.width, sprite.height,
                template.physics.width.round() as usize, template.physics.height.round() as usize,
                SHIP_MASK_STEPS)));
            sprites.push(sprite);
        }
        Ok(Setup { assets, registry: TileRegistry::from_tileset(), sprites, masks })
    }

    /// A fresh game on a freshly loaded level
    fn new_world(&self, o: &Options, seed: u64) -> Result<World, String> {
        let path = self.assets.join("levels").join(&o.level);
        let (level, warnings) = level_file::load(&path, &self.registry)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        for warning in warnings {
            eprintln!("{warning}");
        }
        let mut rng = Rng::new(seed);
        let spawns = World::pick_spawns(&level, &mut rng, 2)
            .ok_or("the level needs at least 2 spawn points")?;
        let ships = self.masks.iter().zip(&spawns).map(|(masks, spawn)| {
            let mut ship = Ship::new(spawn.0, spawn.1);
            ship.mask_set = Some(masks.clone());
            ship
        }).collect();
        Ok(World::new(level, self.registry.clone(), ships, seed ^ 0x5DEECE66D, o.lives))
    }
}

fn fresh_seed(o: &Options) -> u64 {
    if o.demo {
        12345
    } else {
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1)
    }
}

/// A game in progress
struct Game {
    world: World,
    minimap: MinimapView,
    /// The game-over menu has been put up for this game
    game_over_menu_shown: bool,
}

impl Game {
    fn new(setup: &Setup, o: &Options, tc: &TextureCreator<WindowContext>) -> Result<Game, String> {
        let world = setup.new_world(o, fresh_seed(o))?;
        let minimap = MinimapView::new(tc, &world)?;
        Ok(Game { world, minimap, game_over_menu_shown: false })
    }
}

/// Make the window and drawing match the graphics settings
fn apply_graphics(canvas: &mut Canvas<Window>, g: &GraphicsSettings, terrain: &mut TerrainView) -> Result<(), String> {
    let (w, h) = g.resolution;
    {
        let window = canvas.window_mut();
        if g.fullscreen {
            window.set_fullscreen(FullscreenType::Desktop)?;
        } else {
            window.set_fullscreen(FullscreenType::Off)?;
            window.set_size(w, h).map_err(|e| e.to_string())?;
            window.set_position(WindowPos::Centered, WindowPos::Centered);
        }
    }
    // The screen size in the settings is the size the game is drawn at; the
    // window or monitor then scales it to fit
    canvas.set_logical_size(w, h).map_err(|e| e.to_string())?;
    // SAFETY: the renderer is valid for as long as the canvas is
    unsafe { sdl2::sys::SDL_RenderSetVSync(canvas.raw(), if g.vsync { 1 } else { 0 }) };
    terrain.set_smooth(g.smooth_scaling);
    Ok(())
}

/// The pages a --menu name stands for, and whether a game is needed behind them
fn pages_named(name: &str) -> Option<(bool, Vec<Page>)> {
    let settings = vec![Page::Main, Page::Settings];
    let input = vec![Page::Main, Page::Settings, Page::Input];
    Some(match name {
        "main" => (false, vec![Page::Main]),
        "settings" => (false, settings),
        "graphics" => (false, vec![Page::Main, Page::Settings, Page::Graphics]),
        "input" => (false, input),
        "player1" | "rebind" => (false, vec![Page::Main, Page::Settings, Page::Input, Page::Player(0)]),
        "player2" => (false, vec![Page::Main, Page::Settings, Page::Input, Page::Player(1)]),
        "pause" => (true, vec![Page::Pause]),
        "gameover" => (true, vec![Page::GameOver]),
        _ => return None,
    })
}

/// What the player did while a menu is open
fn menu_event(event: &Event, menus: &mut MenuController, view: &MenuView) {
    match event {
        Event::KeyDown { scancode: Some(code), repeat, .. } => {
            if menus.is_capturing() {
                if !*repeat {
                    menus.handle_key(&code.name().to_lowercase());
                }
                return;
            }
            match code {
                Scancode::Up => menus.navigate(-1),
                Scancode::Down => menus.navigate(1),
                Scancode::Left => menus.adjust(-1),
                Scancode::Right => menus.adjust(1),
                Scancode::Return | Scancode::KpEnter | Scancode::Space if !*repeat => menus.activate(),
                Scancode::Delete | Scancode::Backspace if !*repeat => menus.clear_selected(),
                Scancode::Escape if !*repeat => menus.back(),
                _ => {}
            }
        }
        Event::MouseMotion { x, y, .. } => {
            if let Some(row) = view.hit_test(*x, *y) {
                menus.select_row(row);
            }
        }
        Event::MouseButtonDown { mouse_btn, x, y, .. } => {
            if menus.is_capturing() {
                menus.back(); // a click cancels a rebind
                return;
            }
            if let Some(row) = view.hit_test(*x, *y) {
                menus.select_row(row);
                match mouse_btn {
                    MouseButton::Left => menus.activate(),
                    MouseButton::Right => menus.adjust(-1),
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

pub fn run(o: &Options) -> Result<(), String> {
    let setup = Setup::load()?;

    // ---- settings
    let mut store = SettingsStore::new(o.settings_path.clone().unwrap_or_else(default_settings_path));
    let mut settings = store.load();
    if let Some(problem) = &store.last_error {
        eprintln!("{problem}");
    }
    if let Some(size) = o.size {
        settings.graphics.resolution = size;
    }
    if o.fullscreen {
        settings.graphics.fullscreen = true;
    }

    // ---- the window
    let sdl = sdl2::init()?;
    let video = sdl.video()?;
    sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "linear");
    let monitor = video.desktop_display_mode(0).map_err(|e| e.to_string())?;
    let desktop = (monitor.w as u32, monitor.h as u32);
    // A saved size bigger than this monitor (settings from another machine)
    // would not fit: use the largest that does
    let g = &mut settings.graphics;
    if g.resolution.0 > desktop.0 || g.resolution.1 > desktop.1 {
        g.resolution = *available_resolutions(desktop, None).last().unwrap();
    }
    let mut window = video.window(WINDOW_TITLE, g.resolution.0, g.resolution.1);
    window.position_centered().resizable();
    let window = window.build().map_err(|e| e.to_string())?;
    let canvas = window.into_canvas();
    let mut canvas = if o.software {
        canvas.software().build()
    } else if g.vsync {
        canvas.accelerated().present_vsync().build()
    } else {
        canvas.accelerated().build()
    }.map_err(|e| e.to_string())?;
    let tc = canvas.texture_creator();
    let mut events = sdl.event_pump()?;
    let mouse = sdl.mouse();

    // ---- what to draw with
    let mut terrain = TerrainView::new(&tc, &setup.registry, TILE_DEFS.iter().map(|d| d.id))?;
    apply_graphics(&mut canvas, &settings.graphics, &mut terrain)?;
    let mut ship_views = Vec::new();
    {
        let template = Ship::new(0.0, 0.0);
        for sprite in &setup.sprites {
            ship_views.push(ShipView::new(&tc, sprite, &template)?);
        }
    }
    let mut effects = EffectTextures::new(&tc)?;
    let mut text = TextRenderer::new()?;
    let mut hud = HudView::new(setup.sprites.iter().map(|s| Image {
        width: s.width, height: s.height, rgba: s.rgba.clone() }).collect());
    let mut menu_view = MenuView::default();
    let mut flame_rng = Rng::new(fresh_seed(o) ^ 0xF1A3E);
    let mut cameras = [Camera::new(0, 0, 1, 1), Camera::new(0, 0, 1, 1)];

    // ---- menus and controls
    let mut menus = MenuController::new(settings);
    menus.info.resolutions = available_resolutions(desktop, Some(menus.settings.graphics.resolution));
    menus.info.display_info = format!("Monitor: {} x {} at {} Hz", desktop.0, desktop.1, monitor.refresh_rate);
    menus.info.controller_names = vec![None; PLAYER_COUNT];
    let mut keys: Vec<KeyBindings> = menus.settings.players.iter().map(KeyBindings::from_settings).collect();

    // ---- what to show first
    let mut game: Option<Game> = None;
    if o.demo {
        game = Some(Game::new(&setup, o, &tc)?);
    } else if let Some(name) = &o.menu {
        let (needs_game, pages) = pages_named(name).ok_or_else(|| format!("no menu page called {name}"))?;
        if needs_game {
            game = Some(Game::new(&setup, o, &tc)?);
        }
        menus.open_pages(&pages);
        if name == "rebind" {
            let at = menus.rows().iter().position(|r| r.label == "Fire").unwrap_or(0);
            menus.select_row(at);
            menus.activate();
        }
    } else {
        menus.open_main_menu();
    }

    // ---- the loop
    let mut last = Instant::now();
    let mut frame: u64 = 0;
    let (mut fps_time, mut fps_frames, mut fps) = (0.0f64, 0u32, 0.0f64);
    'main: loop {
        let mut pending: Vec<Event> = events.poll_iter().collect();
        if frame % PRESS_EVERY_FRAMES == 1 {
            if let Some(name) = o.press.get((frame / PRESS_EVERY_FRAMES) as usize) {
                let code = Scancode::from_name(name).ok_or_else(|| format!("--press: no key called {name}"))?;
                pending.push(Event::KeyDown { timestamp: 0, window_id: 0, keycode: None, scancode: Some(code),
                                              keymod: sdl2::keyboard::Mod::empty(), repeat: false });
            }
        }
        for event in pending {
            match &event {
                Event::Quit { .. } => break 'main,
                Event::KeyDown { scancode: Some(Scancode::F11), repeat: false, .. } => {
                    let g = &mut menus.settings.graphics;
                    g.fullscreen = !g.fullscreen;
                    apply_graphics(&mut canvas, &menus.settings.graphics, &mut terrain)?;
                    store.save(&menus.settings);
                    continue;
                }
                _ => {}
            }
            if menus.is_open() {
                menu_event(&event, &mut menus, &menu_view);
            } else if let Event::KeyDown { scancode: Some(Scancode::Escape), repeat: false, .. } = event {
                if game.is_some() {
                    menus.open_pause_menu();
                }
            }
        }

        // What the menus asked for
        for event in menus.take_events() {
            match event {
                MenuEvent::Play | MenuEvent::NewGame => {
                    match Game::new(&setup, o, &tc) {
                        Ok(new_game) => {
                            terrain.forget_damage();
                            game = Some(new_game);
                            menus.close();
                        }
                        Err(problem) => {
                            eprintln!("{problem}");
                            menus.say(&format!("Could not start the game: {problem}"));
                        }
                    }
                }
                MenuEvent::Resume => menus.close(),
                MenuEvent::QuitToMenu => {
                    game = None;
                    menus.open_main_menu();
                }
                MenuEvent::QuitApp => break 'main,
                MenuEvent::GraphicsChanged => {
                    if let Err(problem) = apply_graphics(&mut canvas, &menus.settings.graphics, &mut terrain) {
                        eprintln!("{problem}");
                        menus.say("That display mode is not available");
                    }
                    menus.info.resolutions = available_resolutions(desktop, Some(menus.settings.graphics.resolution));
                }
                MenuEvent::InputChanged => {
                    keys = menus.settings.players.iter().map(KeyBindings::from_settings).collect();
                }
                MenuEvent::SaveSettings => {
                    if !store.save(&menus.settings) {
                        eprintln!("{}", store.last_error.clone().unwrap_or_default());
                        menus.say("Could not save settings");
                    }
                }
            }
        }
        // The pointer only shows while a menu is up
        mouse.show_cursor(menus.is_open() || game.is_none());

        let now = Instant::now();
        let real_dt = now.duration_since(last).as_secs_f64().min(MAX_FRAME_SECONDS);
        last = now;
        let dt = if o.demo { 1.0 / 60.0 } else { real_dt };
        menus.update(real_dt);
        menus.info.pacing_info = if menus.settings.graphics.vsync {
            format!("Frame rate: matched to the monitor ({} FPS)", monitor.refresh_rate)
        } else {
            "Frame rate: unlimited".to_string()
        };

        // ---- the game moves on (unless a menu has it paused)
        let (screen_w, screen_h) = canvas.logical_size();
        let scale = screen_h as f64 / REFERENCE_SCREEN_HEIGHT;
        let hud_height = ((HUD_HEIGHT * scale).round() as u32).min(screen_h / 2);
        let view_height = screen_h - hud_height;
        let half = (screen_w / 2) as i32;
        let playing = !menus.is_open();
        if let Some(g) = game.as_mut() {
            if playing {
                let inputs = if o.demo {
                    demo_inputs(frame)
                } else {
                    let state = events.keyboard_state();
                    [keys[0].read(&state), keys[1].read(&state)]
                };
                if o.demo && frame == DEMO_CRASH_FRAME {
                    g.world.ships[0].take_damage(1e9);
                }
                let report = g.world.step(dt, &inputs);
                g.minimap.note_changed(&report.changed_tiles);
                g.minimap.update(&g.world, dt)?;
                if g.world.match_state.menu_due() && !g.game_over_menu_shown {
                    g.game_over_menu_shown = true;
                    menus.open_game_over_menu();
                }
            }

            // ---- the cameras follow the ships
            let views = [(0, half), (half, screen_w as i32 - half)];
            for (cam, (x, w)) in cameras.iter_mut().zip(views) {
                cam.viewport_x = x;
                cam.viewport_y = 0;
                cam.viewport_width = w;
                cam.viewport_height = view_height as i32;
            }
            let (world_w, world_h) = (g.world.width_px(), g.world.height_px());
            let zoom = ZoomSettings::for_screen(screen_h as f64, &cameras[0], world_w, world_h);
            for (cam, ship) in cameras.iter_mut().zip(&g.world.ships) {
                cam.update_zoom_for_speed(ship.speed(), &zoom, if playing { dt } else { 0.0 });
                if let Some(z) = o.force_zoom {
                    cam.zoom = z * scale;
                }
                cam.snap(ship.x, ship.y);
                cam.clamp(0.0, 0.0, world_w, world_h);
            }
        }

        // ---- draw
        canvas.set_draw_color(Color::RGB(0, 0, 0));
        canvas.clear();
        if let Some(g) = game.as_mut() {
            for cam in &cameras {
                canvas.set_clip_rect(Rect::new(cam.viewport_x, cam.viewport_y,
                                               cam.viewport_width as u32, cam.viewport_height as u32));
                terrain.draw(&mut canvas, &tc, cam, &g.world.level, &g.world.registry)?;
                for (view, ship) in ship_views.iter_mut().zip(&g.world.ships) {
                    if !visible(ship) {
                        continue;
                    }
                    // Timed protection after a respawn: a gentle pulse
                    let alpha = if ship.spawn_protected() {
                        (205.0 + 50.0 * (ship.immune_time * std::f64::consts::TAU * 3.0).sin()) as u8
                    } else {
                        255
                    };
                    view.draw(&mut canvas, &mut effects, &mut flame_rng, cam, ship, alpha)?;
                }
                draw_projectiles(&mut canvas, &mut effects, cam, &g.world.projectiles.projectiles)?;
                draw_explosions(&mut canvas, &mut effects, cam, &g.world.explosions.explosions)?;
            }
            canvas.set_clip_rect(None);
            HudView::draw_messages(&mut canvas, &tc, &mut text, &cameras, &g.world.match_state, scale)?;
            canvas.set_draw_color(Color::RGB(50, 50, 50));
            canvas.fill_rect(Rect::new(half - DIVIDER_WIDTH / 2, 0, DIVIDER_WIDTH as u32, view_height))?;
            hud.draw_strip(&mut canvas, &tc, &mut text, &mut effects, &g.minimap, &g.world,
                           Rect::new(0, view_height as i32, screen_w, hud_height), scale)?;
        }
        if menus.is_open() {
            menu_view.draw(&mut canvas, &tc, &mut text, &menus, (screen_w, screen_h), game.is_some(), view_height)?;
        }
        if menus.settings.graphics.show_fps {
            text.draw(&mut canvas, &tc, &format!("{:.0} FPS", fps), ((12.0 * scale) as i32, (8.0 * scale) as i32),
                      FPS_TEXT_SIZE * scale, [235, 238, 245], Anchor::TopLeft, true)?;
        }

        frame += 1;
        let finished = o.frames.map_or(false, |n| frame >= n);
        if finished {
            if let Some(path) = &o.screenshot {
                let bytes = canvas.read_pixels(None, PixelFormatEnum::RGB24)?;
                let (w, h) = canvas.output_size()?;
                Picture::from_rgb24(w as usize, h as usize, &bytes)
                    .save_bmp(path).map_err(|e| format!("{}: {e}", path.display()))?;
            }
        }
        canvas.present();
        if finished {
            break;
        }

        // ---- frames per second
        fps_time += real_dt;
        fps_frames += 1;
        if fps_time >= 0.5 {
            fps = fps_frames as f64 / fps_time;
            fps_time = 0.0;
            fps_frames = 0;
        }
    }
    Ok(())
}
