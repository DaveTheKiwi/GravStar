//! Drawing ships and everything that goes with them: the engine flame, the
//! shield bubble, bullets and explosions.

use gravstar_core::camera::Camera;
use gravstar_core::constants::*;
use gravstar_core::explosion_system::Explosion;
use gravstar_core::projectile_system::Projectile;
use gravstar_core::random::Rng;
use gravstar_core::ship::Ship;
use sdl2::rect::Rect;
use sdl2::render::{Canvas, Texture, TextureCreator};
use sdl2::video::{Window, WindowContext};

use crate::assets::{scale_image, Image};
use crate::textures;

/// Shapes shared by every effect
pub struct EffectTextures {
    disc: Texture,
    ring: Texture,
    pixel: Texture,
}

const EFFECT_TEXTURE_SIZE: u32 = 128;

impl EffectTextures {
    pub fn new(tc: &TextureCreator<WindowContext>) -> Result<Self, String> {
        Ok(EffectTextures {
            disc: textures::disc(tc, EFFECT_TEXTURE_SIZE)?,
            ring: textures::ring(tc, EFFECT_TEXTURE_SIZE, 3.0)?,
            pixel: textures::pixel(tc)?,
        })
    }

    /// A tinted, see-through disc of the given radius (screen pixels)
    pub fn draw_disc(&mut self, canvas: &mut Canvas<Window>, cx: i32, cy: i32, radius: i32,
                 colour: (u8, u8, u8), alpha: u8) -> Result<(), String> {
        self.disc.set_color_mod(colour.0, colour.1, colour.2);
        self.disc.set_alpha_mod(alpha);
        canvas.copy(&self.disc, None, Rect::new(cx - radius, cy - radius, radius as u32 * 2, radius as u32 * 2))
    }

    fn draw_ring(&mut self, canvas: &mut Canvas<Window>, cx: i32, cy: i32, radius: i32,
                 colour: (u8, u8, u8), alpha: u8) -> Result<(), String> {
        self.ring.set_color_mod(colour.0, colour.1, colour.2);
        self.ring.set_alpha_mod(alpha);
        canvas.copy(&self.ring, None, Rect::new(cx - radius, cy - radius, radius as u32 * 2, radius as u32 * 2))
    }

    /// A straight line of the given thickness, as a stretched rotated rectangle
    fn draw_line(&mut self, canvas: &mut Canvas<Window>, from: (i32, i32), to: (i32, i32),
                 thickness: i32, colour: (u8, u8, u8)) -> Result<(), String> {
        let (dx, dy) = ((to.0 - from.0) as f64, (to.1 - from.1) as f64);
        let length = dx.hypot(dy).round().max(1.0) as u32;
        let (mid_x, mid_y) = ((from.0 + to.0) as f64 / 2.0, (from.1 + to.1) as f64 / 2.0);
        self.pixel.set_color_mod(colour.0, colour.1, colour.2);
        self.pixel.set_alpha_mod(255);
        let dest = Rect::new((mid_x - length as f64 / 2.0).round() as i32,
                             (mid_y - thickness as f64 / 2.0).round() as i32,
                             length, thickness.max(1) as u32);
        canvas.copy_ex(&self.pixel, None, dest, dy.atan2(dx).to_degrees(), None, false, false)
    }
}

/// One ship's picture
pub struct ShipView {
    sprite: Texture,
    pub width: u32,
    pub height: u32,
}

impl ShipView {
    /// `sprite` is scaled once to the ship's size in the world
    pub fn new(tc: &TextureCreator<WindowContext>, sprite: &Image, ship: &Ship) -> Result<Self, String> {
        let (width, height) = (ship.physics.width.round() as u32, ship.physics.height.round() as u32);
        let scaled = scale_image(sprite, width as usize, height as usize);
        Ok(ShipView { sprite: textures::from_rgba(tc, width, height, &scaled.rgba)?, width, height })
    }

    /// The flame, the ship and its shield, in that order. `alpha` below 255
    /// makes the ship see-through (spawn protection).
    pub fn draw(&mut self, canvas: &mut Canvas<Window>, fx: &mut EffectTextures, rng: &mut Rng,
                cam: &Camera, ship: &Ship, alpha: u8) -> Result<(), String> {
        let zoom = cam.zoom;
        let (cx, cy) = cam.world_to_screen_int(ship.x, ship.y);

        // Engine flame, behind the ship
        let thrust = ship.input.thrust.clamp(0.0, 1.0);
        if thrust > 0.0 {
            let (sx, sy) = cam.world_to_screen(ship.x, ship.y);
            let (fx_dir, fy_dir) = ship.facing_vector();
            let base = ship.physics.height * zoom * FLAME_SIZE_FACTOR;
            let (flame_x, flame_y) = (sx - fx_dir * base, sy - fy_dir * base);
            let flicker = 0.7 + rng.unit() * 0.6;
            let power = 0.4 + 0.6 * thrust;
            let size = ((base * flicker * 0.8 * power) as i32).max(1);
            fx.draw_disc(canvas, flame_x as i32, flame_y as i32, size, (255, 100, 0), 180)?;
            fx.draw_disc(canvas, flame_x as i32, flame_y as i32, (size / 2).max(1), (255, 220, 50), 220)?;
        }

        // The ship, turned by its heading
        let (w, h) = ((self.width as f64 * zoom).round().max(1.0) as u32,
                      (self.height as f64 * zoom).round().max(1.0) as u32);
        self.sprite.set_alpha_mod(alpha);
        canvas.copy_ex(&self.sprite, None,
                       Rect::new(cx - w as i32 / 2, cy - h as i32 / 2, w, h),
                       ship.angle_degrees(), None, false, false)?;

        // Shield bubble
        let fraction = ship.health.shield_fraction();
        if fraction > 0.0 {
            let radius = ((ship.physics.width * SHIELD_RADIUS_FACTOR * zoom) as i32).max(1);
            let a = (fraction * SHIELD_MAX_ALPHA) as u8;
            fx.draw_disc(canvas, cx, cy, radius, (100, 180, 255), a)?;
            fx.draw_ring(canvas, cx, cy, radius, (150, 210, 255), a / 2)?;
        }
        Ok(())
    }
}

/// Bullets: a bright head with a trail that fades and shortens as the
/// bullet slows down
pub fn draw_projectiles(canvas: &mut Canvas<Window>, fx: &mut EffectTextures, cam: &Camera,
                        projectiles: &[Projectile]) -> Result<(), String> {
    const CORE: (f64, f64, f64) = (255.0, 250.0, 210.0);
    const GLOW: (f64, f64, f64) = (255.0, 170.0, 60.0);
    const DIM: (f64, f64, f64) = (120.0, 60.0, 20.0);
    let zoom = cam.zoom;
    for p in projectiles {
        if !cam.is_visible(p.x, p.y, BULLET_TRAIL_MAX + 4.0) {
            continue;
        }
        let (cx, cy) = cam.world_to_screen_int(p.x, p.y);
        let speed = p.speed();
        let fraction = (speed / p.ptype.muzzle_speed).min(1.0);
        let mix = |a: f64, b: f64| (a + (b - a) * fraction) as u8;
        if speed > 1e-6 {
            let length = (speed * BULLET_TRAIL_SECONDS).clamp(BULLET_TRAIL_MIN, BULLET_TRAIL_MAX);
            let tail = ((cx as f64 - p.vel_x / speed * length * zoom).round() as i32,
                        (cy as f64 - p.vel_y / speed * length * zoom).round() as i32);
            let colour = (mix(DIM.0, GLOW.0), mix(DIM.1, GLOW.1), mix(DIM.2, GLOW.2));
            fx.draw_line(canvas, tail, (cx, cy), ((BULLET_TRAIL_WIDTH * zoom).round() as i32).max(1), colour)?;
        }
        let head = ((BULLET_HEAD_RADIUS * zoom).round() as i32).max(1);
        fx.draw_disc(canvas, cx, cy, head, (CORE.0 as u8, CORE.1 as u8, CORE.2 as u8), 255)?;
    }
    Ok(())
}

/// Explosions: an orange circle that grows and shrinks, with a hotter core
pub fn draw_explosions(canvas: &mut Canvas<Window>, fx: &mut EffectTextures, cam: &Camera,
                       explosions: &[Explosion]) -> Result<(), String> {
    for e in explosions {
        let radius = e.radius() * cam.zoom;
        if radius < 0.5 || !cam.is_visible(e.x, e.y, e.max_radius + 2.0) {
            continue;
        }
        let (cx, cy) = cam.world_to_screen_int(e.x, e.y);
        let r = (radius.round() as i32).max(1);
        fx.draw_disc(canvas, cx, cy, r, (255, 135, 25), 215)?;
        let core = (r as f64 * 0.55).round() as i32;
        if core >= 1 && r >= 3 {
            fx.draw_disc(canvas, cx, cy, core, (255, 225, 140), 235)?;
        }
    }
    Ok(())
}
