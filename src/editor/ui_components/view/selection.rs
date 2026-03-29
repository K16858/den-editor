use super::{Location, buffer::Buffer};
use std::ops::Range;

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

    pub fn get_ranges(&self, buffer: &Buffer) -> Vec<(usize, Range<usize>)> {
        let normalized = self.normalize();
        if normalized.is_empty() {
            return Vec::new();
        }

        let mut ranges = Vec::new();
        let start_line = normalized.start.line_idx;
        let end_line = normalized.end.line_idx;

        for line_idx in start_line..=end_line {
            if let Some(line) = buffer.lines.get(line_idx) {
                let start_byte = if line_idx == start_line {
                    line.grapheme_to_byte_idx(normalized.start.grapheme_idx)
                } else {
                    0
                };

                let end_byte = if line_idx == end_line {
                    line.grapheme_to_byte_idx(normalized.end.grapheme_idx)
                } else {
                    line.line_length()
                };

                if start_byte < end_byte {
                    ranges.push((line_idx, start_byte..end_byte));
                }
            }
        }

        ranges
    }
}
