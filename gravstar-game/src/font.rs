//! Drawing text. The font (Liberation Sans, which looks like Arial) is built
//! into the program. Each letter is turned into a texture once per size and
//! then reused.

use std::collections::HashMap;

use ab_glyph::{point, Font, FontRef, PxScale, ScaleFont};
use gravstar_core::constants::TEXT_SHADOW_FRACTION;
use sdl2::rect::Rect;
use sdl2::render::{Canvas, Texture, TextureCreator};
use sdl2::video::{Window, WindowContext};

use crate::textures;

const FONT_BYTES: &[u8] = include_bytes!("../fonts/LiberationSans-Regular.ttf");

/// Which point of the text is placed at the position given
#[derive(Clone, Copy, PartialEq)]
pub enum Anchor {
    TopLeft,
    MidTop,
    MidLeft,
    MidRight,
    Centre,
}

struct Glyph {
    /// None for letters with no outline (a space)
    texture: Option<Texture>,
    /// Where the picture sits, from the pen position and the top of the line
    offset_x: i32,
    offset_y: i32,
    width: u32,
    height: u32,
}

pub struct TextRenderer {
    font: FontRef<'static>,
    glyphs: HashMap<(char, u32), Glyph>,
}

impl TextRenderer {
    pub fn new() -> Result<Self, String> {
        let font = FontRef::try_from_slice(FONT_BYTES).map_err(|e| format!("font: {e}"))?;
        Ok(TextRenderer { font, glyphs: HashMap::new() })
    }

    fn glyph(&mut self, tc: &TextureCreator<WindowContext>, ch: char, px: u32) -> Result<&Glyph, String> {
        if !self.glyphs.contains_key(&(ch, px)) {
            let scale = PxScale::from(px as f32);
            let scaled = self.font.as_scaled(scale);
            let positioned = self.font.glyph_id(ch).with_scale_and_position(scale, point(0.0, scaled.ascent()));
            let glyph = match self.font.outline_glyph(positioned) {
                Some(outline) => {
                    let bounds = outline.px_bounds();
                    let (x0, y0) = (bounds.min.x.floor() as i32, bounds.min.y.floor() as i32);
                    let width = (bounds.max.x.ceil() as i32 - x0).max(1) as u32;
                    let height = (bounds.max.y.ceil() as i32 - y0).max(1) as u32;
                    let mut rgba = vec![255u8; (width * height * 4) as usize];
                    for pixel in rgba.chunks_exact_mut(4) {
                        pixel[3] = 0;
                    }
                    // (x, y) here count from the picture's top-left corner
                    outline.draw(|x, y, coverage| {
                        if x < width && y < height {
                            rgba[((y * width + x) * 4 + 3) as usize] =
                                (coverage.clamp(0.0, 1.0) * 255.0).round() as u8;
                        }
                    });
                    Glyph {
                        texture: Some(textures::from_rgba(tc, width, height, &rgba)?),
                        offset_x: x0,
                        offset_y: y0,
                        width,
                        height,
                    }
                }
                None => Glyph { texture: None, offset_x: 0, offset_y: 0, width: 0, height: 0 },
            };
            self.glyphs.insert((ch, px), glyph);
        }
        Ok(&self.glyphs[&(ch, px)])
    }

    /// The width and height the text takes up at this size
    pub fn measure(&self, text: &str, size: f64) -> (i32, i32) {
        let scaled = self.font.as_scaled(PxScale::from(size.round().max(1.0) as f32));
        let mut width = 0.0f32;
        let mut previous = None;
        for ch in text.chars() {
            let id = self.font.glyph_id(ch);
            if let Some(p) = previous {
                width += scaled.kern(p, id);
            }
            width += scaled.h_advance(id);
            previous = Some(id);
        }
        (width.ceil() as i32, scaled.height().ceil() as i32)
    }

    /// Draw the text with the given point of it at `pos`. Returns the
    /// rectangle it covers. `shadow` adds a dark shadow, which keeps large
    /// text readable over anything.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&mut self, canvas: &mut Canvas<Window>, tc: &TextureCreator<WindowContext>,
                text: &str, pos: (i32, i32), size: f64, colour: [u8; 3], anchor: Anchor,
                shadow: bool) -> Result<Rect, String> {
        let px = size.round().max(1.0) as u32;
        let (width, height) = self.measure(text, px as f64);
        let (left, top) = match anchor {
            Anchor::TopLeft => pos,
            Anchor::MidTop => (pos.0 - width / 2, pos.1),
            Anchor::MidLeft => (pos.0, pos.1 - height / 2),
            Anchor::MidRight => (pos.0 - width, pos.1 - height / 2),
            Anchor::Centre => (pos.0 - width / 2, pos.1 - height / 2),
        };
        let area = Rect::new(left, top, width.max(1) as u32, height.max(1) as u32);
        if shadow {
            let offset = ((px as f64 * TEXT_SHADOW_FRACTION).round() as i32).max(1);
            self.draw_line(canvas, tc, text, (left + offset, top + offset), px, [0, 0, 0])?;
        }
        self.draw_line(canvas, tc, text, (left, top), px, colour)?;
        Ok(area)
    }

    fn draw_line(&mut self, canvas: &mut Canvas<Window>, tc: &TextureCreator<WindowContext>,
                 text: &str, origin: (i32, i32), px: u32, colour: [u8; 3]) -> Result<(), String> {
        // Where each letter goes along the line
        let mut placed: Vec<(char, i32)> = Vec::new();
        {
            let scaled = self.font.as_scaled(PxScale::from(px as f32));
            let mut pen = origin.0 as f32;
            let mut previous = None;
            for ch in text.chars() {
                let id = self.font.glyph_id(ch);
                if let Some(p) = previous {
                    pen += scaled.kern(p, id);
                }
                placed.push((ch, pen.round() as i32));
                pen += scaled.h_advance(id);
                previous = Some(id);
            }
        }
        for (ch, x) in placed {
            self.glyph(tc, ch, px)?; // make sure it exists
            let glyph = self.glyphs.get_mut(&(ch, px)).unwrap();
            let dest = Rect::new(x + glyph.offset_x, origin.1 + glyph.offset_y, glyph.width, glyph.height);
            if let Some(texture) = glyph.texture.as_mut() {
                texture.set_color_mod(colour[0], colour[1], colour[2]);
                canvas.copy(texture, None, dest)?;
            }
        }
        Ok(())
    }
}
