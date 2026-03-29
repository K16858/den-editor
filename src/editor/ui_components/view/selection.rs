use super::Location;

#[derive(Clone, Copy, Default)]
pub struct Selection {
    pub start: Location,
    pub end: Location,
}

impl Selection {
    pub fn new(start: Location, end: Location) -> Self {
        Self { start, end }
    }

    pub fn is_empty(&self) -> bool {
        self.start.line_idx == self.end.line_idx && self.start.grapheme_idx == self.end.grapheme_idx
    }

    pub fn normalize(&self) -> Self {
        if self.start.line_idx < self.end.line_idx
            || (self.start.line_idx == self.end.line_idx
                && self.start.grapheme_idx <= self.end.grapheme_idx)
        {
            *self
        } else {
            Self {
                start: self.end,
                end: self.start,
            }
        }
    }
}
