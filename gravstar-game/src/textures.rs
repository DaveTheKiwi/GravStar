//! Making SDL textures from pictures and from shapes.

use sdl2::pixels::PixelFormatEnum;
use sdl2::render::{BlendMode, Texture, TextureCreator};
use sdl2::video::WindowContext;

/// A texture from RGBA bytes (4 per pixel, row by row), drawn with
/// transparency
pub fn from_rgba(tc: &TextureCreator<WindowContext>, width: u32, height: u32,
                 rgba: &[u8]) -> Result<Texture, String> {
    let mut texture = tc
        .create_texture_static(PixelFormatEnum::RGBA32, width, height)
        .map_err(|e| e.to_string())?;
    texture.update(None, rgba, width as usize * 4).map_err(|e| e.to_string())?;
    texture.set_blend_mode(BlendMode::Blend);
    Ok(texture)
}

/// A white filled circle with a smooth edge, `diameter` pixels across.
/// Tinted and made see-through when drawn.
pub fn disc(tc: &TextureCreator<WindowContext>, diameter: u32) -> Result<Texture, String> {
    ring_or_disc(tc, diameter, None)
}

/// A white circle outline, `thickness` pixels thick, `diameter` across
pub fn ring(tc: &TextureCreator<WindowContext>, diameter: u32, thickness: f64) -> Result<Texture, String> {
    ring_or_disc(tc, diameter, Some(thickness))
}

fn ring_or_disc(tc: &TextureCreator<WindowContext>, diameter: u32,
                thickness: Option<f64>) -> Result<Texture, String> {
    let r = diameter as f64 / 2.0;
    let mut rgba = vec![255u8; (diameter * diameter * 4) as usize];
    for y in 0..diameter {
        for x in 0..diameter {
            let d = ((x as f64 + 0.5 - r).powi(2) + (y as f64 + 0.5 - r).powi(2)).sqrt();
            // 1 inside, fading to 0 across the last pixel of the edge
            let outer = (r - d).clamp(0.0, 1.0);
            let alpha = match thickness {
                None => outer,
                Some(t) => outer.min((d - (r - t) + 1.0).clamp(0.0, 1.0)),
            };
            rgba[((y * diameter + x) * 4 + 3) as usize] = (alpha * 255.0).round() as u8;
        }
    }
    from_rgba(tc, diameter, diameter, &rgba)
}

/// A single white pixel, to be stretched into rectangles of any colour
pub fn pixel(tc: &TextureCreator<WindowContext>) -> Result<Texture, String> {
    from_rgba(tc, 1, 1, &[255, 255, 255, 255])
}
