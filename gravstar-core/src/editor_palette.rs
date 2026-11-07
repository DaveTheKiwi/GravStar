//! The editor's tile palette: the tile groups (a group is a shape and its
//! rotations) and which tile is currently chosen. Built from the tile set.

use crate::tileset::TILE_DEFS;

/// The order the groups are shown in
const DISPLAY_ORDER: [&str; 12] = [
    "solid", "half", "corner", "small_corner", "slope", "half_slope",
    "slope_mirrored", "half_slope_mirrored", "concave_large", "convex_large",
    "concave_small", "convex_small",
];

pub struct PaletteGroup {
    pub name: &'static str,
    /// What to call it when hovering over it
    pub tooltip: &'static str,
    /// The tile ids, in rotation order: each is the one before turned a
    /// quarter turn clockwise
    pub tiles: Vec<u16>,
}

pub struct Palette {
    pub groups: Vec<PaletteGroup>,
    pub current_group: usize,
    /// Which of the group's tiles is chosen
    pub current_rotation: usize,
}

impl Palette {
    pub fn new() -> Self {
        let mut groups: Vec<PaletteGroup> = Vec::new();
        for name in DISPLAY_ORDER {
            groups.push(Self::group(name));
        }
        // any group not in the list above goes at the end
        for def in TILE_DEFS {
            if !groups.iter().any(|g| g.name == def.group) {
                groups.push(Self::group(def.group));
            }
        }
        Palette { groups, current_group: 0, current_rotation: 0 }
    }

    fn group(name: &'static str) -> PaletteGroup {
        let mut members: Vec<_> = TILE_DEFS.iter().filter(|d| d.group == name).collect();
        members.sort_by_key(|d| d.rotation_order);
        PaletteGroup {
            name,
            tooltip: members.first().map_or("", |d| d.tooltip),
            tiles: members.iter().map(|d| d.id).collect(),
        }
    }

    /// The chosen tile
    pub fn tile_id(&self) -> u16 {
        self.groups[self.current_group].tiles[self.current_rotation]
    }

    /// The tile a group shows (it remembers its own rotation)
    pub fn shown_tile(&self, group: usize) -> u16 {
        let g = &self.groups[group];
        if group == self.current_group {
            g.tiles[self.current_rotation]
        } else {
            g.tiles[0]
        }
    }

    /// Turn the chosen tile a quarter turn clockwise (or back)
    pub fn rotate(&mut self, clockwise: bool) {
        let n = self.groups[self.current_group].tiles.len();
        self.current_rotation = if clockwise {
            (self.current_rotation + 1) % n
        } else {
            (self.current_rotation + n - 1) % n
        };
    }

    /// Choose a group, starting from its first tile
    pub fn select_group(&mut self, group: usize) {
        if group < self.groups.len() && group != self.current_group {
            self.current_group = group;
            self.current_rotation = 0;
        }
    }

    /// Choose a particular tile (for the eyedropper). False if it is not in
    /// the palette (empty tiles, for one).
    pub fn select_tile(&mut self, tile_id: u16) -> bool {
        for (g, group) in self.groups.iter().enumerate() {
            if let Some(r) = group.tiles.iter().position(|&t| t == tile_id) {
                self.current_group = g;
                self.current_rotation = r;
                return true;
            }
        }
        false
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tile_is_in_exactly_one_group() {
        let p = Palette::new();
        assert_eq!(p.groups.len(), 12);
        let mut all: Vec<u16> = p.groups.iter().flat_map(|g| g.tiles.iter().copied()).collect();
        all.sort();
        assert_eq!(all, (1..=45).collect::<Vec<u16>>());
    }

    #[test]
    fn the_first_group_is_the_solid_block() {
        let p = Palette::new();
        assert_eq!(p.groups[0].name, "solid");
        assert_eq!(p.tile_id(), 1);
    }

    #[test]
    fn rotating_goes_round_in_quarter_turns_and_back() {
        let mut p = Palette::new();
        p.select_group(1); // the half tiles: top, right, bottom, left
        assert_eq!(p.tile_id(), 2);
        p.rotate(true);
        assert_eq!(p.tile_id(), 5);
        p.rotate(true);
        assert_eq!(p.tile_id(), 3);
        p.rotate(false);
        p.rotate(false);
        p.rotate(false);
        assert_eq!(p.tile_id(), 4); // wrapped round the other way
        // a single-tile group just stays put
        p.select_group(0);
        p.rotate(true);
        assert_eq!(p.tile_id(), 1);
    }

    #[test]
    fn picking_a_tile_selects_its_group_and_rotation() {
        let mut p = Palette::new();
        assert!(p.select_tile(3));
        assert_eq!(p.groups[p.current_group].name, "half");
        assert_eq!(p.tile_id(), 3);
        assert!(!p.select_tile(0));
        assert!(!p.select_tile(999));
        assert_eq!(p.tile_id(), 3, "a failed pick changes nothing");
    }

    #[test]
    fn a_group_shows_its_own_rotation_only_while_chosen() {
        let mut p = Palette::new();
        p.select_group(1);
        p.rotate(true);
        assert_eq!(p.shown_tile(1), 5);
        p.select_group(2);
        assert_eq!(p.shown_tile(1), 2);
    }
}
