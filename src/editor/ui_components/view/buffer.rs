use super::{FileInfo, Line, Location};
use std::fs::{File, read_to_string};
use std::io::Error;
use std::io::Write;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Default)]
pub struct Buffer {
    pub lines: Vec<Line>,
    pub file_info: FileInfo,
    pub modified: bool,
}

impl Buffer {
    pub fn load(file_name: &str) -> Result<Self, Error> {
        let contents = read_to_string(file_name)?;
        let mut lines = Vec::new();
        for value in contents.lines() {
            lines.push(Line::from(value));
        }
        Ok(Self {
            lines,
            file_info: FileInfo::from(file_name),
            modified: false,
        })
    }

    fn save_to_file(&self, file_info: &FileInfo) -> Result<(), Error> {
        if let Some(file_path) = &file_info.get_path() {
            let mut file = File::create(file_path)?;
            for line in &self.lines {
                writeln!(file, "{line}")?;
            }
        }
        Ok(())
    }

    pub fn save_as(&mut self, file_name: &str) -> Result<(), Error> {
        let file_info = FileInfo::from(file_name);
        self.save_to_file(&file_info)?;
        self.file_info = file_info;
        self.modified = false;
        Ok(())
    }

    pub fn save(&mut self) -> Result<(), Error> {
        if !self.file_info.has_path() {
            return Err(Error::other("No file path set"));
        }
        self.save_to_file(&self.file_info)?;
        self.modified = false;
        Ok(())
    }

    pub const fn is_file_loaded(&self) -> bool {
        self.file_info.has_path()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn height(&self) -> usize {
        self.lines.len()
    }

    pub fn insert_char(&mut self, character: char, at: Location) {
        if at.line_idx > self.height() {
            return;
        }
        if at.line_idx == self.height() {
            self.lines.push(Line::from(&character.to_string()));
            self.modified = true;
        } else if let Some(line) = self.lines.get_mut(at.line_idx) {
            line.insert_char(character, at.grapheme_idx);
            self.modified = true;
        }
    }

    pub fn delete(&mut self, at: Location) {
        if let Some(line) = self.lines.get(at.line_idx) {
            if at.grapheme_idx >= line.grapheme_count()
                && self.height() > at.line_idx.saturating_add(1)
            {
                let next_line = self.lines.remove(at.line_idx.saturating_add(1));
                self.lines[at.line_idx].append(&next_line);
                self.modified = true;
            } else if at.grapheme_idx < line.grapheme_count() {
                self.lines[at.line_idx].delete(at.grapheme_idx);
                self.modified = true;
            }
        }
    }

    pub fn insert_newline(&mut self, at: Location) {
        if at.line_idx == self.height() {
            self.lines.push(Line::default());
            self.modified = true;
        } else if let Some(line) = self.lines.get_mut(at.line_idx) {
            let new = line.split(at.grapheme_idx);
            self.lines.insert(at.line_idx.saturating_add(1), new);
            self.modified = true;
        }
    }

    pub fn insert_string(&mut self, mut at: Location, text: &str) -> Location {
        let sanitized = text.replace("\r\n", "\n").replace('\r', "\n");
        for (idx, line_text) in sanitized.split('\n').enumerate() {
            if idx > 0 {
                self.insert_newline(at);
                at.line_idx += 1;
                at.grapheme_idx = 0;
            }
            for ch in line_text.chars() {
                self.insert_char(ch, at);
                at.grapheme_idx += 1;
            }
        }
        at
    }

    /// Returns the content that would be deleted by `delete(loc)` (grapheme or newline).
    pub fn content_deleted_at(&self, loc: Location) -> Option<String> {
        if let Some(line) = self.lines.get(loc.line_idx) {
            if loc.grapheme_idx < line.grapheme_count() {
                return line.grapheme_at(loc.grapheme_idx);
            }
            if loc.grapheme_idx >= line.grapheme_count()
                && self.height() > loc.line_idx.saturating_add(1)
            {
                return Some("\n".to_string());
            }
        }
        None
    }

    /// Returns the location after "walking" from `from` over the characters in `text`
    /// (each `\n` advances to the next line, other chars advance `grapheme_idx`).
    fn location_after_text(from: Location, text: &str) -> Location {
        let mut at = from;
        for ch in text.chars() {
            if ch == '\n' {
                at.line_idx += 1;
                at.grapheme_idx = 0;
            } else {
                at.grapheme_idx += 1;
            }
        }
        at
    }

    /// Deletes from `from` (inclusive) to `to` (exclusive). Used for undo of Insert.
    pub fn delete_range(&mut self, from: Location, to: Location) {
        if from.line_idx >= self.height() {
            return;
        }
        if from == to {
            return;
        }
        if from.line_idx == to.line_idx {
            if let Some(line) = self.lines.get_mut(from.line_idx) {
                let start_byte = line.grapheme_to_byte_idx(from.grapheme_idx);
                let end_byte = line.grapheme_to_byte_idx(to.grapheme_idx);
                if start_byte < end_byte {
                    line.delete_byte_range(start_byte..end_byte);
                    self.modified = true;
                }
            }
            return;
        }
        // Multi-line: delete from `from` to end of first line, merge following lines, then delete leading span.
        let height = self.height();
        if to.line_idx >= height {
            return;
        }
        let mut graphemes_to_drop: usize = from.grapheme_idx;
        for idx in (from.line_idx + 1)..to.line_idx {
            graphemes_to_drop += self.lines.get(idx).map_or(0, Line::grapheme_count);
        }
        graphemes_to_drop += to.grapheme_idx;

        if let Some(line) = self.lines.get_mut(from.line_idx) {
            let start_byte = line.grapheme_to_byte_idx(from.grapheme_idx);
            let end_byte = line.line_length();
            if start_byte < end_byte {
                line.delete_byte_range(start_byte..end_byte);
                self.modified = true;
            }
        }
        for _ in (from.line_idx + 1)..=to.line_idx {
            if from.line_idx + 1 >= self.lines.len() {
                break;
            }
            let next_line = self.lines.remove(from.line_idx + 1);
            self.lines[from.line_idx].append(&next_line);
            self.modified = true;
        }
        if let Some(line) = self.lines.get_mut(from.line_idx) {
            let end_byte = line.grapheme_to_byte_idx(graphemes_to_drop);
            if end_byte > 0 {
                line.delete_byte_range(0..end_byte);
                self.modified = true;
            }
        }
    }

    /// Deletes the span of content that matches `text` starting at `from` (for undo of Insert).
    pub fn delete_span(&mut self, from: Location, text: &str) {
        let to = Self::location_after_text(from, text);
        self.delete_range(from, to);
    }

    /// Returns all non-overlapping match locations from the beginning of the document,
    /// in document order (top-to-bottom, left-to-right). Only single-line matches.
    pub fn find_all_matches(&self, query: &str) -> Vec<Location> {
        if query.is_empty() {
            return Vec::new();
        }
        let query_grapheme_count = query.graphemes(true).count();
        let mut matches = Vec::new();
        for (line_idx, line) in self.lines.iter().enumerate() {
            let mut from_grapheme_idx = 0;
            loop {
                let Some(grapheme_idx) = line.search_forward(query, from_grapheme_idx) else {
                    break;
                };
                matches.push(Location {
                    grapheme_idx,
                    line_idx,
                });
                from_grapheme_idx = grapheme_idx + query_grapheme_count.max(1);
                if from_grapheme_idx >= line.grapheme_count() {
                    break;
                }
            }
        }
        matches
    }

    pub fn search_forward(&self, query: &str, from: Location) -> Option<Location> {
        if query.is_empty() {
            return None;
        }
        let mut is_first = true;
        for (line_idx, line) in self
            .lines
            .iter()
            .enumerate()
            .cycle()
            .skip(from.line_idx)
            .take(self.lines.len().saturating_add(1))
        {
            let from_grapheme_idx = if is_first {
                is_first = false;
                from.grapheme_idx
            } else {
                0
            };
            if let Some(grapheme_idx) = line.search_forward(query, from_grapheme_idx) {
                return Some(Location {
                    grapheme_idx,
                    line_idx,
                });
            }
        }
        None
    }

    pub fn search_backward(&self, query: &str, from: Location) -> Option<Location> {
        if query.is_empty() {
            return None;
        }
        let mut is_first = true;
        for (line_idx, line) in self
            .lines
            .iter()
            .enumerate()
            .rev()
            .cycle()
            .skip(
                self.lines
                    .len()
                    .saturating_sub(from.line_idx)
                    .saturating_sub(1),
            )
            .take(self.lines.len().saturating_add(1))
        {
            let from_grapheme_idx = if is_first {
                is_first = false;
                from.grapheme_idx
            } else {
                line.grapheme_count()
            };
            if let Some(grapheme_idx) = line.search_backward(query, from_grapheme_idx) {
                return Some(Location {
                    grapheme_idx,
                    line_idx,
                });
            }
        }
        None
    }
}
