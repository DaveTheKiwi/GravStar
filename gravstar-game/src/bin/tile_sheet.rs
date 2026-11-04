//! Developer tool: draws every tile into two pictures so they can be checked
//! by eye.
//!   tile_sheet.bmp  - the tile art over a dark background
//!   tile_masks.bmp  - the collision masks (white = solid, black = empty)
//! Run with:  cargo run -p gravstar-game --bin tile_sheet

use gravstar_core::constants::{TILE_COLOUR, TILE_SIZE};
use gravstar_core::tile_art;
use gravstar_core::tileset::TILE_DEFS;
use gravstar_game::bmp::Picture;
use std::path::Path;

const COLUMNS: usize = 9;
const ZOOM: usize = 4;
const MARGIN: usize = 6;

/// Flat dark backdrop, so partly transparent edge pixels show as a smooth
/// fade rather than disappearing into a pattern
const BACKGROUND_GREY: u8 = 16;

fn main() -> std::io::Result<()> {
    let cell = TILE_SIZE * ZOOM + MARGIN;
    let rows = (TILE_DEFS.len() + COLUMNS - 1) / COLUMNS;
    let (w, h) = (COLUMNS * cell + MARGIN, rows * cell + MARGIN);
    let mut art_sheet = Picture::new(w, h, [30, 32, 44]);
    let mut mask_sheet = Picture::new(w, h, [30, 32, 44]);

    for (i, def) in TILE_DEFS.iter().enumerate() {
        let art = tile_art::render(&def.shape);
        let ox = MARGIN + (i % COLUMNS) * cell;
        let oy = MARGIN + (i / COLUMNS) * cell;
        let solid: u32 = art.mask.iter().map(|b| b.count_ones()).sum();
        println!("{:2} {:32} {:4} solid pixels", def.id, def.name, solid);

        for y in 0..TILE_SIZE {
            for x in 0..TILE_SIZE {
                let bit = y * TILE_SIZE + x;
                let alpha = art.alpha[bit] as u32;
                let in_mask = art.mask[bit / 8] & (1 << (bit % 8)) != 0;
                for zy in 0..ZOOM {
                    for zx in 0..ZOOM {
                        let (px, py) = (ox + x * ZOOM + zx, oy + y * ZOOM + zy);
                        let check = BACKGROUND_GREY;
                        let mut c = [0u8; 3];
                        for k in 0..3 {
                            c[k] = ((TILE_COLOUR[k] as u32 * alpha
                                + check as u32 * (255 - alpha)) / 255) as u8;
                        }
                        art_sheet.set(px, py, c);
                        mask_sheet.set(px, py, if in_mask { [255, 255, 255] } else { [0, 0, 0] });
                    }
                }
            }
        }
    }

    art_sheet.save_bmp(Path::new("tile_sheet.bmp"))?;
    mask_sheet.save_bmp(Path::new("tile_masks.bmp"))?;
    println!("Wrote tile_sheet.bmp and tile_masks.bmp in {}",
             std::env::current_dir()?.display());
    Ok(())
}
