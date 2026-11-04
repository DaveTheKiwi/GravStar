//! A camera: which part of the world a viewport (a rectangle of the screen)
//! shows, and how it zooms. Pure maths - the game crate does the drawing.

use crate::constants::*;

/// How the zoom follows the ship's speed
#[derive(Clone, Copy, Debug)]
pub struct ZoomSettings {
    pub min_zoom: f64,
    pub max_zoom: f64,
    pub zoom_out_time: f64,
    pub zoom_in_time: f64,
    /// Slower than this the zoom stays at max_zoom
    pub dead_zone: f64,
    /// The speed at which the zoom reaches min_zoom
    pub full_speed: f64,
    /// Minimum closing rate, zoom units per second
    pub settle_rate: f64,
}

impl ZoomSettings {
    /// The usual settings for a screen `screen_height` pixels high, whose
    /// viewports can show a level of `world_width` x `world_height` pixels
    /// without showing beyond it
    pub fn for_screen(screen_height: f64, camera: &Camera, world_width: f64,
                      world_height: f64) -> Self {
        let scale = screen_height / REFERENCE_SCREEN_HEIGHT;
        // Lowest zoom allowed: the floor, or higher if the level is small
        // enough that the floor would show beyond its edge
        let level_min = camera.min_zoom_for_world(world_width, world_height);
        let max_zoom = MAX_ZOOM * scale;
        let min_zoom = max_zoom.min((MIN_ZOOM_FLOOR * scale).max(level_min));
        ZoomSettings {
            min_zoom,
            max_zoom,
            zoom_out_time: ZOOM_OUT_TIME,
            zoom_in_time: ZOOM_IN_TIME,
            dead_zone: ZOOM_DEAD_ZONE_SPEED,
            full_speed: ZOOM_FULL_SPEED,
            settle_rate: ZOOM_SETTLE_RATE * scale,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Camera {
    /// The world position the view is centred on
    pub x: f64,
    pub y: f64,
    /// 1.0 = one world pixel per screen pixel
    pub zoom: f64,
    /// The viewport: a rectangle of the screen, in screen pixels
    pub viewport_x: i32,
    pub viewport_y: i32,
    pub viewport_width: i32,
    pub viewport_height: i32,
}

impl Camera {
    pub fn new(viewport_x: i32, viewport_y: i32, viewport_width: i32, viewport_height: i32) -> Self {
        Camera { x: 0.0, y: 0.0, zoom: 1.0, viewport_x, viewport_y, viewport_width, viewport_height }
    }

    // -------------------------
    // Transforms
    // -------------------------
    pub fn world_to_screen(&self, world_x: f64, world_y: f64) -> (f64, f64) {
        ((world_x - self.x) * self.zoom + self.viewport_x as f64 + self.viewport_width as f64 / 2.0,
         (world_y - self.y) * self.zoom + self.viewport_y as f64 + self.viewport_height as f64 / 2.0)
    }

    /// world_to_screen rounded to whole pixels. Everything that must line up
    /// with the terrain (ships, effects) uses this so they all round the
    /// same way.
    pub fn world_to_screen_int(&self, world_x: f64, world_y: f64) -> (i32, i32) {
        let (sx, sy) = self.world_to_screen(world_x, world_y);
        ((sx + 0.5).floor() as i32, (sy + 0.5).floor() as i32)
    }

    /// The exact inverse of world_to_screen
    pub fn screen_to_world(&self, screen_x: f64, screen_y: f64) -> (f64, f64) {
        ((screen_x - self.viewport_x as f64 - self.viewport_width as f64 / 2.0) / self.zoom + self.x,
         (screen_y - self.viewport_y as f64 - self.viewport_height as f64 / 2.0) / self.zoom + self.y)
    }

    // -------------------------
    // Following the ship
    // -------------------------
    /// Centre the camera on a world position. This is how the camera tracks
    /// a ship: locked on, no lag.
    pub fn snap(&mut self, world_x: f64, world_y: f64) {
        self.x = world_x;
        self.y = world_y;
    }

    /// Stop the camera showing beyond the world. Uses the CURRENT zoom, so
    /// call it after the zoom has been updated for the frame. If the world is
    /// smaller than the view on an axis, the camera centres on the world on
    /// that axis.
    pub fn clamp(&mut self, left: f64, top: f64, right: f64, bottom: f64) {
        let half_w = self.viewport_width as f64 / 2.0 / self.zoom;
        let half_h = self.viewport_height as f64 / 2.0 / self.zoom;
        if right - left <= half_w * 2.0 {
            self.x = (left + right) / 2.0;
        } else {
            self.x = self.x.min(right - half_w).max(left + half_w);
        }
        if bottom - top <= half_h * 2.0 {
            self.y = (top + bottom) / 2.0;
        } else {
            self.y = self.y.min(bottom - half_h).max(top + half_h);
        }
    }

    /// The lowest zoom at which this viewport still sees no more than the
    /// world on BOTH axes
    pub fn min_zoom_for_world(&self, world_width: f64, world_height: f64) -> f64 {
        (self.viewport_width as f64 / world_width).max(self.viewport_height as f64 / world_height)
    }

    /// The world rectangle in view: (left, top, right, bottom). Use it to
    /// choose what to draw.
    pub fn visible_world_rect(&self) -> (f64, f64, f64, f64) {
        let half_w = self.viewport_width as f64 / 2.0 / self.zoom;
        let half_h = self.viewport_height as f64 / 2.0 / self.zoom;
        (self.x - half_w, self.y - half_h, self.x + half_w, self.y + half_h)
    }

    /// Is this world point (give or take `margin`) in view?
    pub fn is_visible(&self, world_x: f64, world_y: f64, margin: f64) -> bool {
        let (left, top, right, bottom) = self.visible_world_rect();
        left - margin <= world_x && world_x <= right + margin
            && top - margin <= world_y && world_y <= bottom + margin
    }

    // -------------------------
    // Zoom
    // -------------------------
    /// Zoom out as speed increases and back in as it falls, easing towards
    /// the target instead of jumping to it. The easing is independent of the
    /// frame rate. An exponential ease never quite arrives, so `settle_rate`
    /// is a minimum closing speed that lets the last sliver finish; that
    /// matters at full zoom, where nothing needs scaling.
    pub fn update_zoom_for_speed(&mut self, speed: f64, zoom: &ZoomSettings, dt: f64) {
        let fraction = if speed <= zoom.dead_zone {
            0.0
        } else {
            ((speed - zoom.dead_zone) / (zoom.full_speed - zoom.dead_zone).max(1e-6)).min(1.0)
        };
        let target = zoom.max_zoom - (zoom.max_zoom - zoom.min_zoom) * fraction;
        let diff = target - self.zoom;
        let time_constant = if diff < 0.0 { zoom.zoom_out_time } else { zoom.zoom_in_time };
        let mut step = diff * (1.0 - (-dt / time_constant.max(1e-6)).exp());
        let min_step = zoom.settle_rate * dt;
        if step.abs() < min_step {
            step = min_step.copysign(diff);
        }
        if step.abs() >= diff.abs() {
            self.zoom = target; // arrived exactly
        } else {
            self.zoom += step;
        }
        self.zoom = self.zoom.min(zoom.max_zoom).max(zoom.min_zoom);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cam() -> Camera {
        let mut c = Camera::new(100, 50, 800, 600);
        c.snap(1000.0, 500.0);
        c
    }

    #[test]
    fn world_and_screen_are_exact_inverses() {
        let mut c = cam();
        c.zoom = 0.73;
        let (sx, sy) = c.world_to_screen(1234.5, 321.25);
        let (wx, wy) = c.screen_to_world(sx, sy);
        assert!((wx - 1234.5).abs() < 1e-9 && (wy - 321.25).abs() < 1e-9);
        // the camera's own centre is the middle of the viewport
        assert_eq!(c.world_to_screen(1000.0, 500.0), (500.0, 350.0));
    }

    #[test]
    fn rounding_goes_up_at_a_half() {
        let c = Camera::new(0, 0, 1, 1); // centre at (0.5, 0.5) on screen
        assert_eq!(c.world_to_screen_int(0.0, 0.0), (1, 1));
        assert_eq!(c.world_to_screen_int(-1.0, -1.0), (0, 0));
    }

    #[test]
    fn clamp_keeps_the_view_inside_the_world() {
        let mut c = cam();
        c.snap(10.0, 10.0);
        c.clamp(0.0, 0.0, 4000.0, 3000.0);
        assert_eq!((c.x, c.y), (400.0, 300.0));
        c.snap(9999.0, 9999.0);
        c.clamp(0.0, 0.0, 4000.0, 3000.0);
        assert_eq!((c.x, c.y), (3600.0, 2700.0));
        // a world smaller than the view is centred
        c.clamp(0.0, 0.0, 500.0, 400.0);
        assert_eq!((c.x, c.y), (250.0, 200.0));
        // zooming out shows more, so the clamp holds further in
        let mut c = cam();
        c.zoom = 0.5;
        c.snap(0.0, 0.0);
        c.clamp(0.0, 0.0, 4000.0, 3000.0);
        assert_eq!(c.x, 800.0);
    }

    #[test]
    fn visibility() {
        let c = cam();
        assert!(c.is_visible(1000.0, 500.0, 0.0));
        assert!(!c.is_visible(1500.0, 500.0, 0.0));
        assert!(c.is_visible(1405.0, 500.0, 10.0));
        let (l, t, r, b) = c.visible_world_rect();
        assert_eq!((l, t, r, b), (600.0, 200.0, 1400.0, 800.0));
    }

    fn settings(c: &Camera) -> ZoomSettings {
        ZoomSettings::for_screen(1080.0, c, 6400.0, 4800.0)
    }

    #[test]
    fn zoom_stays_put_below_the_dead_zone_and_eases_out_when_fast() {
        let mut c = cam();
        let z = settings(&c);
        for _ in 0..120 {
            c.update_zoom_for_speed(50.0, &z, 1.0 / 60.0);
        }
        assert_eq!(c.zoom, 1.0);
        for _ in 0..600 {
            c.update_zoom_for_speed(600.0, &z, 1.0 / 60.0);
        }
        assert!((c.zoom - z.min_zoom).abs() < 0.01, "{}", c.zoom);
        for _ in 0..1200 {
            c.update_zoom_for_speed(0.0, &z, 1.0 / 60.0);
        }
        assert_eq!(c.zoom, 1.0); // the last sliver does finish
    }

    #[test]
    fn zoom_easing_is_independent_of_frame_rate() {
        let z = settings(&cam());
        let after = |fps: f64| {
            let mut c = cam();
            for _ in 0..(fps as usize) {
                c.update_zoom_for_speed(400.0, &z, 1.0 / fps);
            }
            c.zoom
        };
        assert!((after(30.0) - after(240.0)).abs() < 0.01);
    }

    #[test]
    fn a_small_level_limits_how_far_out_you_can_zoom() {
        let c = Camera::new(0, 0, 960, 1080);
        let z = ZoomSettings::for_screen(1080.0, &c, 1920.0, 1200.0);
        assert!(z.min_zoom > 0.6); // the level is barely bigger than the view
        assert!(z.min_zoom <= z.max_zoom);
    }

    #[test]
    fn zoom_scales_with_screen_height() {
        let c = Camera::new(0, 0, 960, 540);
        let z = ZoomSettings::for_screen(540.0, &c, 6400.0, 4800.0);
        assert!((z.max_zoom - 0.5).abs() < 1e-9);
    }
}
