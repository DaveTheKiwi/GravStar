//! The strip along the bottom of the screen (name, lives, shield and hull
//! bars and a minimap for each player) and the big messages in each view
//! (the 3 - 2 - 1 countdown, VICTORY / DEFEAT).

use std::collections::{HashMap, HashSet};

use gravstar_core::camera::Camera;
use gravstar_core::constants::*;
use gravstar_core::match_rules::{Match, Outcome};
use gravstar_core::minimap::{tile_coverage, world_to_minimap};
use gravstar_core::ship::Ship;
use gravstar_core::world::World;
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::{Canvas, Texture, TextureCreator};
use sdl2::video::{Window, WindowContext};

use crate::assets::{scale_image, Image};
use crate::font::{Anchor, TextRenderer};
use crate::ship_view::EffectTextures;
use crate::textures;

fn colour(c: [u8; 3]) -> Color {
    Color::RGB(c[0], c[1], c[2])
}

/// The whole level as a small picture, one pixel per tile, shaded by how
/// solid each tile is. Terrain damage shows up at most twice a second.
pub struct MinimapView {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
    texture: Texture,
    changed: HashSet<(usize, usize)>,
    since_refresh: f64,
}

impl MinimapView {
    pub fn new(tc: &TextureCreator<WindowContext>, world: &World) -> Result<Self, String> {
        let (width, height) = (world.level.width, world.level.height);
        let mut view = MinimapView {
            width,
            height,
            pixels: vec![255; width * height * 4],
            texture: textures::from_rgba(tc, width as u32, height as u32, &vec![0u8; width * height * 4])?,
            changed: HashSet::new(),
            since_refresh: 0.0,
        };
        for row in 0..height {
            for col in 0..width {
                view.paint(world, col, row);
            }
        }
        view.upload()?;
        Ok(view)
    }

    fn paint(&mut self, world: &World, col: usize, row: usize) {
        let tile = &world.level.tiles[row * self.width + col];
        let coverage = tile_coverage(tile, &world.registry);
        let at = (row * self.width + col) * 4;
        for k in 0..3 {
            let (space, terrain) = (MINIMAP_SPACE_COLOUR[k] as f64, MINIMAP_TERRAIN_COLOUR[k] as f64);
            self.pixels[at + k] = (space + (terrain - space) * coverage) as u8;
        }
        self.pixels[at + 3] = 255;
    }

    fn upload(&mut self) -> Result<(), String> {
        self.texture.update(None, &self.pixels, self.width * 4).map_err(|e| e.to_string())
    }

    /// Note terrain that has changed; it is redrawn within MINIMAP_REFRESH_SECONDS
    pub fn note_changed(&mut self, tiles: &[(usize, usize)]) {
        self.changed.extend(tiles.iter().copied());
    }

    pub fn update(&mut self, world: &World, dt: f64) -> Result<(), String> {
        self.since_refresh += dt;
        if !self.changed.is_empty() && self.since_refresh >= MINIMAP_REFRESH_SECONDS {
            let tiles: Vec<_> = self.changed.drain().collect();
            for (col, row) in tiles {
                self.paint(world, col, row);
            }
            self.since_refresh = 0.0;
            self.upload()?;
        }
        Ok(())
    }

    /// The largest map-shaped rectangle that fits in `bounds`, against its
    /// left or right edge, centred up and down
    pub fn fit(&self, bounds: Rect, align_right: bool) -> Rect {
        let aspect = self.width as f64 / self.height as f64;
        let (mut w, mut h) = ((bounds.height() as f64 * aspect) as u32, bounds.height());
        if w > bounds.width() {
            w = bounds.width();
            h = (bounds.width() as f64 / aspect) as u32;
        }
        let (w, h) = (w.max(1), h.max(1));
        let x = if align_right { bounds.right() - w as i32 } else { bounds.left() };
        Rect::new(x, bounds.center().y() - h as i32 / 2, w, h)
    }

    /// Draw the map in `rect` with a dot for each ship given as (x, y,
    /// colour, is this the viewer's own ship)
    pub fn draw(&self, canvas: &mut Canvas<Window>, fx: &mut EffectTextures, rect: Rect,
                world: &World, markers: &[(f64, f64, [u8; 3], bool)], scale: f64) -> Result<(), String> {
        canvas.copy(&self.texture, None, rect)?;
        canvas.set_draw_color(colour(MINIMAP_FRAME_COLOUR));
        canvas.draw_rect(Rect::new(rect.x() - 1, rect.y() - 1, rect.width() + 2, rect.height() + 2))?;

        let radius = ((MINIMAP_MARKER_RADIUS * scale) as i32).max(2);
        // The viewer's own dot is drawn last, so it is never hidden
        let mut ordered: Vec<_> = markers.to_vec();
        ordered.sort_by_key(|m| m.3);
        for (x, y, c, own) in ordered {
            let (mx, my) = world_to_minimap(x, y, world.width_px(), world.height_px(),
                                            rect.width() as f64, rect.height() as f64);
            let (px, py) = (rect.x() + mx as i32, rect.y() + my as i32);
            fx.draw_disc(canvas, px, py, radius + 1, if own { (255, 255, 255) } else { (0, 0, 0) }, 255)?;
            fx.draw_disc(canvas, px, py, radius, (c[0], c[1], c[2]), 255)?;
        }
        Ok(())
    }
}

pub struct HudView {
    /// Each player's ship picture, for the lives icons
    sprites: Vec<Image>,
    /// The icons, by (player, height in pixels)
    icons: HashMap<(usize, u32), Texture>,
}

impl HudView {
    pub fn new(sprites: Vec<Image>) -> Self {
        HudView { sprites, icons: HashMap::new() }
    }

    fn icon(&mut self, tc: &TextureCreator<WindowContext>, player: usize, height: u32)
            -> Result<&mut Texture, String> {
        if !self.icons.contains_key(&(player, height)) {
            let sprite = &self.sprites[player];
            let width = ((sprite.width as f64 * height as f64 / sprite.height as f64).round() as u32).max(1);
            let scaled = scale_image(sprite, width as usize, height as usize);
            self.icons.insert((player, height), textures::from_rgba(tc, width, height, &scaled.rgba)?);
        }
        Ok(self.icons.get_mut(&(player, height)).unwrap())
    }

    /// Green when healthy, through yellow to red when nearly gone
    fn hull_colour(fraction: f64) -> [u8; 3] {
        let f = fraction.clamp(0.0, 1.0);
        if f > 0.5 {
            [(255.0 * (1.0 - f) * 2.0) as u8, 220, 60]
        } else {
            [235, (220.0 * f * 2.0) as u8, 60]
        }
    }

    /// A labelled bar that empties towards the screen edge it is on
    #[allow(clippy::too_many_arguments)]
    fn bar(canvas: &mut Canvas<Window>, tc: &TextureCreator<WindowContext>, text: &mut TextRenderer,
           rect: Rect, fraction: f64, fill: [u8; 3], label: &str, scale: f64,
           from_right: bool) -> Result<(), String> {
        canvas.set_draw_color(colour(HUD_BAR_BACKGROUND));
        canvas.fill_rect(rect)?;
        let w = (rect.width() as f64 * fraction.clamp(0.0, 1.0)) as u32;
        if w > 0 {
            let x = if from_right { rect.right() - w as i32 } else { rect.x() };
            canvas.set_draw_color(colour(fill));
            canvas.fill_rect(Rect::new(x, rect.y(), w, rect.height()))?;
        }
        canvas.set_draw_color(colour(HUD_EDGE));
        canvas.draw_rect(rect)?;
        let gap = (HUD_BAR_LABEL_GAP * scale) as i32;
        let size = HUD_BAR_LABEL_SIZE * scale;
        if from_right {
            text.draw(canvas, tc, label, (rect.left() - gap, rect.center().y()), size,
                      HUD_LABEL_COLOUR, Anchor::MidRight, false)?;
        } else {
            text.draw(canvas, tc, label, (rect.right() + gap, rect.center().y()), size,
                      HUD_LABEL_COLOUR, Anchor::MidLeft, false)?;
        }
        Ok(())
    }

    /// The whole strip. Player 1 is on the left of the screen and player 2
    /// mirrored on the right; each has a map on the outer edge and a status
    /// block beside it.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_strip(&mut self, canvas: &mut Canvas<Window>, tc: &TextureCreator<WindowContext>,
                      text: &mut TextRenderer, fx: &mut EffectTextures, minimap: &MinimapView,
                      world: &World, strip: Rect, scale: f64) -> Result<(), String> {
        canvas.set_draw_color(colour(HUD_BACKGROUND));
        canvas.fill_rect(strip)?;
        canvas.set_draw_color(colour(HUD_EDGE));
        canvas.fill_rect(Rect::new(strip.x(), strip.y(), strip.width(), 2))?;
        canvas.fill_rect(Rect::new(strip.center().x() - 1, strip.y(), 2, strip.height()))?;

        // The maps, on the outer edges
        let margin = (HUD_MARGIN * scale) as i32;
        let box_width = (HUD_MINIMAP_MAX_WIDTH * scale) as u32;
        let box_height = (strip.height() as i32 - 2 * margin).max(1) as u32;
        let left_box = Rect::new(strip.x() + margin, strip.y() + margin, box_width, box_height);
        let right_box = Rect::new(strip.right() - margin - box_width as i32, strip.y() + margin,
                                  box_width, box_height);
        let map_rects = [minimap.fit(left_box, false), minimap.fit(right_box, true)];

        let gap = (HUD_MINIMAP_GAP * scale) as i32;
        let info_edge = [map_rects[0].right() + gap, map_rects[1].left() - gap];

        let block_top = strip.y() + (strip.height() as i32 - (HUD_STATUS_HEIGHT * scale) as i32) / 2;
        let name_y = block_top + (HUD_NAME_ROW_Y * scale) as i32;
        let icon_height = ((HUD_ICON_HEIGHT * scale) as u32).max(8);
        let icon_gap = (HUD_ICON_GAP * scale) as i32;
        let bar_w = (HUD_BAR_WIDTH * scale) as u32;
        let bar_h = ((HUD_BAR_HEIGHT * scale) as u32).max(6);

        for player in 0..world.ships.len().min(2) {
            let left = player == 0;
            let ship = &world.ships[player];
            let lives = world.match_state.lives(player) as usize;
            let own_colour = PLAYER_COLOURS[player % 2];
            let label = format!("{HUD_PLAYER_LABEL} {}", player + 1);

            // Name and a ship icon for each life (lost ones drawn dim)
            let mut x = info_edge[player];
            if left {
                let name = text.draw(canvas, tc, &label, (x, name_y), HUD_NAME_TEXT_SIZE * scale,
                                     own_colour, Anchor::MidLeft, false)?;
                x = name.right() + (HUD_NAME_ICON_GAP * scale) as i32;
            }
            for life in 0..(world.match_state.starting_lives as usize).max(lives) {
                let icon = self.icon(tc, player, icon_height)?;
                let (w, h) = (icon.query().width, icon.query().height);
                icon.set_alpha_mod(if life < lives { 255 } else { HUD_LOST_LIFE_ALPHA });
                if left {
                    canvas.copy(icon, None, Rect::new(x, name_y - h as i32 / 2, w, h))?;
                    x += w as i32 + icon_gap;
                } else {
                    canvas.copy(icon, None, Rect::new(x - w as i32, name_y - h as i32 / 2, w, h))?;
                    x -= w as i32 + icon_gap;
                }
            }
            if !left {
                text.draw(canvas, tc, &label, (x - (14.0 * scale) as i32, name_y),
                          HUD_NAME_TEXT_SIZE * scale, own_colour, Anchor::MidRight, false)?;
            }

            // Shield and hull bars
            let bar_x = if left { info_edge[player] } else { info_edge[player] - bar_w as i32 };
            let top = block_top + (HUD_BARS_Y * scale) as i32;
            Self::bar(canvas, tc, text, Rect::new(bar_x, top, bar_w, bar_h),
                      ship.health.shield_fraction(), SHIELD_BAR_COLOUR, HUD_SHIELD_LABEL, scale, !left)?;
            Self::bar(canvas, tc, text,
                      Rect::new(bar_x, top + bar_h as i32 + (HUD_BAR_GAP * scale) as i32, bar_w, bar_h),
                      ship.health.hull_fraction(), Self::hull_colour(ship.health.hull_fraction()),
                      HUD_HULL_LABEL, scale, !left)?;

            // The map. What a player's map shows is decided here: only their
            // own ship, not the opponent's.
            let markers: Vec<(f64, f64, [u8; 3], bool)> = world.ships.iter().enumerate()
                .filter(|(i, s)| *i == player && !s.dead)
                .map(|(i, s)| (s.x, s.y, PLAYER_COLOURS[i % 2], true))
                .collect();
            minimap.draw(canvas, fx, map_rects[player], world, &markers, scale)?;
        }
        Ok(())
    }

    /// The big message in each view: the countdown, GO!, or the result
    pub fn draw_messages(canvas: &mut Canvas<Window>, tc: &TextureCreator<WindowContext>,
                         text: &mut TextRenderer, cameras: &[Camera], state: &Match,
                         scale: f64) -> Result<(), String> {
        for (player, cam) in cameras.iter().enumerate() {
            let cx = cam.viewport_x + cam.viewport_width / 2;
            let (top, h) = (cam.viewport_y, cam.viewport_height as f64);

            if let Some(outcome) = state.outcome(player) {
                let (message, c) = match outcome {
                    Outcome::Victory => (MESSAGE_VICTORY, MESSAGE_VICTORY_COLOUR),
                    Outcome::Defeat => (MESSAGE_DEFEAT, MESSAGE_DEFEAT_COLOUR),
                    Outcome::Draw => (MESSAGE_DRAW, MESSAGE_DRAW_COLOUR),
                };
                text.draw(canvas, tc, message, (cx, top + (h * MESSAGE_RESULT_Y) as i32),
                          MESSAGE_RESULT_SIZE * scale, c, Anchor::MidTop, true)?;
            } else if let Some(number) = state.countdown_number(player) {
                text.draw(canvas, tc, MESSAGE_COUNTDOWN_LABEL,
                          (cx, top + (h * MESSAGE_COUNTDOWN_LABEL_Y) as i32),
                          MESSAGE_COUNTDOWN_LABEL_SIZE * scale, MESSAGE_LABEL_COLOUR, Anchor::MidTop, true)?;
                text.draw(canvas, tc, &number.to_string(), (cx, top + (h * MESSAGE_COUNTDOWN_Y) as i32),
                          MESSAGE_COUNTDOWN_SIZE * scale, MESSAGE_COUNTDOWN_COLOUR, Anchor::MidTop, true)?;
            } else if state.show_go(player) {
                text.draw(canvas, tc, MESSAGE_GO, (cx, top + (h * MESSAGE_COUNTDOWN_Y) as i32),
                          MESSAGE_GO_SIZE * scale, MESSAGE_GO_COLOUR, Anchor::MidTop, true)?;
            }
        }
        Ok(())
    }
}

/// Which ships should be drawn at all
pub fn visible(ship: &Ship) -> bool {
    !ship.dead
}
