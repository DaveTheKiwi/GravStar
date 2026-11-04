//! Finding the small pieces of terrain that a cut has set free.
//!
//! An island is a group of solid pixels joined to each other (diagonals
//! count) but to nothing else. It may straddle tile edges: tile boundaries
//! mean nothing to what is physically attached to what.
//!
//! The search is kept cheap three ways:
//!  * Only pixels touching the cut are used as starting points - a piece can
//!    only have been cut free if it touches the hole - and only the thin
//!    ring just outside the hole is visited to find them.
//!  * Each search gives up the moment the piece reaches `min_area` pixels,
//!    because it is then big enough to stay. A search never explores more
//!    than `min_area` pixels, however large the terrain is.
//!  * A starting pixel that touches a piece already known to be big is part
//!    of that piece, so it needs no search of its own.
//! A piece joined to an indestructible tile also stays (it cannot be
//! deleted, and is attached to something that is never going away).

use std::collections::{HashMap, HashSet};

use crate::constants::{ISLAND_SEARCH_MARGIN, ISLAND_SEED_RING_WIDTH, TILE_SIZE};
use crate::level::Level;
use crate::tile_registry::TileRegistry;

/// What a pixel is
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Empty,
    Destructible,
    Permanent,
}

type Pixel = (i32, i32);

/// After a circular cut of `radius` at (centre_x, centre_y), find the pieces
/// of terrain it cut free that are smaller than `min_area` pixels. Each
/// island is a list of world pixel positions, sorted by row then column, and
/// the islands come in the order their first starting pixel was met.
pub fn find_small_islands(level: &Level, registry: &TileRegistry,
                          centre_x: f64, centre_y: f64, radius: f64,
                          min_area: usize) -> Vec<Vec<Pixel>> {
    if min_area <= 1 {
        return Vec::new();
    }
    let ts = TILE_SIZE as i32;

    // (col, row) -> that tile's mask and whether it is destructible
    let mut tile_cache: HashMap<(i32, i32), Option<(&[u8], bool)>> = HashMap::new();
    let mut kind_at = |x: i32, y: i32| -> Kind {
        let (col, row) = (x.div_euclid(ts), y.div_euclid(ts));
        let info = *tile_cache.entry((col, row)).or_insert_with(|| {
            let tile = level.tile(col, row)?;
            if tile.tile_id == 0 {
                return None;
            }
            let def = registry.get(tile.tile_id)?;
            Some((&tile.mask(registry)[..], def.destructible))
        });
        let Some((mask, destructible)) = info else { return Kind::Empty };
        let bit = (y.rem_euclid(ts) * ts + x.rem_euclid(ts)) as usize;
        if mask[bit >> 3] & (1 << (bit & 7)) == 0 {
            Kind::Empty
        } else if destructible {
            Kind::Destructible
        } else {
            Kind::Permanent
        }
    };

    // Starting points: solid pixels just outside the hole, row by row, left
    // to right, visiting only the ring itself
    let inner2 = radius * radius;
    let outer = radius + ISLAND_SEED_RING_WIDTH;
    let outer2 = outer * outer;
    let reach = (radius + ISLAND_SEARCH_MARGIN).ceil() as i32;
    let mut seeds: Vec<Pixel> = Vec::new();
    let centre_row = centre_y.floor() as i32;
    for y in centre_row - reach..=centre_row + reach {
        let dy = (y as f64 + 0.5) - centre_y;
        let dy2 = dy * dy;
        if dy2 > outer2 {
            continue;
        }
        let outer_half = (outer2 - dy2).sqrt();
        let left = (centre_x - outer_half).floor() as i32 - 1;
        let right = (centre_x + outer_half).ceil() as i32 + 1;
        let spans: Vec<(i32, i32)> = if dy2 < inner2 {
            // Through the hole: only the arc on each side of it (the exact
            // test below decides; the +-1 just guards rounding)
            let inner_half = (inner2 - dy2).sqrt();
            vec![(left, (centre_x - inner_half).floor() as i32 + 1),
                 ((centre_x + inner_half).ceil() as i32 - 1, right)]
        } else {
            vec![(left, right)]
        };
        for (span_left, span_right) in spans {
            for x in span_left..=span_right {
                let dx = (x as f64 + 0.5) - centre_x;
                let d2 = dx * dx + dy2;
                if d2 > inner2 && d2 <= outer2 && kind_at(x, y) == Kind::Destructible {
                    seeds.push((x, y));
                }
            }
        }
    }

    let mut islands: Vec<Vec<Pixel>> = Vec::new();
    let mut settled: HashSet<Pixel> = HashSet::new(); // already in an island
    let mut big: HashSet<Pixel> = HashSet::new(); // in a piece that stays
    for seed in seeds {
        if settled.contains(&seed) || big.contains(&seed) {
            continue;
        }
        let (sx, sy) = seed;
        let touches_big = (sy - 1..=sy + 1)
            .any(|ny| (sx - 1..=sx + 1).any(|nx| big.contains(&(nx, ny))));
        if touches_big {
            big.insert(seed); // same piece as one already known to be big
            continue;
        }

        let mut piece: HashSet<Pixel> = HashSet::new();
        piece.insert(seed);
        let mut stack = vec![seed];
        let mut is_island = true;
        'search: while let Some((x, y)) = stack.pop() {
            for ny in y - 1..=y + 1 {
                for nx in x - 1..=x + 1 {
                    if piece.contains(&(nx, ny)) {
                        continue;
                    }
                    match kind_at(nx, ny) {
                        Kind::Empty => continue,
                        Kind::Permanent => {
                            is_island = false;
                            break 'search;
                        }
                        Kind::Destructible => {}
                    }
                    if piece.len() + 1 >= min_area {
                        is_island = false; // big enough to stay
                        break 'search;
                    }
                    piece.insert((nx, ny));
                    stack.push((nx, ny));
                }
            }
        }

        if is_island {
            settled.extend(piece.iter().copied());
            let mut pixels: Vec<Pixel> = piece.into_iter().collect();
            pixels.sort_by_key(|&(x, y)| (y, x));
            islands.push(pixels);
        } else {
            big.extend(piece);
        }
    }
    islands
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank(cols: usize, rows: usize) -> Level {
        Level::new("t", cols, rows)
    }

    /// Make a tile solid only where `keep(x, y)` (tile pixel coordinates)
    fn shape_tile(level: &mut Level, reg: &TileRegistry, col: usize, row: usize,
                  keep: impl Fn(usize, usize) -> bool) {
        level.set_tile(col, row, 1);
        let mut remove = Vec::new();
        for y in 0..TILE_SIZE {
            for x in 0..TILE_SIZE {
                if !keep(x, y) {
                    remove.push((x, y));
                }
            }
        }
        level.tile_mut(col as i32, row as i32).unwrap().remove_pixels(reg, &remove);
    }

    #[test]
    fn a_small_blob_straddling_two_tiles_is_found_whole() {
        let reg = TileRegistry::from_tileset();
        let mut level = blank(2, 1);
        // a 4 x 4 block at x 30..33, y 10..13: two columns in each tile
        shape_tile(&mut level, &reg, 0, 0, |x, y| x >= 30 && (10..14).contains(&y));
        shape_tile(&mut level, &reg, 1, 0, |x, y| x <= 1 && (10..14).contains(&y));
        let islands = find_small_islands(&level, &reg, 29.0, 12.0, 1.0, 40);
        assert_eq!(islands.len(), 1);
        assert_eq!(islands[0].len(), 16);
        assert_eq!(islands[0][0], (30, 10)); // sorted by row, then column
    }

    #[test]
    fn a_blob_at_the_size_limit_stays() {
        let reg = TileRegistry::from_tileset();
        let mut level = blank(1, 1);
        // 5 x 8 = 40 pixels: not smaller than 40, so it stays
        shape_tile(&mut level, &reg, 0, 0, |x, y| (10..15).contains(&x) && (10..18).contains(&y));
        assert!(find_small_islands(&level, &reg, 9.0, 12.0, 1.0, 40).is_empty());
        // 39 pixels: goes
        shape_tile(&mut level, &reg, 0, 0, |x, y| (10..15).contains(&x) && (10..18).contains(&y)
                   && !(x == 14 && y == 17));
        assert_eq!(find_small_islands(&level, &reg, 9.0, 12.0, 1.0, 40).len(), 1);
    }

    #[test]
    fn diagonal_neighbours_count_as_joined() {
        let reg = TileRegistry::from_tileset();
        let mut level = blank(1, 1);
        // a staircase of single pixels joined only corner to corner
        shape_tile(&mut level, &reg, 0, 0, |x, y| x >= 10 && x < 20 && y == x);
        let islands = find_small_islands(&level, &reg, 8.0, 12.0, 2.0, 40);
        assert_eq!(islands.len(), 1);
        assert_eq!(islands[0].len(), 10);
    }

    #[test]
    fn switched_off_when_the_area_is_one() {
        let reg = TileRegistry::from_tileset();
        let mut level = blank(1, 1);
        shape_tile(&mut level, &reg, 0, 0, |x, y| x == 10 && y == 10);
        assert!(find_small_islands(&level, &reg, 8.0, 10.0, 2.0, 1).is_empty());
    }

    #[test]
    fn carving_a_tile_into_four_small_corners_removes_them() {
        let reg = TileRegistry::from_tileset();
        let mut level = blank(3, 3);
        level.set_tile(1, 1, 1);
        let result = level.carve_circle(&reg, 48.0, 48.0, 20.0);
        assert_eq!(result.islands.len(), 4);
        assert!(result.islands.iter().all(|i| i.len() < 40 && i.iter().all(|p| p.tile_id == 1)));
        assert_eq!(result.changed, vec![(1, 1)]);
        assert!(level.tile(1, 1).unwrap().mask(&reg).iter().all(|b| *b == 0));
    }

    #[test]
    fn a_cut_that_leaves_one_big_ring_removes_nothing_extra() {
        let reg = TileRegistry::from_tileset();
        let mut level = blank(3, 3);
        level.set_tile(1, 1, 1);
        let result = level.carve_circle(&reg, 48.0, 48.0, 15.0);
        assert!(result.islands.is_empty());
        assert!(level.tile(1, 1).unwrap().mask(&reg).iter().any(|b| *b != 0));
    }

    #[test]
    fn cuts_beyond_the_level_edge_do_not_panic() {
        let reg = TileRegistry::from_tileset();
        let mut level = blank(2, 2);
        level.set_tile(0, 0, 1);
        let _ = level.carve_circle(&reg, -3.0, -3.0, 25.0);
        let _ = level.carve_circle(&reg, 70.0, 70.0, 25.0);
    }
}
