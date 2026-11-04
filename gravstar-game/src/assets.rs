//! Finding and loading the game's files (levels and pictures).

use std::fs::File;
use std::path::{Path, PathBuf};

/// A picture as RGBA bytes, 4 per pixel, row by row
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

/// The `assets` folder: looked for in the current folder and the program's
/// folder, and in every folder above them (so it is found whether the game
/// is started from the project, from a crate's folder or from `target`),
/// also under `reference-python`, where the Python project's copy lives
pub fn find_assets_dir() -> Option<PathBuf> {
    let mut starts = Vec::new();
    if let Ok(dir) = std::env::current_dir() {
        starts.push(dir);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            starts.push(dir.to_path_buf());
        }
    }
    for start in starts {
        for folder in start.ancestors() {
            for candidate in ["assets", "reference-python/assets"] {
                let path = folder.join(candidate);
                if path.join("levels").is_dir() {
                    return Some(path);
                }
            }
        }
    }
    None
}

/// Load a PNG as RGBA
pub fn load_png(path: &Path) -> Result<Image, String> {
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| format!("{}: {e}", path.display()))?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).map_err(|e| format!("{}: {e}", path.display()))?;
    let (width, height) = (info.width as usize, info.height as usize);
    let data = &buffer[..info.buffer_size()];

    let rgba = match info.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => data.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
        png::ColorType::Indexed => return Err(format!("{}: unexpanded palette", path.display())),
    };
    Ok(Image { width, height, rgba })
}

/// Resize a picture; each new pixel is the average of the part of the old
/// picture it covers (colours weighted by how solid each pixel is, so a
/// soft edge does not darken)
pub fn scale_image(image: &Image, new_width: usize, new_height: usize) -> Image {
    let mut out = vec![0u8; new_width * new_height * 4];
    let sx = image.width as f64 / new_width as f64;
    let sy = image.height as f64 / new_height as f64;
    for y in 0..new_height {
        let (y0, y1) = (y as f64 * sy, (y + 1) as f64 * sy);
        for x in 0..new_width {
            let (x0, x1) = (x as f64 * sx, (x + 1) as f64 * sx);
            let (mut r, mut g, mut b, mut a, mut weight) = (0.0, 0.0, 0.0, 0.0, 0.0);
            for py in (y0.floor() as usize)..(y1.ceil() as usize).min(image.height) {
                let wy = (y1.min((py + 1) as f64) - y0.max(py as f64)).max(0.0);
                for px in (x0.floor() as usize)..(x1.ceil() as usize).min(image.width) {
                    let wx = (x1.min((px + 1) as f64) - x0.max(px as f64)).max(0.0);
                    let p = &image.rgba[(py * image.width + px) * 4..][..4];
                    let w = wx * wy;
                    let alpha = p[3] as f64 / 255.0;
                    r += p[0] as f64 * alpha * w;
                    g += p[1] as f64 * alpha * w;
                    b += p[2] as f64 * alpha * w;
                    a += alpha * w;
                    weight += w;
                }
            }
            let o = (y * new_width + x) * 4;
            if a > 0.0 {
                out[o] = (r / a).round() as u8;
                out[o + 1] = (g / a).round() as u8;
                out[o + 2] = (b / a).round() as u8;
                out[o + 3] = (a / weight * 255.0).round() as u8;
            }
        }
    }
    Image { width: new_width, height: new_height, rgba: out }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaling_keeps_colour_at_soft_edges() {
        // left half opaque red, right half empty: shrinking to one pixel
        // gives half-transparent red, not dark red
        let img = Image {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 0, 0, 0],
        };
        let small = scale_image(&img, 1, 1);
        assert_eq!(&small.rgba[..], &[255, 0, 0, 128]);
    }

    #[test]
    fn same_size_is_unchanged() {
        let img = Image { width: 2, height: 1, rgba: vec![10, 20, 30, 255, 40, 50, 60, 128] };
        let same = scale_image(&img, 2, 1);
        assert_eq!(same.rgba, img.rgba);
    }
}
