//! Turning tiles and blocks of tiles a quarter turn. The tile set has every
//! shape in four rotations (a group); turning a tile means swapping it for
//! the next one in its group, so turning a block means moving the tiles to
//! their new places and swapping each for its turned self.

use crate::tileset::TILE_DEFS;

/// The tile that is this one turned a quarter turn (clockwise, or back).
/// Tiles with no other rotations, and empty tiles, stay as they are.
pub fn rotate_tile(tile_id: u16, clockwise: bool) -> u16 {
    let Some(def) = TILE_DEFS.iter().find(|d| d.id == tile_id) else { return tile_id };
    let mut group: Vec<_> = TILE_DEFS.iter().filter(|d| d.group == def.group).collect();
    group.sort_by_key(|d| d.rotation_order);
    let at = group.iter().position(|d| d.id == tile_id).unwrap_or(0);
    let n = group.len();
    group[if clockwise { (at + 1) % n } else { (at + n - 1) % n }].id
}

/// A block of tiles (row by row, `width` x `height`) turned a quarter turn
/// round its middle: the result is `height` wide and `width` high.
pub fn rotate_block(tiles: &[u16], width: usize, height: usize, clockwise: bool) -> Vec<u16> {
    let mut out = vec![0u16; tiles.len()];
    for row in 0..height {
        for col in 0..width {
            let (new_col, new_row) = if clockwise {
                (height - 1 - row, col)
            } else {
                (row, width - 1 - col)
            };
            out[new_row * height + new_col] = rotate_tile(tiles[row * width + col], clockwise);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::TILE_SIZE;
    use crate::tile_registry::TileRegistry;

    /// A picture turned a quarter turn clockwise
    fn turned(alpha: &[u8]) -> Vec<u8> {
        let n = TILE_SIZE;
        let mut out = vec![0u8; n * n];
        for row in 0..n {
            for col in 0..n {
                out[col * n + (n - 1 - row)] = alpha[row * n + col];
            }
        }
        out
    }

    #[test]
    fn turning_a_tile_really_turns_its_picture_a_quarter_turn_clockwise() {
        let registry = TileRegistry::from_tileset();
        for def in TILE_DEFS.iter() {
            let next = rotate_tile(def.id, true);
            let a = turned(&registry.get(def.id).unwrap().alpha);
            let b = &registry.get(next).unwrap().alpha;
            let wrong = a.iter().zip(b.iter()).filter(|(x, y)| x.abs_diff(**y) > 40).count();
            assert!(wrong <= TILE_SIZE, "tile {} ({}) turned clockwise is not tile {} ({} pixels differ)",
                    def.id, def.name, next, wrong);
        }
    }

    #[test]
    fn four_turns_bring_a_tile_back_and_a_turn_back_undoes_a_turn() {
        for def in TILE_DEFS.iter() {
            let mut id = def.id;
            for _ in 0..4 {
                id = rotate_tile(id, true);
            }
            assert_eq!(id, def.id);
            assert_eq!(rotate_tile(rotate_tile(def.id, true), false), def.id);
        }
        assert_eq!(rotate_tile(0, true), 0);
        assert_eq!(rotate_tile(1, true), 1, "a solid block looks the same any way up");
        assert_eq!(rotate_tile(999, true), 999);
    }

    #[test]
    fn a_block_turns_clockwise_corner_to_corner() {
        // 3 wide, 2 high:      becomes 2 wide, 3 high:
        //   a b c                d a
        //   d e f                e b
        //                        f c
        let (a, b, c, d, e, f) = (1, 2, 3, 6, 7, 8);
        let tiles = [a, b, c, d, e, f];
        let turned = rotate_block(&tiles, 3, 2, true);
        let want = [rotate_tile(d, true), rotate_tile(a, true),
                    rotate_tile(e, true), rotate_tile(b, true),
                    rotate_tile(f, true), rotate_tile(c, true)];
        assert_eq!(turned, want);
    }

    #[test]
    fn four_turns_of_a_block_bring_it_back_and_anticlockwise_undoes_clockwise() {
        let tiles: Vec<u16> = (0..15).map(|i| [0, 2, 3, 6, 11, 18, 40][i % 7]).collect();
        let mut t = tiles.clone();
        let (mut w, mut h) = (5, 3);
        for _ in 0..4 {
            t = rotate_block(&t, w, h, true);
            std::mem::swap(&mut w, &mut h);
        }
        assert_eq!(t, tiles);
        let there = rotate_block(&tiles, 5, 3, true);
        assert_eq!(rotate_block(&there, 3, 5, false), tiles);
    }
}
