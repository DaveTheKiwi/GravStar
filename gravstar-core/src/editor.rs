//! The level editor's brain: the level being edited, the tools, selection,
//! copy and paste, undo and redo, and the rules for what each mouse button
//! does. No drawing and no windows - the game crate feeds it mouse
//! positions (as tile positions) and draws what it holds.
//!
//! Mouse rules. The primary (left) button uses the tool. The secondary
//! (right) button turns things a quarter turn: the chosen tile (with Draw and
//! Fill), or the whole selection or the block being pasted (with Select),
//! tile art and all. The middle button deletes: it erases (Draw, Erase),
//! fills with empty (Fill), empties the selection (Select), ends a paste, or
//! removes a spawn point (Spawn). Panning and zooming are the view's business.

use crate::constants::*;
use crate::editor_document::{EditorDocument, TileRect};
use crate::editor_history::{History, ResizeRecord, SelectionChange, Step};
use crate::editor_palette::Palette;
use crate::editor_rotate::rotate_block;
use crate::editor_selection::Selection;
use crate::level::SpawnPoint;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Draw,
    Erase,
    Fill,
    Select,
    Spawn,
    /// Click a tile to choose it for drawing (the eyedropper)
    Pick,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    /// Left: use the tool
    Primary,
    /// Right: turn a quarter turn
    Secondary,
    /// Middle: delete
    Middle,
}

/// A message for the status bar
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub text: String,
    pub is_error: bool,
}

/// A copied block of tiles. It holds only tile ids, so it works in any level.
#[derive(Clone, Debug, Default)]
pub struct Clipboard {
    pub tiles: Vec<u16>,
    pub width: usize,
    pub height: usize,
}

impl Clipboard {
    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }
}

/// A fill that was worked out for the preview
struct FillCache {
    revision: u64,
    limit: Option<TileRect>,
    size: (usize, usize),
    region: Vec<usize>,
    member: Vec<bool>,
}

/// Remembers how a selection has been turned, so that turning it round four
/// times brings it back exactly where it started (a block of odd and even
/// sizes cannot turn about its middle without landing on half tiles, so which
/// way to round depends on how many turns it has had)
struct TurnTrack {
    origin: TileRect,
    turns: u8,
    current: TileRect,
}

/// A stroke of the Draw or Erase tool in progress
struct Stroke {
    tile: u16,
    last: Option<(i32, i32)>,
}

pub struct Editor {
    pub doc: EditorDocument,
    pub palette: Palette,
    pub tool: Tool,
    pub selection: Selection,
    pub clipboard: Clipboard,
    /// A pasted block is following the mouse; each click stamps it down
    pub pasting: bool,
    pub status: Option<Status>,
    /// Goes up whenever the level changes in any way (so pictures and caches
    /// made from it know when they are out of date)
    pub revision: u64,
    history: History,
    pending: Step,
    spawns_before: Option<Vec<SpawnPoint>>,
    stroke: Option<Stroke>,
    saved_id: u64,
    saved_name: String,
    fill_cache: Option<FillCache>,
    turn_track: Option<TurnTrack>,
}

impl Editor {
    pub fn new() -> Self {
        Self::with_document(EditorDocument::new(
            EDITOR_DEFAULT_LEVEL_NAME, EDITOR_DEFAULT_LEVEL_WIDTH, EDITOR_DEFAULT_LEVEL_HEIGHT))
    }

    pub fn with_document(doc: EditorDocument) -> Self {
        let history = History::new(EDITOR_MAX_UNDO_STEPS);
        Editor {
            saved_id: history.current_id(),
            saved_name: doc.name.clone(),
            doc,
            palette: Palette::new(),
            tool: Tool::Draw,
            selection: Selection::default(),
            clipboard: Clipboard::default(),
            pasting: false,
            status: None,
            revision: 0,
            history,
            pending: Step::default(),
            spawns_before: None,
            stroke: None,
            fill_cache: None,
            turn_track: None,
        }
    }

    // ---- messages

    pub fn set_status(&mut self, text: impl Into<String>) {
        self.status = Some(Status { text: text.into(), is_error: false });
    }

    pub fn set_error(&mut self, text: impl Into<String>) {
        self.status = Some(Status { text: text.into(), is_error: true });
    }

    pub fn clear_status(&mut self) {
        self.status = None;
    }

    // ---- the level as a whole

    /// Start again with this level (a new or loaded one). Undo is cleared, and
    /// so is the selection; the clipboard survives so tiles can be copied
    /// between levels.
    pub fn replace_document(&mut self, doc: EditorDocument) {
        self.stroke = None;
        self.pending = Step::default();
        self.spawns_before = None;
        self.doc = doc;
        self.history.clear();
        self.saved_id = self.history.current_id();
        self.saved_name = self.doc.name.clone();
        self.selection.clear();
        self.pasting = false;
        self.turn_track = None;
        self.revision += 1;
    }

    pub fn new_level(&mut self, name: &str, width: usize, height: usize) {
        self.replace_document(EditorDocument::new(name, width, height));
        self.set_status("New level");
    }

    /// Unsaved changes? True until the level is saved, and again after any
    /// change - but not after undoing right back to the saved state.
    pub fn is_modified(&self) -> bool {
        self.history.current_id() != self.saved_id || self.doc.name != self.saved_name
    }

    pub fn mark_saved(&mut self) {
        self.saved_id = self.history.current_id();
        self.saved_name = self.doc.name.clone();
    }

    pub fn set_name(&mut self, name: &str) {
        self.doc.name = name.to_string();
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub fn undo_count(&self) -> usize {
        self.history.undo_count()
    }

    // ---- recording edits

    /// Set the tile at an index, noting the change for undo. Putting a tile
    /// on a spawn point removes the spawn point (they only live on empty tiles).
    fn set_index(&mut self, index: usize, tile: u16) {
        let old = self.doc.tiles[index];
        if old == tile {
            return;
        }
        self.pending.cells.push((index, old, tile));
        self.doc.tiles[index] = tile;
        self.revision += 1;
        if tile != 0 && !self.doc.spawn_points.is_empty() {
            let (col, row) = ((index % self.doc.width) as i32, (index / self.doc.width) as i32);
            if self.doc.spawn_at(col, row).is_some() {
                self.note_spawns();
                self.doc.remove_spawn(col, row);
            }
        }
    }

    /// Remember the spawn points as they were, before the first change to them
    fn note_spawns(&mut self) {
        if self.spawns_before.is_none() {
            self.spawns_before = Some(self.doc.spawn_points.clone());
        }
    }

    /// Finish the edit being recorded: it becomes one undo step. Returns how
    /// many spawn points it removed.
    fn commit(&mut self) -> usize {
        self.commit_with(None)
    }

    /// Like `commit`, noting how the edit changed the selection too
    fn commit_with(&mut self, selection: Option<(Option<TileRect>, Option<TileRect>)>) -> usize {
        let mut step = std::mem::take(&mut self.pending);
        step.selection = selection;
        let mut removed = 0;
        if let Some(before) = self.spawns_before.take() {
            removed = before.len().saturating_sub(self.doc.spawn_points.len());
            if before != self.doc.spawn_points {
                step.spawns = Some((before, self.doc.spawn_points.clone()));
                self.revision += 1;
            }
        }
        self.history.push(step);
        removed
    }

    fn removed_message(count: usize) -> String {
        format!(" - removed {count} spawn point{}", if count == 1 { "" } else { "s" })
    }

    // ---- tools

    pub fn set_tool(&mut self, tool: Tool) {
        self.finish_stroke();
        self.tool = tool;
        self.pasting = false;
    }

    /// The mouse button went down on a tile (which may be outside the level)
    pub fn pointer_down(&mut self, button: Button, col: i32, row: i32) {
        self.finish_stroke();
        if self.pasting {
            match button {
                Button::Primary => {
                    let (c, r) = self.paste_origin(col, row);
                    self.paste_at(c, r);
                }
                Button::Secondary => self.turn_in_hand(true),
                Button::Middle => {
                    self.pasting = false;
                    self.set_status("Paste finished");
                }
            }
            return;
        }
        match (self.tool, button) {
            (Tool::Draw, Button::Primary) => self.begin_stroke(self.palette.tile_id(), col, row),
            (Tool::Draw, Button::Middle) | (Tool::Erase, Button::Primary | Button::Middle) => {
                self.begin_stroke(0, col, row)
            }
            (Tool::Draw | Tool::Fill, Button::Secondary) => self.palette.rotate(true),
            (Tool::Fill, Button::Primary) => self.fill_at(col, row, self.palette.tile_id()),
            (Tool::Fill, Button::Middle) => self.fill_at(col, row, 0),
            (Tool::Select, Button::Primary) => {
                if self.doc.in_bounds(col, row) {
                    self.selection.begin(col, row);
                }
            }
            (Tool::Select, Button::Secondary) => {
                self.turn_selection(true);
            }
            (Tool::Select, Button::Middle) => {
                self.delete_selection();
            }
            (Tool::Spawn, Button::Primary) => self.add_spawn(col, row),
            (Tool::Spawn, Button::Middle) => self.remove_spawn(col, row),
            (Tool::Pick, Button::Primary) => self.pick(col, row),
            (Tool::Erase | Tool::Spawn | Tool::Pick, _) => {}
        }
    }

    /// The mouse moved to another tile with the button held
    pub fn pointer_drag(&mut self, col: i32, row: i32) {
        if self.stroke.is_some() {
            self.stroke_to(col, row);
        } else if self.selection.dragging {
            let (w, h) = (self.doc.width as i32, self.doc.height as i32);
            self.selection.drag_to(col.clamp(0, w - 1), row.clamp(0, h - 1));
        }
    }

    pub fn pointer_up(&mut self) {
        self.finish_stroke();
        self.selection.end();
    }

    // ---- drawing and erasing

    fn begin_stroke(&mut self, tile: u16, col: i32, row: i32) {
        self.stroke = Some(Stroke { tile, last: None });
        self.stroke_to(col, row);
    }

    /// Paint every tile on the way from where the stroke last was to here,
    /// so a fast drag leaves no gaps
    fn stroke_to(&mut self, col: i32, row: i32) {
        let Some(stroke) = &mut self.stroke else { return };
        let (tile, from) = (stroke.tile, stroke.last.unwrap_or((col, row)));
        stroke.last = Some((col, row));
        let (mut x, mut y) = from;
        let (dx, dy) = ((col - x).abs(), -(row - y).abs());
        let (sx, sy) = (if x < col { 1 } else { -1 }, if y < row { 1 } else { -1 });
        let mut err = dx + dy;
        loop {
            if let Some(index) = self.doc.index(x, y) {
                self.set_index(index, tile);
            }
            if x == col && y == row {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    fn finish_stroke(&mut self) {
        if self.stroke.take().is_some() {
            let removed = self.commit();
            if removed > 0 {
                self.set_status(format!("Removed {removed} spawn point{}",
                                        if removed == 1 { "" } else { "s" }));
            }
        }
    }

    // ---- fill

    fn fill_limit(&self) -> Option<TileRect> {
        self.selection.rect()
    }

    pub fn fill_at(&mut self, col: i32, row: i32, tile: u16) {
        if !self.doc.in_bounds(col, row) {
            return;
        }
        let region = self.doc.find_fill_region(col, row, self.fill_limit());
        if region.is_empty() {
            self.set_status("Outside the selection - Esc clears the selection");
            return;
        }
        if self.doc.tile_id(col, row) == tile {
            self.set_status("Nothing to fill - that area is already that tile");
            return;
        }
        for &index in &region {
            self.set_index(index, tile);
        }
        let removed = self.commit();
        let mut message = format!("Filled {} tile{}", region.len(),
                                  if region.len() == 1 { "" } else { "s" });
        if removed > 0 {
            message += &Self::removed_message(removed);
        }
        self.set_status(message);
    }

    /// The tiles a primary-button fill at (col, row) would change, as indices
    /// into the document (empty if it would do nothing). Moving about inside
    /// the same region reuses the last answer, so hovering over a huge area
    /// stays smooth.
    pub fn fill_preview(&mut self, col: i32, row: i32) -> &[usize] {
        let tile = self.palette.tile_id();
        let limit = self.fill_limit();
        let size = (self.doc.width, self.doc.height);
        let index = match self.doc.index(col, row) {
            Some(i) if self.doc.tiles[i] != tile => i,
            _ => {
                self.fill_cache = None;
                return &[];
            }
        };
        let fresh = self.fill_cache.as_ref().map_or(false, |c| {
            c.revision == self.revision && c.limit == limit && c.size == size && c.member[index]
        });
        if !fresh {
            let region = self.doc.find_fill_region(col, row, limit);
            let mut member = vec![false; self.doc.tiles.len()];
            for &i in &region {
                member[i] = true;
            }
            self.fill_cache = Some(FillCache { revision: self.revision, limit, size, region, member });
        }
        &self.fill_cache.as_ref().unwrap().region
    }

    // ---- spawn points

    fn add_spawn(&mut self, col: i32, row: i32) {
        if !self.doc.in_bounds(col, row) {
            return;
        }
        if self.doc.tile_id(col, row) != 0 {
            self.set_error("Spawn points can only be placed on empty tiles");
            return;
        }
        self.note_spawns();
        if self.doc.add_spawn(col, row) {
            self.commit();
            self.revision += 1;
            self.set_status(format!("Spawn point {} placed", self.doc.spawn_points.len()));
        } else {
            self.spawns_before = None;
            self.set_status("There is a spawn point there already");
        }
    }

    fn remove_spawn(&mut self, col: i32, row: i32) {
        self.note_spawns();
        if self.doc.remove_spawn(col, row) {
            self.commit();
            self.revision += 1;
            self.set_status("Spawn point removed");
        } else {
            self.spawns_before = None;
        }
    }

    // ---- the eyedropper

    fn pick(&mut self, col: i32, row: i32) {
        let id = self.doc.tile_id(col, row);
        if id != 0 && self.palette.select_tile(id) {
            self.tool = Tool::Draw;
        }
    }

    // ---- selection, copy, cut, paste

    pub fn select_all(&mut self) {
        self.selection.select_all(self.doc.width, self.doc.height);
        self.set_status(format!("Selected everything ({} x {})", self.doc.width, self.doc.height));
    }

    pub fn clear_selection(&mut self) {
        self.selection.clear();
        self.pasting = false;
    }

    /// Esc: ends a paste, otherwise clears the selection
    pub fn escape(&mut self) {
        if self.pasting {
            self.pasting = false;
            self.set_status("Paste finished");
        } else {
            self.selection.clear();
        }
    }

    pub fn copy_selection(&mut self) -> bool {
        let Some(rect) = self.selection.rect() else {
            self.set_status("Nothing selected");
            return false;
        };
        let Some((tiles, width, height)) = self.doc.copy_region(rect) else {
            self.set_status("Nothing selected");
            return false;
        };
        self.clipboard = Clipboard { tiles, width, height };
        self.set_status(format!("Copied {width} x {height} tiles"));
        true
    }

    pub fn cut_selection(&mut self) -> bool {
        if !self.copy_selection() {
            return false;
        }
        self.clear_selected_tiles();
        self.set_status(format!("Cut {} x {} tiles", self.clipboard.width, self.clipboard.height));
        true
    }

    /// Empty the selected tiles (the clipboard is left alone)
    pub fn delete_selection(&mut self) -> bool {
        let Some((_, _, w, h)) = self.selection.rect() else {
            self.set_status("Nothing selected");
            return false;
        };
        self.clear_selected_tiles();
        self.set_status(format!("Deleted {w} x {h} tiles"));
        true
    }

    /// (Spawn points sit on empty tiles, so emptying tiles leaves them alone.)
    fn clear_selected_tiles(&mut self) {
        let Some(rect) = self.selection.rect() else { return };
        let Some((left, top, w, h)) = self.doc.clip_rect(rect) else { return };
        for r in top..top + h as i32 {
            for c in left..left + w as i32 {
                if let Some(i) = self.doc.index(c, r) {
                    self.set_index(i, 0);
                }
            }
        }
        self.commit();
    }

    /// Start pasting: the clipboard block follows the mouse and each click
    /// stamps it down, until the paste is finished (secondary button or Esc)
    pub fn begin_paste(&mut self) -> bool {
        if self.clipboard.is_empty() {
            self.set_status("Nothing to paste - copy or cut something first");
            return false;
        }
        self.set_tool(Tool::Select);
        self.pasting = true;
        self.set_status("Click to paste. Right-click or Esc to finish");
        true
    }

    /// Where the top-left corner of the pasted block goes when the mouse is
    /// over (col, row): the block is centred on the mouse, so that turning it
    /// keeps it under the mouse
    pub fn paste_origin(&self, col: i32, row: i32) -> (i32, i32) {
        (col - (self.clipboard.width / 2) as i32, row - (self.clipboard.height / 2) as i32)
    }

    /// Turn the block being pasted a quarter turn, tiles and all
    pub fn turn_in_hand(&mut self, clockwise: bool) {
        let c = &self.clipboard;
        if c.is_empty() {
            return;
        }
        let tiles = rotate_block(&c.tiles, c.width, c.height, clockwise);
        self.clipboard = Clipboard { tiles, width: c.height, height: c.width };
    }

    /// Turn the selected tiles a quarter turn, tiles and all, about the
    /// middle of the selection. Everything in the selection moves to its new
    /// place (so a tall block becomes a wide one); the selection follows.
    /// Refused if the turned block would hang off the edge of the level.
    /// Spawn points stay where they are (and are removed if a solid tile
    /// lands on one). One undo step.
    pub fn turn_selection(&mut self, clockwise: bool) -> bool {
        let Some(rect) = self.selection.rect() else {
            self.set_status("Nothing selected");
            return false;
        };
        let track = match self.turn_track.take() {
            Some(t) if t.current == rect => t,
            _ => TurnTrack { origin: rect, turns: 0, current: rect },
        };
        let turns = (track.turns + if clockwise { 1 } else { 3 }) % 4;
        let (c0, r0, w0, h0) = track.origin;
        let target: TileRect = if turns % 2 == 0 {
            track.origin
        } else {
            (c0 + (w0 as i32 - h0 as i32).div_euclid(2), r0 + (h0 as i32 - w0 as i32).div_euclid(2), h0, w0)
        };
        if self.doc.clip_rect(target) != Some(target) {
            self.turn_track = Some(track);
            self.set_error("No room to turn that here - it would hang off the edge of the level");
            return false;
        }
        let (c, r, w, h) = rect;
        let Some((tiles, _, _)) = self.doc.copy_region(rect) else { return false };
        let turned = rotate_block(&tiles, w, h, clockwise);
        for row in r..r + h as i32 {
            for col in c..c + w as i32 {
                if let Some(i) = self.doc.index(col, row) {
                    self.set_index(i, 0);
                }
            }
        }
        for dy in 0..target.3 {
            for dx in 0..target.2 {
                if let Some(i) = self.doc.index(target.0 + dx as i32, target.1 + dy as i32) {
                    self.set_index(i, turned[dy * target.2 + dx]);
                }
            }
        }
        let removed = self.commit_with(Some((Some(rect), Some(target))));
        self.selection.set_rect(target);
        self.turn_track = Some(TurnTrack { origin: track.origin, turns, current: target });
        let mut message = String::from("Turned the selection");
        if removed > 0 {
            message += &Self::removed_message(removed);
        }
        self.set_status(message);
        true
    }

    /// Stamp the clipboard with its top-left corner on (col, row). Empty
    /// tiles in the block overwrite too (it lands as a solid rectangle), and
    /// tiles put on spawn points remove them. The pasted area becomes the
    /// selection.
    pub fn paste_at(&mut self, col: i32, row: i32) -> bool {
        let (w, h) = (self.clipboard.width, self.clipboard.height);
        if self.clipboard.is_empty() {
            return false;
        }
        let Some(written) = self.doc.clip_rect((col, row, w, h)) else {
            self.set_status("Nothing pasted - that is outside the level");
            return false;
        };
        for dy in 0..h {
            for dx in 0..w {
                let (c, r) = (col + dx as i32, row + dy as i32);
                if let Some(i) = self.doc.index(c, r) {
                    let tile = self.clipboard.tiles[dy * w + dx];
                    self.set_index(i, tile);
                }
            }
        }
        let before = self.selection.rect();
        let removed = self.commit_with(Some((before, Some(written))));
        self.selection.set_rect(written);
        let mut message = format!("Pasted {} x {} tiles", written.2, written.3);
        if (written.2, written.3) != (w, h) {
            message += " (cropped at the level edge)";
        }
        if removed > 0 {
            message += &Self::removed_message(removed);
        }
        self.set_status(message);
        true
    }

    // ---- whole-level edits

    /// Empty the whole level, spawn points included
    pub fn clear_level(&mut self) {
        self.finish_stroke();
        self.note_spawns();
        for i in 0..self.doc.tiles.len() {
            self.set_index(i, 0);
        }
        self.doc.spawn_points.clear();
        self.commit();
        self.revision += 1;
        self.set_status("Level cleared");
    }

    /// Change the level's size, keeping the tiles that still fit. Spawn and
    /// delivery points outside the new size are removed. Can be undone.
    pub fn resize(&mut self, width: usize, height: usize) -> Result<(), String> {
        if !(MIN_LEVEL_WIDTH..=MAX_LEVEL_WIDTH).contains(&width)
            || !(MIN_LEVEL_HEIGHT..=MAX_LEVEL_HEIGHT).contains(&height)
        {
            return Err(format!("A level is {MIN_LEVEL_WIDTH}-{MAX_LEVEL_WIDTH} tiles wide and \
                                {MIN_LEVEL_HEIGHT}-{MAX_LEVEL_HEIGHT} tiles high"));
        }
        if (width, height) == (self.doc.width, self.doc.height) {
            return Ok(());
        }
        self.finish_stroke();
        let record = ResizeRecord::capture(&self.doc, width, height);
        let dropped = self.doc.resize(width, height);
        let selection = Some((self.selection.rect(), None));
        self.history.push(Step { resize: Some(Box::new(record)), selection, ..Default::default() });
        self.selection.clear();
        self.pasting = false;
        self.revision += 1;
        let mut message = format!("Resized to {width} x {height}");
        if dropped > 0 {
            message += &format!(" - removed {dropped} point{} outside it",
                                if dropped == 1 { "" } else { "s" });
        }
        self.set_status(message);
        Ok(())
    }

    // ---- undo and redo

    pub fn undo(&mut self) -> bool {
        self.finish_stroke();
        let done = self.history.undo(&mut self.doc);
        if let Some(selection) = done {
            self.after_history_move("Undone", selection);
        }
        done.is_some()
    }

    pub fn redo(&mut self) -> bool {
        self.finish_stroke();
        let done = self.history.redo(&mut self.doc);
        if let Some(selection) = done {
            self.after_history_move("Redone", selection);
        }
        done.is_some()
    }

    fn after_history_move(&mut self, message: &str, selection: SelectionChange) {
        self.revision += 1;
        match selection {
            Some(Some(rect)) => self.selection.set_rect(rect),
            Some(None) => self.selection.clear(),
            None => {}
        }
        // a resize undone or redone can leave the selection outside the level
        if let Some((c, r, w, h)) = self.selection.rect() {
            if c as usize + w > self.doc.width || r as usize + h > self.doc.height {
                self.selection.clear();
            }
        }
        self.set_status(message);
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Button::{Middle, Primary, Secondary};

    fn editor() -> Editor {
        Editor::with_document(EditorDocument::new("t", 20, 12))
    }

    fn click(e: &mut Editor, button: Button, col: i32, row: i32) {
        e.pointer_down(button, col, row);
        e.pointer_up();
    }

    fn solid_count(e: &Editor) -> usize {
        e.doc.tiles.iter().filter(|&&t| t != 0).count()
    }

    #[test]
    fn a_whole_drag_is_one_undo_step() {
        let mut e = editor();
        e.pointer_down(Primary, 2, 2);
        e.pointer_drag(3, 2);
        e.pointer_drag(4, 2);
        e.pointer_up();
        assert_eq!(solid_count(&e), 3);
        assert_eq!(e.undo_count(), 1);
        e.undo();
        assert_eq!(solid_count(&e), 0);
        e.redo();
        assert_eq!(solid_count(&e), 3);
    }

    #[test]
    fn a_fast_drag_leaves_no_gaps() {
        let mut e = editor();
        e.pointer_down(Primary, 0, 0);
        e.pointer_drag(10, 5); // the mouse jumped a long way in one frame
        e.pointer_up();
        // a line from (0,0) to (10,5) touches 11 tiles, each column once
        assert_eq!(solid_count(&e), 11);
        for col in 0..=10 {
            assert!((0..=5).any(|row| e.doc.tile_id(col, row) != 0), "gap at column {col}");
        }
    }

    #[test]
    fn the_middle_button_erases_with_the_draw_tool() {
        let mut e = editor();
        e.pointer_down(Primary, 1, 1);
        e.pointer_drag(5, 1);
        e.pointer_up();
        click(&mut e, Middle, 3, 1);
        assert_eq!(e.doc.tile_id(3, 1), 0);
        assert_eq!(solid_count(&e), 4);
        e.set_tool(Tool::Erase);
        e.pointer_down(Primary, 1, 1);
        e.pointer_drag(5, 1);
        e.pointer_up();
        assert_eq!(solid_count(&e), 0);
    }

    #[test]
    fn drawing_outside_the_level_does_nothing() {
        let mut e = editor();
        click(&mut e, Primary, -3, 4);
        click(&mut e, Primary, 99, 4);
        assert_eq!(solid_count(&e), 0);
        assert_eq!(e.undo_count(), 0, "no change, no undo step");
        // a stroke that runs off the edge paints the part inside
        e.pointer_down(Primary, 18, 0);
        e.pointer_drag(25, 0);
        e.pointer_up();
        assert_eq!(solid_count(&e), 2);
    }

    #[test]
    fn the_palette_tile_is_what_gets_drawn() {
        let mut e = editor();
        e.palette.select_tile(3);
        click(&mut e, Primary, 1, 1);
        assert_eq!(e.doc.tile_id(1, 1), 3);
    }

    #[test]
    fn spawn_points_go_on_empty_tiles_only_and_can_be_removed() {
        let mut e = editor();
        e.set_tool(Tool::Spawn);
        click(&mut e, Primary, 2, 2);
        click(&mut e, Primary, 6, 2);
        assert_eq!(e.doc.spawn_points.len(), 2);
        e.set_tool(Tool::Draw);
        click(&mut e, Primary, 9, 9);
        e.set_tool(Tool::Spawn);
        click(&mut e, Primary, 9, 9);
        assert!(e.status.as_ref().unwrap().is_error);
        assert_eq!(e.doc.spawn_points.len(), 2);
        click(&mut e, Middle, 2, 2);
        assert_eq!(e.doc.spawn_points.len(), 1);
        assert_eq!(e.doc.spawn_points[0].id, 1);
        e.undo();
        assert_eq!(e.doc.spawn_points.len(), 2);
    }

    #[test]
    fn drawing_over_a_spawn_point_removes_it_and_undo_brings_it_back() {
        let mut e = editor();
        e.set_tool(Tool::Spawn);
        click(&mut e, Primary, 4, 4);
        e.set_tool(Tool::Draw);
        e.pointer_down(Primary, 3, 4);
        e.pointer_drag(5, 4);
        e.pointer_up();
        assert!(e.doc.spawn_points.is_empty());
        assert_eq!(e.doc.tile_id(4, 4), 1);
        e.undo();
        assert_eq!(e.doc.spawn_points.len(), 1);
        assert_eq!(e.doc.tile_id(4, 4), 0);
        e.redo();
        assert!(e.doc.spawn_points.is_empty());
    }

    #[test]
    fn fill_fills_a_region_in_one_step_and_the_middle_button_fills_with_empty() {
        let mut e = editor();
        e.set_tool(Tool::Fill);
        click(&mut e, Primary, 5, 5);
        assert_eq!(solid_count(&e), 20 * 12);
        assert_eq!(e.undo_count(), 1);
        click(&mut e, Middle, 5, 5);
        assert_eq!(solid_count(&e), 0);
        click(&mut e, Middle, 5, 5);
        assert_eq!(e.undo_count(), 2, "filling what is already empty is not a step");
        e.undo();
        e.undo();
        assert_eq!(solid_count(&e), 0);
    }

    #[test]
    fn a_fill_stays_inside_the_selection() {
        let mut e = editor();
        e.set_tool(Tool::Select);
        e.pointer_down(Primary, 2, 2);
        e.pointer_drag(4, 3);
        e.pointer_up();
        e.set_tool(Tool::Fill);
        click(&mut e, Primary, 3, 3);
        assert_eq!(solid_count(&e), 6);
        click(&mut e, Primary, 15, 9); // outside the selection: refused
        assert_eq!(solid_count(&e), 6);
        e.escape();
        assert!(!e.selection.has_selection());
    }

    #[test]
    fn a_fill_over_a_spawn_point_removes_it() {
        let mut e = editor();
        e.set_tool(Tool::Spawn);
        click(&mut e, Primary, 4, 4);
        e.set_tool(Tool::Fill);
        click(&mut e, Primary, 0, 0);
        assert!(e.doc.spawn_points.is_empty());
        e.undo();
        assert_eq!(e.doc.spawn_points.len(), 1);
        assert_eq!(solid_count(&e), 0);
    }

    #[test]
    fn the_fill_preview_matches_the_fill_and_is_reused_while_nothing_changes() {
        let mut e = editor();
        e.set_tool(Tool::Draw);
        e.pointer_down(Primary, 10, 0);
        e.pointer_drag(10, 11);
        e.pointer_up(); // a wall splits the level in two
        e.set_tool(Tool::Fill);
        let n = e.fill_preview(2, 2).len();
        assert_eq!(n, 10 * 12);
        let rev = e.revision;
        assert_eq!(e.fill_preview(3, 4).len(), n); // same region: from the cache
        assert_eq!(e.revision, rev);
        assert_eq!(e.fill_preview(10, 3).len(), 0, "already that tile: nothing to fill");
        click(&mut e, Primary, 2, 2);
        assert_eq!(e.fill_preview(15, 2).len(), 9 * 12, "edit made the cache stale");
    }

    #[test]
    fn copy_cut_and_paste() {
        let mut e = editor();
        e.pointer_down(Primary, 1, 1);
        e.pointer_drag(3, 1);
        e.pointer_up();
        e.set_tool(Tool::Select);
        e.pointer_down(Primary, 1, 1);
        e.pointer_drag(3, 2);
        e.pointer_up();
        assert!(e.copy_selection());
        assert_eq!((e.clipboard.width, e.clipboard.height), (3, 2));
        assert!(e.begin_paste());
        click(&mut e, Primary, 11, 6); // the block is centred on the mouse
        assert_eq!(e.doc.tile_id(10, 5), 1);
        assert_eq!(e.doc.tile_id(12, 5), 1);
        assert_eq!(e.doc.tile_id(10, 6), 0);
        assert_eq!(e.selection.rect(), Some((10, 5, 3, 2)));
        click(&mut e, Primary, 11, 9); // it keeps pasting until finished
        assert_eq!(e.doc.tile_id(11, 8), 1);
        click(&mut e, Middle, 0, 0);
        assert!(!e.pasting);
        // cut removes the original
        e.selection.set_rect((1, 1, 3, 1));
        assert!(e.cut_selection());
        assert_eq!(e.doc.tile_id(2, 1), 0);
        e.undo();
        assert_eq!(e.doc.tile_id(2, 1), 1);
        // delete leaves the clipboard alone
        e.selection.set_rect((10, 5, 3, 1));
        assert!(e.delete_selection());
        assert_eq!(e.doc.tile_id(11, 5), 0);
        assert!(!e.clipboard.is_empty());
    }

    #[test]
    fn pasting_at_the_edge_is_cropped_and_one_step_undoes_it() {
        let mut e = editor();
        e.pointer_down(Primary, 0, 0);
        e.pointer_drag(3, 0);
        e.pointer_up();
        e.selection.set_rect((0, 0, 4, 1));
        e.copy_selection();
        e.begin_paste();
        let steps = e.undo_count();
        e.paste_at(18, 4);
        assert_eq!(e.selection.rect(), Some((18, 4, 2, 1)));
        assert_eq!(e.undo_count(), steps + 1);
        e.paste_at(40, 40);
        assert_eq!(e.undo_count(), steps + 1, "a paste outside the level does nothing");
        e.undo();
        assert_eq!(e.doc.tile_id(18, 4), 0);
    }

    #[test]
    fn a_paste_replaces_everything_it_covers_and_removes_spawn_points_under_solid_tiles() {
        let mut e = editor();
        e.pointer_down(Primary, 0, 0);
        e.pointer_drag(1, 0);
        e.pointer_up();
        e.selection.set_rect((0, 0, 2, 1));
        e.copy_selection();
        e.set_tool(Tool::Spawn);
        click(&mut e, Primary, 5, 5);
        e.begin_paste();
        click(&mut e, Primary, 5, 5);
        assert!(e.doc.spawn_points.is_empty());
        e.undo();
        assert_eq!(e.doc.spawn_points.len(), 1);
    }

    #[test]
    fn nothing_to_copy_or_paste_says_so() {
        let mut e = editor();
        assert!(!e.copy_selection());
        assert!(!e.cut_selection());
        assert!(!e.delete_selection());
        assert!(!e.begin_paste());
        assert!(!e.pasting);
    }

    #[test]
    fn switching_tools_abandons_a_paste() {
        let mut e = editor();
        click(&mut e, Primary, 1, 1);
        e.selection.set_rect((1, 1, 1, 1));
        e.copy_selection();
        e.begin_paste();
        assert!(e.pasting);
        e.set_tool(Tool::Draw);
        assert!(!e.pasting);
    }

    #[test]
    fn escape_ends_a_paste_first_and_then_clears_the_selection() {
        let mut e = editor();
        click(&mut e, Primary, 1, 1);
        e.selection.set_rect((1, 1, 1, 1));
        e.copy_selection();
        e.begin_paste();
        e.escape();
        assert!(!e.pasting && e.selection.has_selection());
        e.escape();
        assert!(!e.selection.has_selection());
    }

    #[test]
    fn the_eyedropper_picks_a_tile_and_goes_back_to_drawing() {
        let mut e = editor();
        e.palette.select_tile(3);
        click(&mut e, Primary, 4, 4);
        e.palette.select_tile(1);
        e.set_tool(Tool::Pick);
        click(&mut e, Primary, 4, 4);
        assert_eq!(e.palette.tile_id(), 3);
        assert_eq!(e.tool, Tool::Draw);
        e.set_tool(Tool::Pick);
        click(&mut e, Primary, 8, 8); // empty: nothing picked
        assert_eq!(e.palette.tile_id(), 3);
        assert_eq!(e.tool, Tool::Pick);
    }

    #[test]
    fn modified_is_true_until_saved_and_false_again_after_undoing_back_to_the_save() {
        let mut e = editor();
        assert!(!e.is_modified());
        click(&mut e, Primary, 1, 1);
        assert!(e.is_modified());
        e.mark_saved();
        assert!(!e.is_modified());
        click(&mut e, Primary, 2, 2);
        assert!(e.is_modified());
        e.undo();
        assert!(!e.is_modified());
        e.undo();
        assert!(e.is_modified(), "now before the save");
        e.set_name("renamed");
        e.redo();
        assert!(e.is_modified());
        e.mark_saved();
        assert!(!e.is_modified());
        e.set_name("again");
        assert!(e.is_modified(), "a new name is a change");
    }

    #[test]
    fn loading_clears_the_undo_history_but_not_the_clipboard() {
        let mut e = editor();
        click(&mut e, Primary, 1, 1);
        e.selection.set_rect((1, 1, 1, 1));
        e.copy_selection();
        assert!(e.can_undo());
        let mut other = EditorDocument::new("other", 30, 15);
        other.tiles[0] = 2;
        e.replace_document(other);
        assert!(!e.can_undo() && !e.can_redo(), "undo must not reach into the old level");
        assert!(!e.selection.has_selection());
        assert!(!e.clipboard.is_empty());
        assert!(!e.is_modified());
        assert!(!e.undo());
        assert_eq!(e.doc.tile_id(0, 0), 2);
    }

    #[test]
    fn a_resize_is_recorded_so_it_can_be_undone() {
        let mut e = editor();
        click(&mut e, Primary, 15, 9);
        e.set_tool(Tool::Spawn);
        click(&mut e, Primary, 16, 10);
        assert!(e.resize(12, 10).is_ok());
        assert_eq!((e.doc.width, e.doc.height), (12, 10));
        assert!(e.doc.spawn_points.is_empty());
        assert!(e.is_modified());
        e.undo();
        assert_eq!((e.doc.width, e.doc.height), (20, 12));
        assert_eq!(e.doc.tile_id(15, 9), 1);
        assert_eq!(e.doc.spawn_points.len(), 1);
        e.redo();
        assert_eq!((e.doc.width, e.doc.height), (12, 10));
        assert!(e.resize(5, 5).is_err());
        assert!(e.resize(600, 20).is_err());
        assert!(e.resize(12, 10).is_ok(), "the same size is not an error");
    }

    #[test]
    fn undoing_a_resize_clears_a_selection_that_no_longer_fits() {
        let mut e = editor();
        e.resize(40, 30).unwrap();
        e.selection.set_rect((30, 20, 5, 5));
        e.undo();
        assert!(!e.selection.has_selection());
    }

    #[test]
    fn clearing_the_level_is_one_undoable_step() {
        let mut e = editor();
        e.pointer_down(Primary, 0, 0);
        e.pointer_drag(9, 0);
        e.pointer_up();
        e.set_tool(Tool::Spawn);
        click(&mut e, Primary, 5, 5);
        e.clear_level();
        assert_eq!(solid_count(&e), 0);
        assert!(e.doc.spawn_points.is_empty());
        e.undo();
        assert_eq!(solid_count(&e), 10);
        assert_eq!(e.doc.spawn_points.len(), 1);
    }

    #[test]
    fn a_new_edit_after_undo_drops_the_redo_chain() {
        let mut e = editor();
        click(&mut e, Primary, 1, 1);
        e.undo();
        assert!(e.can_redo());
        click(&mut e, Primary, 2, 2);
        assert!(!e.can_redo());
    }

    #[test]
    fn undo_in_the_middle_of_a_stroke_finishes_the_stroke_first() {
        let mut e = editor();
        e.pointer_down(Primary, 1, 1);
        e.pointer_drag(3, 1);
        e.undo(); // the stroke is closed, then undone
        assert_eq!(solid_count(&e), 0);
        e.pointer_drag(5, 1); // the mouse is still down, but the stroke is over
        e.pointer_up();
        assert_eq!(solid_count(&e), 0);
    }

    /// Put a 3 wide, 2 high block of distinct shapes at (4,4) and select it:
    ///   half-top(2)  slope(11)   corner(6)
    ///   solid(1)     empty       half-top(2)
    fn block_of_shapes(e: &mut Editor) {
        let shapes = [[2u16, 11, 6], [1, 0, 2]];
        for (dy, row) in shapes.iter().enumerate() {
            for (dx, &t) in row.iter().enumerate() {
                let i = e.doc.index(4 + dx as i32, 4 + dy as i32).unwrap();
                e.doc.tiles[i] = t;
            }
        }
        e.set_tool(Tool::Select);
        e.selection.set_rect((4, 4, 3, 2));
    }

    #[test]
    fn the_right_button_turns_the_chosen_tile_with_draw_and_fill() {
        let mut e = editor();
        e.palette.select_tile(2);
        click(&mut e, Secondary, 0, 0);
        assert_eq!(e.palette.tile_id(), 5);
        e.set_tool(Tool::Fill);
        click(&mut e, Secondary, 0, 0);
        assert_eq!(e.palette.tile_id(), 3);
        assert_eq!(e.undo_count(), 0, "turning the tile is not an edit of the level");
    }

    #[test]
    fn the_right_button_turns_a_selection_tiles_and_all() {
        let mut e = editor();
        block_of_shapes(&mut e);
        click(&mut e, Secondary, 0, 0);
        // 3 wide x 2 high becomes 2 wide x 3 high, about the same middle
        assert_eq!(e.selection.rect(), Some((4, 3, 2, 3)));
        let turned = |t| crate::editor_rotate::rotate_tile(t, true);
        // the left column, bottom to top, becomes the top row, left to right
        assert_eq!(e.doc.tile_id(4, 3), turned(1));
        assert_eq!(e.doc.tile_id(5, 3), turned(2));
        assert_eq!(e.doc.tile_id(4, 4), turned(0));
        assert_eq!(e.doc.tile_id(5, 4), turned(11));
        assert_eq!(e.doc.tile_id(4, 5), turned(2));
        assert_eq!(e.doc.tile_id(5, 5), turned(6));
        // nothing is left behind where the block used to be
        assert_eq!(e.doc.tile_id(6, 4), 0);
        assert_eq!(e.doc.tile_id(6, 5), 0);
        assert_eq!(solid_count(&e), 5);
    }

    #[test]
    fn four_turns_bring_the_selection_back_exactly_and_one_undo_undoes_one_turn() {
        for &(w, h) in &[(3usize, 2usize), (4, 2), (3, 3), (2, 5)] {
            let mut e = editor();
            e.set_tool(Tool::Select);
            e.selection.set_rect((6, 3, w, h));
            for i in 0..w * h {
                let idx = e.doc.index(6 + (i % w) as i32, 3 + (i / w) as i32).unwrap();
                e.doc.tiles[idx] = [2, 11, 6, 1, 0, 40][i % 6];
            }
            let tiles = e.doc.tiles.clone();
            for _ in 0..4 {
                click(&mut e, Secondary, 0, 0);
            }
            assert_eq!(e.doc.tiles, tiles, "{w}x{h}: four turns must restore the tiles");
            assert_eq!(e.selection.rect(), Some((6, 3, w, h)), "{w}x{h}: and the selection");
            e.undo();
            assert_eq!(e.selection.rect().map(|r| (r.2, r.3)), Some((h, w)), "{w}x{h}: undo puts the selection back");
            e.redo();
            assert_eq!(e.selection.rect(), Some((6, 3, w, h)));
            assert_eq!(e.doc.tiles, tiles);
        }
    }

    #[test]
    fn undoing_a_turn_restores_the_tiles_and_the_selection() {
        let mut e = editor();
        block_of_shapes(&mut e);
        let before = e.doc.tiles.clone();
        click(&mut e, Secondary, 0, 0);
        assert_ne!(e.doc.tiles, before);
        e.undo();
        assert_eq!(e.doc.tiles, before);
        assert_eq!(e.selection.rect(), Some((4, 4, 3, 2)));
    }

    #[test]
    fn turning_clockwise_four_times_with_the_other_tiles_untouched() {
        let mut e = editor();
        e.set_tool(Tool::Draw);
        click(&mut e, Primary, 15, 10); // something outside the selection
        block_of_shapes(&mut e);
        click(&mut e, Secondary, 0, 0);
        click(&mut e, Secondary, 0, 0);
        assert_eq!(e.doc.tile_id(15, 10), 1);
        assert_eq!(e.selection.rect(), Some((4, 4, 3, 2)), "a half turn lands back on the same place");
    }

    #[test]
    fn a_turn_that_would_leave_the_level_is_refused() {
        let mut e = editor();
        e.set_tool(Tool::Select);
        e.selection.set_rect((0, 0, 4, 2)); // turned it would need row -1
        e.doc.tiles[0] = 2;
        let before = e.doc.tiles.clone();
        click(&mut e, Secondary, 0, 0);
        assert!(e.status.as_ref().unwrap().is_error);
        assert_eq!(e.doc.tiles, before);
        assert_eq!(e.selection.rect(), Some((0, 0, 4, 2)));
        assert_eq!(e.undo_count(), 0);
    }

    #[test]
    fn turning_a_selection_removes_a_spawn_point_a_solid_tile_lands_on() {
        let mut e = editor();
        block_of_shapes(&mut e);
        e.doc.add_spawn(5, 3); // empty now, but the turned block puts a tile there
        click(&mut e, Secondary, 0, 0);
        assert!(e.doc.spawn_points.is_empty());
        e.undo();
        assert_eq!(e.doc.spawn_points.len(), 1);
    }

    #[test]
    fn with_nothing_selected_the_right_button_says_so() {
        let mut e = editor();
        e.set_tool(Tool::Select);
        click(&mut e, Secondary, 3, 3);
        assert_eq!(e.status.as_ref().unwrap().text, "Nothing selected");
    }

    #[test]
    fn the_right_button_turns_the_block_being_pasted_and_it_stays_under_the_mouse() {
        let mut e = editor();
        block_of_shapes(&mut e);
        e.copy_selection();
        e.begin_paste();
        click(&mut e, Secondary, 0, 0);
        assert_eq!((e.clipboard.width, e.clipboard.height), (2, 3));
        assert!(e.pasting, "turning does not end the paste");
        // paste it with the mouse on (12, 7): the 2 x 3 block is centred there
        assert_eq!(e.paste_origin(12, 7), (11, 6));
        click(&mut e, Primary, 12, 7);
        assert_eq!(e.selection.rect(), Some((11, 6, 2, 3)));
        let turned = |t| crate::editor_rotate::rotate_tile(t, true);
        assert_eq!(e.doc.tile_id(11, 6), turned(1));
        assert_eq!(e.doc.tile_id(12, 8), turned(6));
        // four turns in hand give the original block
        let original = e.clipboard.tiles.clone();
        let (w, h) = (e.clipboard.width, e.clipboard.height);
        for _ in 0..4 {
            click(&mut e, Secondary, 0, 0);
        }
        assert_eq!((e.clipboard.tiles, e.clipboard.width, e.clipboard.height), (original, w, h));
    }

    #[test]
    fn the_middle_button_empties_the_selection_and_ends_a_paste() {
        let mut e = editor();
        block_of_shapes(&mut e);
        e.copy_selection();
        click(&mut e, Middle, 0, 0);
        assert_eq!(solid_count(&e), 0);
        e.undo();
        assert_eq!(solid_count(&e), 5);
        e.begin_paste();
        click(&mut e, Middle, 0, 0);
        assert!(!e.pasting);
    }
}
