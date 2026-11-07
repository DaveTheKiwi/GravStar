//! GravStar core: pure game logic. No graphics, windowing or audio code
//! belongs in this crate - the game crate draws what the core describes.

pub mod camera;
pub mod collision;
pub mod controllers;
pub mod debris_system;
pub mod editor;
pub mod editor_document;
pub mod editor_history;
pub mod editor_palette;
pub mod editor_rotate;
pub mod editor_selection;
pub mod constants;
pub mod match_rules;
pub mod physics;
pub mod projectile;
pub mod projectile_system;
pub mod random;
pub mod raster;
pub mod settings;
pub mod ship;
pub mod ship_masks;
pub mod weapon;
pub mod world;
pub mod terrain_islands;
pub mod tile_art;
pub mod explosion_system;
pub mod level;
pub mod level_file;
pub mod menu;
pub mod minimap;
pub mod tile_instance;
pub mod tile_registry;
pub mod tileset;

pub fn version() -> &'static str {
    "gravstar-core 0.1"
}
