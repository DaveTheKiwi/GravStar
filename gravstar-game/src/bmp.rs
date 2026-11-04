//! A tiny picture type that can be saved as a 24-bit BMP (opens in any image
//! viewer), for screenshots and the developer tools.

use std::fs::File;
use std::io::Write;
use std::path::Path;

pub struct Picture {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<[u8; 3]>,
}

impl Picture {
    pub fn new(width: usize, height: usize, background: [u8; 3]) -> Self {
        Picture { width, height, rgb: vec![background; width * height] }
    }

    /// From packed RGB bytes (3 per pixel, row by row)
    pub fn from_rgb24(width: usize, height: usize, bytes: &[u8]) -> Self {
        Picture {
            width,
            height,
            rgb: bytes.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect(),
        }
    }

    pub fn set(&mut self, x: usize, y: usize, colour: [u8; 3]) {
        self.rgb[y * self.width + x] = colour;
    }

    pub fn save_bmp(&self, path: &Path) -> std::io::Result<()> {
        let row_bytes = (self.width * 3 + 3) / 4 * 4;
        let image_size = row_bytes * self.height;
        let mut f = File::create(path)?;
        f.write_all(b"BM")?;
        f.write_all(&((54 + image_size) as u32).to_le_bytes())?;
        f.write_all(&[0; 4])?;
        f.write_all(&54u32.to_le_bytes())?;
        f.write_all(&40u32.to_le_bytes())?;
        f.write_all(&(self.width as i32).to_le_bytes())?;
        f.write_all(&(self.height as i32).to_le_bytes())?;
        f.write_all(&1u16.to_le_bytes())?;
        f.write_all(&24u16.to_le_bytes())?;
        f.write_all(&[0; 4])?;
        f.write_all(&(image_size as u32).to_le_bytes())?;
        f.write_all(&[0; 16])?;
        for y in (0..self.height).rev() {
            let mut row = Vec::with_capacity(row_bytes);
            for x in 0..self.width {
                let c = self.rgb[y * self.width + x];
                row.extend_from_slice(&[c[2], c[1], c[0]]);
            }
            row.resize(row_bytes, 0);
            f.write_all(&row)?;
        }
        Ok(())
    }
}
