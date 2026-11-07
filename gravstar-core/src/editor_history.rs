//! Undo and redo for the editor. Each step remembers only what it changed
//! (the tiles, the spawn points, or - for a resize - the whole old level), so
//! a hundred steps on a big level cost very little.

use crate::editor_document::{EditorDocument, TileRect};
use crate::level::{DeliveryPoint, SpawnPoint};

/// What a resize destroyed, so that undo can bring it back
#[derive(Clone, Debug)]
pub struct ResizeRecord {
    old_width: usize,
    old_height: usize,
    old_tiles: Vec<u16>,
    old_spawns: Vec<SpawnPoint>,
    old_deliveries: Vec<DeliveryPoint>,
    new_width: usize,
    new_height: usize,
}

impl ResizeRecord {
    /// Take the old state, just before the document is resized
    pub fn capture(doc: &EditorDocument, new_width: usize, new_height: usize) -> Self {
        ResizeRecord {
            old_width: doc.width,
            old_height: doc.height,
            old_tiles: doc.tiles.clone(),
            old_spawns: doc.spawn_points.clone(),
            old_deliveries: doc.delivery_points.clone(),
            new_width,
            new_height,
        }
    }
}

/// One undoable edit
#[derive(Clone, Debug, Default)]
pub struct Step {
    /// Identifies the state of the level after this step (see History)
    pub id: u64,
    /// Changed tiles: (index, tile before, tile after)
    pub cells: Vec<(usize, u16, u16)>,
    /// The spawn points before and after, if they changed
    pub spawns: Option<(Vec<SpawnPoint>, Vec<SpawnPoint>)>,
    pub resize: Option<Box<ResizeRecord>>,
    /// The selection before and after, if the step changed it (so undo and
    /// redo can put it back)
    pub selection: Option<(Option<TileRect>, Option<TileRect>)>,
}

/// What undoing or redoing did to the selection: nothing (None), or set it to
/// this (a rectangle, or none at all)
pub type SelectionChange = Option<Option<TileRect>>;

impl Step {
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty() && self.spawns.is_none() && self.resize.is_none()
    }

    fn undo(&self, doc: &mut EditorDocument) {
        if let Some(r) = &self.resize {
            doc.width = r.old_width;
            doc.height = r.old_height;
            doc.tiles = r.old_tiles.clone();
            doc.spawn_points = r.old_spawns.clone();
            doc.delivery_points = r.old_deliveries.clone();
            return;
        }
        for &(index, before, _) in self.cells.iter().rev() {
            doc.tiles[index] = before;
        }
        if let Some((before, _)) = &self.spawns {
            doc.spawn_points = before.clone();
        }
    }

    fn redo(&self, doc: &mut EditorDocument) {
        if let Some(r) = &self.resize {
            doc.resize(r.new_width, r.new_height);
            return;
        }
        for &(index, _, after) in &self.cells {
            doc.tiles[index] = after;
        }
        if let Some((_, after)) = &self.spawns {
            doc.spawn_points = after.clone();
        }
    }
}

/// The undo and redo stacks. Every step gets a number, and the level's state
/// is named by the number of the last step applied (`current_id`), which is
/// how the editor knows whether the level is the same as when it was saved,
/// even after undoing back to it.
pub struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
    max_steps: usize,
    /// The state before the oldest remembered step
    base_id: u64,
    next_id: u64,
}

impl History {
    pub fn new(max_steps: usize) -> Self {
        History { undo: Vec::new(), redo: Vec::new(), max_steps, base_id: 0, next_id: 1 }
    }

    /// Forget everything (a level was loaded or started)
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.base_id = self.next_id;
        self.next_id += 1;
    }

    pub fn current_id(&self) -> u64 {
        self.undo.last().map_or(self.base_id, |s| s.id)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_count(&self) -> usize {
        self.undo.len()
    }

    /// Remember a finished edit (an empty one is ignored). Anything that
    /// could have been redone is gone.
    pub fn push(&mut self, mut step: Step) {
        if step.is_empty() {
            return;
        }
        step.id = self.next_id;
        self.next_id += 1;
        self.redo.clear();
        self.undo.push(step);
        if self.undo.len() > self.max_steps {
            let dropped = self.undo.remove(0);
            self.base_id = dropped.id;
        }
    }

    /// Undo the last step. None if there was nothing to undo.
    pub fn undo(&mut self, doc: &mut EditorDocument) -> Option<SelectionChange> {
        let step = self.undo.pop()?;
        step.undo(doc);
        let selection = step.selection.map(|(before, _)| before);
        self.redo.push(step);
        Some(selection)
    }

    /// Redo the last step undone. None if there was nothing to redo.
    pub fn redo(&mut self, doc: &mut EditorDocument) -> Option<SelectionChange> {
        let step = self.redo.pop()?;
        step.redo(doc);
        let selection = step.selection.map(|(_, after)| after);
        self.undo.push(step);
        Some(selection)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell_step(index: usize, before: u16, after: u16) -> Step {
        Step { cells: vec![(index, before, after)], ..Default::default() }
    }

    #[test]
    fn undo_and_redo_walk_back_and_forth() {
        let mut doc = EditorDocument::new("t", 3, 3);
        let mut h = History::new(10);
        doc.tiles[4] = 5;
        h.push(cell_step(4, 0, 5));
        doc.tiles[4] = 6;
        h.push(cell_step(4, 5, 6));
        assert!(h.undo(&mut doc).is_some());
        assert_eq!(doc.tiles[4], 5);
        assert!(h.undo(&mut doc).is_some());
        assert_eq!(doc.tiles[4], 0);
        assert!(h.undo(&mut doc).is_none());
        assert!(h.redo(&mut doc).is_some() && h.redo(&mut doc).is_some());
        assert_eq!(doc.tiles[4], 6);
        assert!(h.redo(&mut doc).is_none());
    }

    #[test]
    fn a_new_edit_ends_the_redo_chain_and_empty_edits_are_ignored() {
        let mut doc = EditorDocument::new("t", 3, 3);
        let mut h = History::new(10);
        h.push(cell_step(0, 0, 1));
        h.undo(&mut doc);
        assert!(h.can_redo());
        h.push(Step::default());
        assert!(h.can_redo(), "an empty step changes nothing");
        h.push(cell_step(1, 0, 2));
        assert!(!h.can_redo());
    }

    #[test]
    fn the_oldest_steps_fall_off_the_end() {
        let mut doc = EditorDocument::new("t", 3, 3);
        let mut h = History::new(3);
        for i in 0..5u16 {
            doc.tiles[0] = i + 1;
            h.push(cell_step(0, i, i + 1));
        }
        assert_eq!(h.undo_count(), 3);
        while h.undo(&mut doc).is_some() {}
        assert_eq!(doc.tiles[0], 2, "only the last three edits could be undone");
    }

    #[test]
    fn the_state_id_comes_back_after_undoing_to_it() {
        let mut doc = EditorDocument::new("t", 3, 3);
        let mut h = History::new(10);
        let saved = h.current_id();
        h.push(cell_step(0, 0, 1));
        assert_ne!(h.current_id(), saved);
        h.undo(&mut doc);
        assert_eq!(h.current_id(), saved);
        h.redo(&mut doc);
        assert_ne!(h.current_id(), saved);
    }

    #[test]
    fn a_resize_can_be_undone_and_redone() {
        let mut doc = EditorDocument::new("t", 6, 6);
        doc.tiles[5 * 6 + 5] = 9;
        doc.add_spawn(5, 5);
        let mut h = History::new(10);
        let record = ResizeRecord::capture(&doc, 3, 3);
        doc.resize(3, 3);
        h.push(Step { resize: Some(Box::new(record)), ..Default::default() });
        assert_eq!((doc.width, doc.spawn_points.len()), (3, 0));
        h.undo(&mut doc);
        assert_eq!((doc.width, doc.height, doc.tiles.len()), (6, 6, 36));
        assert_eq!(doc.tile_id(5, 5), 9);
        assert_eq!(doc.spawn_points.len(), 1);
        h.redo(&mut doc);
        assert_eq!((doc.width, doc.tiles.len(), doc.spawn_points.len()), (3, 9, 0));
    }

    #[test]
    fn spawn_point_changes_are_undone_too() {
        let mut doc = EditorDocument::new("t", 6, 6);
        let before = doc.spawn_points.clone();
        doc.add_spawn(2, 2);
        let mut h = History::new(10);
        h.push(Step { spawns: Some((before, doc.spawn_points.clone())), ..Default::default() });
        h.undo(&mut doc);
        assert!(doc.spawn_points.is_empty());
        h.redo(&mut doc);
        assert_eq!(doc.spawn_points.len(), 1);
    }
}
