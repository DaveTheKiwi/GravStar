//! Loads every level file found in the project's assets/levels folder and
//! checks it survives a save and reload. Skipped if the folder is not there.

use gravstar_core::level_file;
use gravstar_core::tile_registry::TileRegistry;
use std::path::PathBuf;

fn levels_folder() -> Option<PathBuf> {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    for candidate in ["assets/levels", "reference-python/assets/levels"] {
        let path = workspace.join(candidate);
        if path.is_dir() {
            return Some(path);
        }
    }
    None
}

#[test]
fn every_real_level_loads_and_round_trips() {
    let Some(folder) = levels_folder() else {
        println!("no assets/levels folder found - skipping");
        return;
    };
    let registry = TileRegistry::from_tileset();
    let mut loaded = 0;
    for entry in std::fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().map_or(true, |e| e != "json") {
            continue;
        }
        let (level, warnings) = level_file::load(&path, &registry)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(warnings.is_empty(), "{}: {warnings:?}", path.display());

        let text = level_file::to_json(&level);
        let (again, _) = level_file::parse(&text, &registry).unwrap();
        assert_eq!(level_file::to_json(&again), text, "{}", path.display());
        loaded += 1;

        // The big level must come back byte for byte as the Python game
        // wrote it (apart from the line endings)
        if path.file_name().unwrap() == "spaceport.json" {
            let original = std::fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
            assert_eq!(text, original, "spaceport.json is not reproduced exactly");
        }
    }
    println!("{loaded} level files checked");
}
