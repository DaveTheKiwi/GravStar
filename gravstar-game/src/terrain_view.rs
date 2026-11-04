//! Drawing the terrain. Each kind of tile has one texture, shared by every
//! tile of that kind. A damaged tile gets its own, rebuilt from its mask
//! whenever the damage changes. The GPU scales the tiles for the zoom.

use std::collections::HashMap;

use gravstar_core::camera::Camera;
use gravstar_core::constants::{TILE_COLOUR, TILE_SIZE};
use gravstar_core::level::Level;
use gravstar_core::tile_art::TileMask;
use gravstar_core::tile_registry::{TileDefinition, TileRegistry};
use sdl2::rect::Rect;
use sdl2::render::{Canvas, Texture, TextureCreator};
use sdl2::video::{Window, WindowContext};

use crate::textures;

pub struct TerrainView {
    /// One texture per undamaged tile kind
    kinds: HashMap<u16, Texture>,
    /// A damaged tile's own picture: (tile kind, damage version, texture)
    damaged: HashMap<(i32, i32), (u16, u32, Texture)>,
    /// Per tile kind: the faint edge pixels that only mean something while a
    /// solid pixel is beside them
    fringe: HashMap<u16, Vec<(usize, usize)>>,
    /// Scale smoothly (blended) or sharply (blocky) when zoomed
    smooth: bool,
}

fn apply_scale_mode(texture: &Texture, smooth: bool) {
    use sdl2::sys::{SDL_ScaleMode, SDL_SetTextureScaleMode};
    let mode = if smooth { SDL_ScaleMode::SDL_ScaleModeLinear } else { SDL_ScaleMode::SDL_ScaleModeNearest };
    // SAFETY: the texture is valid for as long as we hold it
    unsafe { SDL_SetTextureScaleMode(texture.raw(), mode) };
}

fn bit(mask: &TileMask, x: usize, y: usize) -> bool {
    let b = y * TILE_SIZE + x;
    mask[b >> 3] & (1 << (b & 7)) != 0
}

fn has_solid_neighbour(mask: &TileMask, x: usize, y: usize) -> bool {
    for ny in y.saturating_sub(1)..(y + 2).min(TILE_SIZE) {
        for nx in x.saturating_sub(1)..(x + 2).min(TILE_SIZE) {
            if (nx, ny) != (x, y) && bit(mask, nx, ny) {
                return true;
            }
        }
    }
    false
}

/// RGBA for a tile picture with the given alpha values
fn rgba_from_alpha(alpha: &[u8]) -> Vec<u8> {
    alpha.iter().flat_map(|a| [TILE_COLOUR[0], TILE_COLOUR[1], TILE_COLOUR[2], *a]).collect()
}

impl TerrainView {
    pub fn new(tc: &TextureCreator<WindowContext>, registry: &TileRegistry,
               tile_ids: impl Iterator<Item = u16>) -> Result<Self, String> {
        let mut kinds = HashMap::new();
        let mut fringe = HashMap::new();
        for id in tile_ids {
            let Some(def) = registry.get(id) else { continue };
            if def.is_empty {
                continue;
            }
            kinds.insert(id, textures::from_rgba(tc, TILE_SIZE as u32, TILE_SIZE as u32,
                                                 &rgba_from_alpha(&def.alpha))?);
            fringe.insert(id, Self::find_fringe(def));
        }
        Ok(TerrainView { kinds, damaged: HashMap::new(), fringe, smooth: true })
    }

    /// Choose smooth (blended) or sharp scaling for every tile picture
    pub fn set_smooth(&mut self, smooth: bool) {
        self.smooth = smooth;
        for texture in self.kinds.values() {
            apply_scale_mode(texture, smooth);
        }
        for (_, _, texture) in self.damaged.values() {
            apply_scale_mode(texture, smooth);
        }
    }

    /// Forget every damaged tile's picture (a new level is about to be drawn)
    pub fn forget_damage(&mut self) {
        for (_, (_, _, texture)) in self.damaged.drain() {
            // SAFETY: the textures are removed from use here
            unsafe { texture.destroy() };
        }
    }

    /// The soft-edged pixels beside solid ones: visible, but not solid
    fn find_fringe(def: &TileDefinition) -> Vec<(usize, usize)> {
        let mut found = Vec::new();
        for y in 0..TILE_SIZE {
            for x in 0..TILE_SIZE {
                if def.alpha[y * TILE_SIZE + x] > 0 && !bit(&def.mask, x, y)
                    && has_solid_neighbour(&def.mask, x, y)
                {
                    found.push((x, y));
                }
            }
        }
        found
    }

    /// A damaged tile's picture: the pixels that were solid and are not now
    /// are cleared, and so are soft edge pixels left hanging with no solid
    /// pixel beside them
    fn damaged_picture(&self, def: &TileDefinition, current: &TileMask) -> Vec<u8> {
        let mut alpha = def.alpha.clone();
        for y in 0..TILE_SIZE {
            for x in 0..TILE_SIZE {
                if bit(&def.mask, x, y) && !bit(current, x, y) {
                    alpha[y * TILE_SIZE + x] = 0;
                }
            }
        }
        if let Some(fringe) = self.fringe.get(&def.id) {
            for &(x, y) in fringe {
                if !has_solid_neighbour(current, x, y) {
                    alpha[y * TILE_SIZE + x] = 0;
                }
            }
        }
        rgba_from_alpha(&alpha)
    }

    /// Draw the part of the level the camera sees, clipped to its viewport
    pub fn draw(&mut self, canvas: &mut Canvas<Window>, tc: &TextureCreator<WindowContext>,
                cam: &Camera, level: &Level, registry: &TileRegistry) -> Result<(), String> {
        let ts = TILE_SIZE as f64;
        let (left, top, right, bottom) = cam.visible_world_rect();
        let first_col = ((left / ts).floor() as i32).max(0);
        let last_col = ((right / ts).floor() as i32).min(level.width as i32 - 1);
        let first_row = ((top / ts).floor() as i32).max(0);
        let last_row = ((bottom / ts).floor() as i32).min(level.height as i32 - 1);
        if last_col < first_col || last_row < first_row {
            return Ok(());
        }

        // Screen position of every tile edge. Neighbouring tiles share an
        // edge exactly, so there are no seams at any zoom.
        let edge_x: Vec<i32> = (first_col..=last_col + 1).map(|c| {
            ((c as f64 * ts - cam.x) * cam.zoom + cam.viewport_x as f64
                + cam.viewport_width as f64 / 2.0 + 0.5).floor() as i32
        }).collect();
        let edge_y: Vec<i32> = (first_row..=last_row + 1).map(|r| {
            ((r as f64 * ts - cam.y) * cam.zoom + cam.viewport_y as f64
                + cam.viewport_height as f64 / 2.0 + 0.5).floor() as i32
        }).collect();

        for row in first_row..=last_row {
            for col in first_col..=last_col {
                let Some(tile) = level.tile(col, row) else { continue };
                if tile.tile_id == 0 {
                    continue;
                }
                let (cx, cy) = ((col - first_col) as usize, (row - first_row) as usize);
                let dest = Rect::new(edge_x[cx], edge_y[cy],
                                     (edge_x[cx + 1] - edge_x[cx]).max(1) as u32,
                                     (edge_y[cy + 1] - edge_y[cy]).max(1) as u32);

                if tile.is_damaged() {
                    let mask = tile.mask(registry);
                    if mask.iter().all(|b| *b == 0) {
                        continue; // nothing left to draw
                    }
                    let current = self.damaged.get(&(col, row));
                    let stale = current.map_or(true, |(id, version, _)| {
                        *id != tile.tile_id || *version != tile.damage_version()
                    });
                    if stale {
                        let Some(def) = registry.get(tile.tile_id) else { continue };
                        let picture = self.damaged_picture(def, mask);
                        let texture = textures::from_rgba(tc, TILE_SIZE as u32, TILE_SIZE as u32, &picture)?;
                        apply_scale_mode(&texture, self.smooth);
                        if let Some((_, _, old)) = self.damaged.insert(
                            (col, row), (tile.tile_id, tile.damage_version(), texture))
                        {
                            // SAFETY: the old texture is no longer used
                            unsafe { old.destroy() };
                        }
                    }
                    let (_, _, texture) = &self.damaged[&(col, row)];
                    canvas.copy(texture, None, dest)?;
                } else if let Some(texture) = self.kinds.get(&tile.tile_id) {
                    canvas.copy(texture, None, dest)?;
                }
            }
        }
        Ok(())
    }
}
