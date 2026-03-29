use super::location::Location;

#[derive(Clone, Debug)]
pub enum EditOp {
    Insert {
        at: Location,
        text: String,
        /// Cursor position after redo (may differ from the text end, e.g. auto-close).
        cursor_after: Location,
    },
    Delete { at: Location, text: String },
    /// Multiple ops that are undone/redone as a single unit.
    Group(Vec<EditOp>),
}

#[derive(Default)]
pub struct UndoHistory {
    undo_stack: Vec<EditOp>,
    redo_stack: Vec<EditOp>,
}

impl UndoHistory {
    pub fn push_edit(&mut self, op: EditOp) {
        self.undo_stack.push(op);
    }

    pub fn pop_undo(&mut self) -> Option<EditOp> {
        self.undo_stack.pop()
    }

    pub fn pop_redo(&mut self) -> Option<EditOp> {
        self.redo_stack.pop()
    }

    pub fn push_redo(&mut self, op: EditOp) {
        self.redo_stack.push(op);
    }

    pub fn clear_redo(&mut self) {
        self.redo_stack.clear();
    }

    pub fn clear_all(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    pub fn has_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn has_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }
}
