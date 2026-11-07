//! Oracle test: flood fill, region copy and paste in the editor document must
//! give exactly what the Python version's LevelDocument gives, on random
//! levels. (Paste is done here the way the editor does it: every tile of the
//! block is written, and a solid one removes a spawn point under it.)
//! Regenerate the fixture with fixtures/make_editor_oracle.py.

use gravstar_core::editor::Editor;
use gravstar_core::editor_document::EditorDocument;
use serde::Deserialize;

#[derive(Deserialize)]
struct Fill {
    col: i32,
    row: i32,
    limit: Option<(i32, i32, usize, usize)>,
    region: Vec<(i32, i32)>,
}

#[derive(Deserialize)]
struct Copy {
    rect: (i32, i32, usize, usize),
    rows: Vec<Vec<u16>>,
    clip: Option<(i32, i32, usize, usize)>,
    has_tiles: bool,
}

#[derive(Deserialize)]
struct Paste {
    col: i32,
    row: i32,
    block: Vec<Vec<u16>>,
    written: Option<(i32, i32, usize, usize)>,
    removed: usize,
    tiles_after: Vec<u16>,
    spawns_after: Vec<(usize, usize)>,
}

#[derive(Deserialize)]
struct Case {
    width: usize,
    height: usize,
    tiles: Vec<u16>,
    spawns: Vec<(usize, usize)>,
    fill: Fill,
    copy: Copy,
    paste: Paste,
}

fn document(case: &Case) -> EditorDocument {
    let mut doc = EditorDocument::new("oracle", case.width, case.height);
    doc.tiles = case.tiles.clone();
    for &(col, row) in &case.spawns {
        doc.add_spawn(col as i32, row as i32);
    }
    doc
}

#[test]
fn fill_copy_and_paste_match_the_python_version() {
    let cases: Vec<Case> = serde_json::from_str(include_str!("fixtures/editor_oracle.json"))
        .expect("fixture parses");
    assert!(cases.len() >= 30);
    for (n, case) in cases.iter().enumerate() {
        let doc = document(case);

        // flood fill
        let f = &case.fill;
        let mut got: Vec<(i32, i32)> = doc.find_fill_region(f.col, f.row, f.limit).iter()
            .map(|&i| ((i % case.width) as i32, (i / case.width) as i32)).collect();
        got.sort_by_key(|&(c, r)| (c, r));
        let mut want = f.region.clone();
        want.sort_by_key(|&(c, r)| (c, r));
        assert_eq!(got, want, "case {n}: fill region");

        // copy
        let c = &case.copy;
        assert_eq!(doc.clip_rect(c.rect), c.clip, "case {n}: clip");
        assert_eq!(doc.region_has_tiles(c.rect), c.has_tiles, "case {n}: has tiles");
        let copied = doc.copy_region(c.rect).map(|(tiles, w, _)| {
            tiles.chunks(w).map(|r| r.to_vec()).collect::<Vec<_>>()
        }).unwrap_or_default();
        assert_eq!(copied, c.rows, "case {n}: copied tiles");

        // paste, through the editor's own clipboard
        let p = &case.paste;
        let mut editor = Editor::with_document(doc);
        editor.clipboard.width = p.block[0].len();
        editor.clipboard.height = p.block.len();
        editor.clipboard.tiles = p.block.iter().flatten().copied().collect();
        editor.paste_at(p.col, p.row);
        match p.written {
            None => assert_eq!(editor.undo_count(), 0, "case {n}: nothing should be pasted"),
            Some(rect) => assert_eq!(editor.selection.rect(), Some(rect), "case {n}: written"),
        }
        assert_eq!(editor.doc.tiles, p.tiles_after, "case {n}: tiles after paste");
        let spawns: Vec<(usize, usize)> =
            editor.doc.spawn_points.iter().map(|s| (s.col, s.row)).collect();
        assert_eq!(spawns, p.spawns_after, "case {n}: spawn points after paste");
        assert_eq!(case.spawns.len() - spawns.len(), p.removed, "case {n}: spawns removed");
    }
}
