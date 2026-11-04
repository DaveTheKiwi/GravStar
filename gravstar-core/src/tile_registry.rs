//! Every kind of tile the game knows, looked up by id. Built from the tile
//! set (see tileset); a level's tiles refer to these by id.

use std::collections::HashMap;

use crate::constants::TILE_SIZE;
use crate::tile_art::{self, TileMask, EMPTY_MASK};
use crate::tileset::TILE_DEFS;

#[derive(Clone)]
pub struct TileDefinition {
    pub id: u16,
    pub name: &'static str,
    /// Tile 0 - nothing there
    pub is_empty: bool,
    /// Can damage remove pieces of it? (hardness below 1.0)
    pub destructible: bool,
    /// The undamaged collision mask, shared by every tile of this kind
    pub mask: TileMask,
    /// The tile's picture: TILE_SIZE x TILE_SIZE alpha values, row by row
    /// (includes the soft antialiased edge that the mask leaves out)
    pub alpha: Vec<u8>,
}

#[derive(Clone)]
pub struct TileRegistry {
    definitions: HashMap<u16, TileDefinition>,
}

impl TileRegistry {
    /// The full tile set, generated from the shape descriptions
    pub fn from_tileset() -> Self {
        let mut definitions = HashMap::new();
        definitions.insert(0, TileDefinition {
            id: 0, name: "empty", is_empty: true, destructible: false,
            mask: EMPTY_MASK, alpha: vec![0; TILE_SIZE * TILE_SIZE],
        });
        for def in TILE_DEFS {
            let art = tile_art::render(&def.shape);
            definitions.insert(def.id, TileDefinition {
                id: def.id,
                name: def.name,
                is_empty: false,
                destructible: def.hardness < 1.0,
                mask: art.mask,
                alpha: art.alpha,
            });
        }
        TileRegistry { definitions }
    }

    pub fn get(&self, id: u16) -> Option<&TileDefinition> {
        self.definitions.get(&id)
    }

    /// Number of definitions, including tile 0 (empty)
    pub fn count(&self) -> usize {
        self.definitions.len()
    }

    /// The undamaged mask for a tile id (empty for an unknown id)
    pub fn mask(&self, id: u16) -> &TileMask {
        self.definitions.get(&id).map_or(&EMPTY_MASK, |d| &d.mask)
    }
}
