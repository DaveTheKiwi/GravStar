//! Drawing the debris: pieces of terrain still hanging where they were cut
//! free, and the pixels falling and fading. This is built to be cheap: every
//! pixel is a small filled square (no textures, no per-pixel calls), and the
//! pixels are sorted into a few fade steps so each step reaches the screen
//! as one batch.

use gravstar_core::camera::Camera;
use gravstar_core::constants::{DEBRIS_ALPHA_STEPS, TILE_COLOUR};
use gravstar_core::debris_system::DebrisSystem;
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::{BlendMode, Canvas};
use sdl2::video::Window;

pub struct DebrisView {
    /// The squares to draw, one list per fade step (0 = fully solid)
    batches: Vec<Vec<Rect>>,
}

impl DebrisView {
    pub fn new() -> Self {
        DebrisView { batches: vec![Vec::new(); DEBRIS_ALPHA_STEPS] }
    }

    /// Draw what the camera sees. Every terrain pixel is the same colour for
    /// now (TILE_COLOUR); the pixels do carry their tile kind, so tiles with
    /// their own colours could be drawn here one day.
    pub fn draw(&mut self, canvas: &mut Canvas<Window>, cam: &Camera, debris: &DebrisSystem)
                -> Result<(), String> {
        if debris.islands.is_empty() && debris.particles.is_empty() {
            return Ok(());
        }
        let (left, top, right, bottom) = cam.visible_world_rect();
        let (left, top, right, bottom) = (left - 1.0, top - 1.0, right + 1.0, bottom + 1.0);
        let half_w = cam.viewport_width as f64 / 2.0;
        let half_h = cam.viewport_height as f64 / 2.0;
        let screen_x = |wx: i32| ((wx as f64 - cam.x) * cam.zoom + cam.viewport_x as f64 + half_w
                                  + 0.5).floor() as i32;
        let screen_y = |wy: i32| ((wy as f64 - cam.y) * cam.zoom + cam.viewport_y as f64 + half_h
                                  + 0.5).floor() as i32;
        // One world pixel is this many screen pixels (rounded up, so a piece
        // of terrain never shows gaps when zoomed in)
        let size = cam.zoom.ceil().max(1.0) as u32;

        for batch in self.batches.iter_mut() {
            batch.clear();
        }
        let inside = |x: f64, y: f64| x >= left && x <= right && y >= top && y <= bottom;

        for island in &debris.islands {
            for p in island.pending() {
                if inside(p.x as f64, p.y as f64) {
                    self.batches[0].push(Rect::new(screen_x(p.x), screen_y(p.y), size, size));
                }
            }
        }
        let steps = DEBRIS_ALPHA_STEPS as f32;
        for p in &debris.particles {
            let (x, y) = (p.x.floor(), p.y.floor());
            if !inside(x as f64, y as f64) {
                continue;
            }
            let step = (((1.0 - p.alpha()) * steps) as usize).min(DEBRIS_ALPHA_STEPS - 1);
            self.batches[step].push(Rect::new(screen_x(x as i32), screen_y(y as i32), size, size));
        }

        canvas.set_blend_mode(BlendMode::Blend);
        for (step, batch) in self.batches.iter().enumerate() {
            if batch.is_empty() {
                continue;
            }
            let alpha = (255 * (DEBRIS_ALPHA_STEPS - step) / DEBRIS_ALPHA_STEPS) as u8;
            canvas.set_draw_color(Color::RGBA(TILE_COLOUR[0], TILE_COLOUR[1], TILE_COLOUR[2], alpha));
            canvas.fill_rects(batch)?;
        }
        canvas.set_blend_mode(BlendMode::None);
        Ok(())
    }
}

impl Default for DebrisView {
    fn default() -> Self {
        Self::new()
    }
}
