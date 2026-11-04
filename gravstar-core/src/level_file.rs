//! Reading and writing level files (JSON). The writer lays the file out the
//! way the Python game did: the header one item per line and each row of
//! tiles on a single line, so a level diffs and reads well in a text editor.

use std::fmt;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::constants::{LEVEL_FORMAT_VERSION, TILE_SIZE};
use crate::level::{DeliveryPoint, Level, SpawnPoint};
use crate::tile_instance::TileInstance;
use crate::tile_registry::TileRegistry;

#[derive(Debug)]
pub enum LevelError {
    Io(std::io::Error),
    Json(serde_json::Error),
    /// The file is well formed but does not describe a usable level
    Invalid(String),
}

impl fmt::Display for LevelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LevelError::Io(e) => write!(f, "cannot read or write the level file: {e}"),
            LevelError::Json(e) => write!(f, "the level file is not valid: {e}"),
            LevelError::Invalid(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for LevelError {}

impl From<std::io::Error> for LevelError {
    fn from(e: std::io::Error) -> Self {
        LevelError::Io(e)
    }
}

impl From<serde_json::Error> for LevelError {
    fn from(e: serde_json::Error) -> Self {
        LevelError::Json(e)
    }
}

/// What is in the file
#[derive(Deserialize)]
struct LevelFile {
    name: String,
    width: usize,
    height: usize,
    tile_size: usize,
    spawn_points: Vec<SpawnPoint>,
    delivery_points: Vec<DeliveryPoint>,
    tiles: Vec<Vec<u16>>,
}

/// Load a level. Besides the level, returns warnings about anything that was
/// fixed up on the way (unknown tile ids become empty).
pub fn load(path: &Path, registry: &TileRegistry)
            -> Result<(Level, Vec<String>), LevelError> {
    parse(&fs::read_to_string(path)?, registry)
}

pub fn parse(json: &str, registry: &TileRegistry)
             -> Result<(Level, Vec<String>), LevelError> {
    let file: LevelFile = serde_json::from_str(json)?;

    if file.tile_size != TILE_SIZE {
        return Err(LevelError::Invalid(format!(
            "tile size {} is not supported (tiles are {} pixels)",
            file.tile_size, TILE_SIZE)));
    }
    if file.tiles.len() != file.height {
        return Err(LevelError::Invalid(format!(
            "Level height mismatch: expected {} rows, found {}",
            file.height, file.tiles.len())));
    }
    for (row_index, row) in file.tiles.iter().enumerate() {
        if row.len() != file.width {
            return Err(LevelError::Invalid(format!(
                "Level width mismatch on row {}: expected {} cols, found {}",
                row_index, file.width, row.len())));
        }
    }

    let mut warnings = Vec::new();
    let mut tiles = Vec::with_capacity(file.width * file.height);
    for (row_index, row) in file.tiles.iter().enumerate() {
        for (col_index, &tile_id) in row.iter().enumerate() {
            if tile_id != 0 && registry.get(tile_id).is_none() {
                warnings.push(format!(
                    "Warning: unknown tile_id {tile_id} at row {row_index}, col {col_index} - replaced with empty"));
                tiles.push(TileInstance::new(0));
            } else {
                tiles.push(TileInstance::new(tile_id));
            }
        }
    }

    for spawn in &file.spawn_points {
        if spawn.col >= file.width || spawn.row >= file.height {
            return Err(LevelError::Invalid(format!(
                "Spawn point {} is out of bounds: col {}, row {}",
                spawn.id, spawn.col, spawn.row)));
        }
    }
    for point in &file.delivery_points {
        if point.col >= file.width || point.row >= file.height {
            return Err(LevelError::Invalid(format!(
                "Delivery point id {} is out of bounds: col {}, row {}",
                point.id, point.col, point.row)));
        }
    }

    let level = Level {
        name: file.name,
        width: file.width,
        height: file.height,
        tiles,
        spawn_points: file.spawn_points,
        delivery_points: file.delivery_points,
    };
    Ok((level, warnings))
}

/// The level as file text
pub fn to_json(level: &Level) -> String {
    let points = |list: Vec<(usize, usize, u32)>| -> String {
        let items: Vec<String> = list.iter()
            .map(|(col, row, id)| format!("{{\"col\": {col}, \"row\": {row}, \"id\": {id}}}"))
            .collect();
        format!("[{}]", items.join(", "))
    };
    let spawns = points(level.spawn_points.iter().map(|p| (p.col, p.row, p.id)).collect());
    let deliveries = points(level.delivery_points.iter().map(|p| (p.col, p.row, p.id)).collect());
    let name = serde_json::to_string(&level.name).unwrap();

    let mut out = String::new();
    out.push_str("{\n");
    out.push_str(&format!("  \"name\": {name},\n"));
    out.push_str(&format!("  \"version\": {LEVEL_FORMAT_VERSION},\n"));
    out.push_str(&format!("  \"width\": {},\n", level.width));
    out.push_str(&format!("  \"height\": {},\n", level.height));
    out.push_str(&format!("  \"tile_size\": {TILE_SIZE},\n"));
    out.push_str(&format!("  \"spawn_points\": {spawns},\n"));
    out.push_str(&format!("  \"delivery_points\": {deliveries},\n"));
    out.push_str("  \"tiles\": [\n");
    for row in 0..level.height {
        let ids: Vec<String> = (0..level.width)
            .map(|col| level.tiles[row * level.width + col].tile_id.to_string())
            .collect();
        let comma = if row + 1 < level.height { "," } else { "" };
        out.push_str(&format!("    [{}]{comma}\n", ids.join(", ")));
    }
    out.push_str("  ]\n}\n");
    out
}

pub fn save(level: &Level, path: &Path) -> Result<(), LevelError> {
    fs::write(path, to_json(level))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMALL: &str = r#"{
  "name": "small",
  "version": 1,
  "width": 3,
  "height": 2,
  "tile_size": 32,
  "spawn_points": [{"col": 1, "row": 0, "player": 1}, {"col": 2, "row": 1, "id": 2}],
  "delivery_points": [{"col": 0, "row": 1, "id": 1}],
  "tiles": [
    [1, 0, 6],
    [0, 2, 45]
  ]
}"#;

    fn registry() -> TileRegistry {
        TileRegistry::from_tileset()
    }

    #[test]
    fn loads_a_small_level() {
        let (level, warnings) = parse(SMALL, &registry()).unwrap();
        assert!(warnings.is_empty());
        assert_eq!((level.width, level.height), (3, 2));
        assert_eq!(level.tile(2, 0).unwrap().tile_id, 6);
        assert_eq!(level.tile(2, 1).unwrap().tile_id, 45);
        assert_eq!(level.spawn_points[0].id, 1); // "player" accepted
        assert_eq!(level.delivery_points.len(), 1);
    }

    #[test]
    fn saving_and_reloading_gives_the_same_level() {
        let reg = registry();
        let (level, _) = parse(SMALL, &reg).unwrap();
        let text = to_json(&level);
        let (again, _) = parse(&text, &reg).unwrap();
        assert_eq!(to_json(&again), text);
        assert!(text.contains("\"spawn_points\": [{\"col\": 1, \"row\": 0, \"id\": 1}, "));
        assert!(text.contains("    [1, 0, 6],\n    [0, 2, 45]\n"));
    }

    #[test]
    fn unknown_tiles_become_empty_with_a_warning() {
        let text = SMALL.replace("45", "99");
        let (level, warnings) = parse(&text, &registry()).unwrap();
        assert_eq!(level.tile(2, 1).unwrap().tile_id, 0);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("unknown tile_id 99 at row 1, col 2"));
    }

    #[test]
    fn bad_levels_are_rejected() {
        let reg = registry();
        let wrong_rows = SMALL.replace("\"height\": 2", "\"height\": 3");
        assert!(matches!(parse(&wrong_rows, &reg), Err(LevelError::Invalid(_))));
        let wrong_cols = SMALL.replace("[0, 2, 45]", "[0, 2]");
        assert!(matches!(parse(&wrong_cols, &reg), Err(LevelError::Invalid(_))));
        let off_map = SMALL.replace("\"col\": 2, \"row\": 1, \"id\": 2", "\"col\": 9, \"row\": 1, \"id\": 2");
        assert!(matches!(parse(&off_map, &reg), Err(LevelError::Invalid(_))));
        let big_tiles = SMALL.replace("\"tile_size\": 32", "\"tile_size\": 64");
        assert!(matches!(parse(&big_tiles, &reg), Err(LevelError::Invalid(_))));
        assert!(matches!(parse("{ nonsense", &reg), Err(LevelError::Json(_))));
    }
}
