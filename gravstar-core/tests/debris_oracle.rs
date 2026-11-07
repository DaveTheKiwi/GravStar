//! Oracle test: the passes in which a cut-free piece crumbles (bottom edge
//! first, then the next edge up, ...) must be exactly those of the Python
//! version's CrumblingIsland. The order of pixels inside one pass is random
//! in both, and the two use different random number generators, so it is not
//! compared. Regenerate the fixture with fixtures/make_debris_oracle.py.

use gravstar_core::debris_system::DebrisSystem;
use gravstar_core::level::IslandPixel;
use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    pixels: Vec<(i32, i32)>,
    passes: Vec<Vec<(i32, i32)>>,
}

#[test]
fn the_passes_match_the_python_version() {
    let text = include_str!("fixtures/debris_layers.json");
    let cases: Vec<Case> = serde_json::from_str(text).expect("fixture parses");
    assert!(cases.len() >= 10);
    for (n, case) in cases.iter().enumerate() {
        let pixels: Vec<IslandPixel> = case.pixels.iter()
            .map(|&(x, y)| IslandPixel { x, y, tile_id: 1 }).collect();
        let mut debris = DebrisSystem::new(1);
        debris.add_island(&pixels);
        let mut passes: Vec<Vec<(i32, i32)>> = debris.islands[0].layers().iter().map(|l| {
            let mut v: Vec<_> = l.iter().map(|p| (p.x, p.y)).collect();
            v.sort();
            v
        }).collect();
        let mut expected = case.passes.clone();
        for p in expected.iter_mut() {
            p.sort();
        }
        passes.retain(|p| !p.is_empty());
        assert_eq!(passes, expected, "shape {n} crumbles in different passes");
    }
}
