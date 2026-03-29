use super::super::{
    DocumentStatus, Line, NAME, Position, Size, VERSION,
    command::{Edit, Move, MoveDirection},
    highlight::{HighlightAnnotation, HighlightState, HighlighterRegistry},
    line::GetAnnotatedVisibleSubstrParams,
    terminal::Terminal,
};
use super::UIComponent;
use arboard::Clipboard;
use std::cmp::min;
use std::collections::HashMap;
mod buffer;
use buffer::Buffer;
use std::io::Error;
mod fileinfo;
use fileinfo::FileInfo;
mod searchinfo;
use searchinfo::SearchInfo;
mod location;
use location::Location;
mod search_direction;
use search_direction::SearchDirection;
mod selection;
use selection::Selection;
mod undo;
use undo::{EditOp, UndoHistory};

type HighlightCache = HashMap<usize, (Vec<HighlightAnnotation>, HighlightState, u64)>;

/// Pairs of opening and closing brackets/quotes for auto-close.
/// TODO: move this to a configuration file so users can customize the pairs.
const BRACKET_PAIRS: &[(char, char)] = &[
    ('(', ')'),
    ('[', ']'),
    ('{', '}'),
    ('"', '"'),
    ('\'', '\''),
];

/// Returns the matching closing character for `ch` if it is an opening bracket.
fn closing_bracket_for(ch: char) -> Option<char> {
    BRACKET_PAIRS
        .iter()
        .find(|(open, _)| *open == ch)
        .map(|(_, close)| *close)
}

/// Returns true if `ch` is a closing bracket defined in `BRACKET_PAIRS`.
fn is_closing_bracket(ch: char) -> bool {
    BRACKET_PAIRS.iter().any(|(_, close)| *close == ch)
}

#[derive(Default)]
pub struct View {
    buffer: Buffer,
    needs_redraw: bool,
    size: Size,
    text_location: Location,
    scroll_offset: Position,
    search_info: Option<SearchInfo>,
    highlighter_registry: HighlighterRegistry,
    highlight_cache: HighlightCache,
    cache_version: u64,
    selection: Option<Selection>,
    undo_history: UndoHistory,
}

impl View {
    pub fn get_status(&self) -> DocumentStatus {
        let language_name = self
            .buffer
            .file_info
            .get_path()
            .and_then(|p| p.extension())
            .and_then(|ext| ext.to_str())
            .and_then(|ext| self.highlighter_registry.get_highlighter(Some(ext)))
            .map(|h| h.language_name().to_string());

        DocumentStatus {
            total_lines: self.buffer.height(),
            current_line_idx: self.text_location.line_idx,
            file_name: format!("{}", self.buffer.file_info),
            is_modified: self.buffer.modified,
            language_name,
        }
    }

    fn render_welcome_screen(&self, origin_y: usize) -> Result<(), Error> {
        let Size { height, width } = self.size;
        let vertical_center = height / 3;

        for row in 0..height {
            let draw_row = origin_y + row;
            if row == vertical_center {
                Self::draw_welcome_message_at(draw_row, width)?;
            } else {
                Self::draw_empty_row(draw_row)?;
            }
        }
        Ok(())
    }

    const GUTTER_WIDTH: usize = 5;
    const GUTTER_PADDING: usize = 2;

    #[allow(clippy::too_many_lines)]
    fn render_buffer(&mut self, origin_y: usize) -> Result<(), Error> {
        let Size { height, width } = self.size;
        let gutter_total = Self::GUTTER_WIDTH + Self::GUTTER_PADDING;
        let content_width = width.saturating_sub(gutter_total);
        let top = self.scroll_offset.row;

        let highlighter = self
            .buffer
            .file_info
            .get_path()
            .and_then(|p| p.extension())
            .and_then(|ext| ext.to_str())
            .and_then(|ext| self.highlighter_registry.get_highlighter(Some(ext)));

        let mut state = HighlightState::default();
        if let Some(hl) = highlighter {
            for line_idx in 0..top {
                if let Some(line) = self.buffer.lines.get(line_idx) {
                    if let Some((_, cached_state, cached_version)) =
                        self.highlight_cache.get(&line_idx)
                    {
                        if *cached_version == self.cache_version {
                            state = *cached_state;
                        } else {
                            let (annotations, new_state) = hl.highlight_line(line, line_idx, state);
                            self.highlight_cache
                                .insert(line_idx, (annotations, new_state, self.cache_version));
                            state = new_state;
                        }
                    } else {
                        let (annotations, new_state) = hl.highlight_line(line, line_idx, state);
                        self.highlight_cache
                            .insert(line_idx, (annotations, new_state, self.cache_version));
                        state = new_state;
                    }
                }
            }
        }

        for screen_row in 0..height {
            let line_idx = top + screen_row;
            let draw_row = origin_y + screen_row;

            if let Some(line) = self.buffer.lines.get(line_idx) {
                let selection_range = self
                    .selection
                    .and_then(|sel| Self::selection_byte_range_for_line(sel, line, line_idx));

                let left = self.scroll_offset.col;
                let right = left + content_width;
                let line_num_str = format!(
                    "{:>w$}{pad}",
                    line_idx + 1,
                    w = Self::GUTTER_WIDTH - 1,
                    pad = " ".repeat(Self::GUTTER_PADDING + 1)
                );
                let query = self
                    .search_info
                    .as_ref()
                    .and_then(|search_info| search_info.query.as_deref());
                let selected_match = (self.text_location.line_idx == line_idx && query.is_some())
                    .then_some(self.text_location.grapheme_idx);

                if let Some((cached_annotations, cached_state, cached_version)) =
                    self.highlight_cache.get(&line_idx)
                {
                    if *cached_version == self.cache_version {
                        let (annotated_string, new_state) =
                            line.get_annotated_visible_substr(GetAnnotatedVisibleSubstrParams {
                                range: left..right,
                                query,
                                selected_match,
                                highlighter,
                                state: *cached_state,
                                cached_annotations: Some(cached_annotations),
                                selection_range,
                            });
                        state = new_state;
                        Terminal::print_annotated_row_with_prefix(
                            draw_row,
                            &line_num_str,
                            &annotated_string,
                            line_idx == self.text_location.line_idx,
                        )?;
                    } else if let Some(hl) = highlighter {
                        let (annotations, new_state) = hl.highlight_line(line, line_idx, state);
                        self.highlight_cache.insert(
                            line_idx,
                            (annotations.clone(), new_state, self.cache_version),
                        );
                        let (annotated_string, final_state) =
                            line.get_annotated_visible_substr(GetAnnotatedVisibleSubstrParams {
                                range: left..right,
                                query,
                                selected_match,
                                highlighter,
                                state: new_state,
                                cached_annotations: Some(&annotations),
                                selection_range,
                            });
                        state = final_state;
                        Terminal::print_annotated_row_with_prefix(
                            draw_row,
                            &line_num_str,
                            &annotated_string,
                            line_idx == self.text_location.line_idx,
                        )?;
                    } else {
                        let (annotated_string, new_state) =
                            line.get_annotated_visible_substr(GetAnnotatedVisibleSubstrParams {
                                range: left..right,
                                query,
                                selected_match,
                                highlighter: None,
                                state,
                                cached_annotations: None,
                                selection_range,
                            });
                        state = new_state;
                        Terminal::print_annotated_row_with_prefix(
                            draw_row,
                            &line_num_str,
                            &annotated_string,
                            line_idx == self.text_location.line_idx,
                        )?;
                    }
                } else if let Some(hl) = highlighter {
                    let (annotations, new_state) = hl.highlight_line(line, line_idx, state);
                    self.highlight_cache.insert(
                        line_idx,
                        (annotations.clone(), new_state, self.cache_version),
                    );
                    let (annotated_string, final_state) =
                        line.get_annotated_visible_substr(GetAnnotatedVisibleSubstrParams {
                            range: left..right,
                            query,
                            selected_match,
                            highlighter,
                            state: new_state,
                            cached_annotations: Some(&annotations),
                            selection_range,
                        });
                    state = final_state;
                    Terminal::print_annotated_row_with_prefix(
                        draw_row,
                        &line_num_str,
                        &annotated_string,
                        line_idx == self.text_location.line_idx,
                    )?;
                } else {
                    let (annotated_string, new_state) =
                        line.get_annotated_visible_substr(GetAnnotatedVisibleSubstrParams {
                            range: left..right,
                            query,
                            selected_match,
                            highlighter: None,
                            state,
                            cached_annotations: None,
                            selection_range,
                        });
                    state = new_state;
                    Terminal::print_annotated_row_with_prefix(
                        draw_row,
                        &line_num_str,
                        &annotated_string,
                        line_idx == self.text_location.line_idx,
                    )?;
                }
            } else {
                let empty_prefix = " ".repeat(Self::GUTTER_WIDTH + Self::GUTTER_PADDING);
                Self::render_line(draw_row, &format!("{empty_prefix}~"))?;
                state = HighlightState::default();
            }
        }
        Ok(())
    }

    fn draw_welcome_message_at(at: usize, width: usize) -> Result<(), Error> {
        let mut welcome_message = format!("{NAME} -- version {VERSION}");
        let gutter_total = Self::GUTTER_WIDTH + Self::GUTTER_PADDING;
        let content_width = width.saturating_sub(gutter_total);
        let len = welcome_message.len();
        let prefix = " ".repeat(gutter_total);
        if content_width > len {
            let padding = (content_width - len) / 2;
            let spaces = " ".repeat(padding.saturating_sub(1));
            welcome_message = format!("{prefix}~{spaces}{welcome_message}");
        } else {
            welcome_message = format!("{prefix}{welcome_message}");
        }
        welcome_message.truncate(width);
        Self::render_line(at, &welcome_message)?;
        Ok(())
    }

    fn draw_empty_row(at: usize) -> Result<(), Error> {
        let empty_prefix = " ".repeat(Self::GUTTER_WIDTH + Self::GUTTER_PADDING);
        Self::render_line(at, &format!("{empty_prefix}~"))?;
        Ok(())
    }

    fn render_line(at: usize, line_text: &str) -> Result<(), Error> {
        Terminal::print_row(at, line_text)
    }

    pub fn caret_position(&self) -> Position {
        let Position { col, row } = self
            .text_location_to_position()
            .saturating_sub(self.scroll_offset);

        let gutter_total = Self::GUTTER_WIDTH + Self::GUTTER_PADDING;
        Position {
            col: col + gutter_total,
            row,
        }
    }

    fn text_location_to_position(&self) -> Position {
        let row = self.text_location.line_idx;
        let col = self.buffer.lines.get(row).map_or(0, |line| {
            // Note:
            // grapheme_idx is "before the grapheme at that index"
            // The caret position should also be "before the grapheme at that index"
            // Therefore, we use width_until(grapheme_idx) to get the width up to that position
            line.width_until(self.text_location.grapheme_idx)
        });
        Position { col, row }
    }

    pub fn handle_move_command(&mut self, move_cmd: Move) {
        let Size { height, .. } = self.size;

        let old_line_idx = self.text_location.line_idx;
        let old_scroll_row = self.scroll_offset.row;

        match move_cmd.direction {
            MoveDirection::ScrollUp => {
                self.scroll_only_up();
                return;
            }
            MoveDirection::ScrollDown => {
                self.scroll_only_down();
                return;
            }
            _ => {}
        }

        if move_cmd.is_selection {
            if self.selection.is_none() {
                self.start_selection();
            }
        } else {
            self.clear_selection();
        }

        match move_cmd.direction {
            MoveDirection::Up => self.move_up(1),
            MoveDirection::Down => self.move_down(1),
            MoveDirection::Left => self.move_left(),
            MoveDirection::Right => self.move_right(),
            MoveDirection::PageUp => self.move_up(height.saturating_sub(1)),
            MoveDirection::PageDown => self.move_down(height.saturating_sub(1)),
            MoveDirection::WordLeft => self.move_to_prev_word_start(),
            MoveDirection::WordRight => self.move_to_next_word_start(),
            MoveDirection::LineStart => self.move_to_start_of_line(),
            MoveDirection::LineEnd => self.move_to_end_of_line(),
            MoveDirection::DocumentStart => self.move_to_start_of_document(),
            MoveDirection::DocumentEnd => self.move_to_end_of_document(),
            MoveDirection::ScrollUp | MoveDirection::ScrollDown => {}
        }

        if move_cmd.is_selection {
            self.extend_selection();
        }

        self.scroll_text_location_into_view();
        // Note:
        // Redraw conditions:
        // - Selection operation (selection range changes)
        // - Line changed (current line highlight or cursor line content changes)
        // - Scroll position changed (displayed lines change)
        let line_changed = self.text_location.line_idx != old_line_idx;
        let scroll_changed = self.scroll_offset.row != old_scroll_row;

        if move_cmd.is_selection || line_changed || scroll_changed {
            self.mark_redraw(true);
        }
    }

    fn move_up(&mut self, step: usize) {
        self.text_location.line_idx = self.text_location.line_idx.saturating_sub(step);
        self.snap_to_valid_grapheme();
    }

    fn move_down(&mut self, step: usize) {
        self.text_location.line_idx = self.text_location.line_idx.saturating_add(step);
        self.snap_to_valid_grapheme();
        self.snap_to_valid_line();
    }

    fn move_right(&mut self) {
        let line_width = self
            .buffer
            .lines
            .get(self.text_location.line_idx)
            .map_or(0, Line::grapheme_count);
        if self.text_location.grapheme_idx < line_width {
            self.text_location.grapheme_idx += 1;
        } else {
            self.move_to_start_of_line();
            self.move_down(1);
        }
    }

    fn move_left(&mut self) {
        if self.text_location.grapheme_idx > 0 {
            self.text_location.grapheme_idx -= 1;
        } else if self.text_location.line_idx > 0 {
            self.move_up(1);
            self.move_to_end_of_line();
        }
    }

    fn move_to_start_of_line(&mut self) {
        self.text_location.grapheme_idx = 0;
    }

    fn move_to_end_of_line(&mut self) {
        self.text_location.grapheme_idx = self
            .buffer
            .lines
            .get(self.text_location.line_idx)
            .map_or(0, Line::grapheme_count);
    }

    fn move_to_start_of_document(&mut self) {
        self.text_location.line_idx = 0;
        self.text_location.grapheme_idx = 0;
    }

    fn move_to_end_of_document(&mut self) {
        let last_line_idx = self.buffer.height().saturating_sub(1);
        self.text_location.line_idx = last_line_idx;
        self.text_location.grapheme_idx = self
            .buffer
            .lines
            .get(last_line_idx)
            .map_or(0, Line::grapheme_count);
    }

    fn move_to_prev_word_start(&mut self) {
        let line_idx = self.text_location.line_idx;
        let grapheme_idx = self.text_location.grapheme_idx;
        if let Some(line) = self.buffer.lines.get(line_idx)
            && let Some(idx) = line.prev_word_start(grapheme_idx)
        {
            self.text_location.grapheme_idx = idx;
            return;
        }
        if self.text_location.line_idx > 0 {
            self.move_up(1);
            self.move_to_end_of_line();
            if let Some(line) = self.buffer.lines.get(self.text_location.line_idx) {
                let end = line.grapheme_count();
                self.text_location.grapheme_idx = line.prev_word_start(end).unwrap_or(0);
            }
        }
    }

    fn move_to_next_word_start(&mut self) {
        let line_idx = self.text_location.line_idx;
        let grapheme_idx = self.text_location.grapheme_idx;
        if let Some(line) = self.buffer.lines.get(line_idx)
            && let Some(idx) = line.next_word_end(grapheme_idx)
        {
            self.text_location.grapheme_idx = idx;
            return;
        }
        if self.text_location.line_idx < self.buffer.height().saturating_sub(1) {
            self.move_down(1);
            self.move_to_start_of_line();
            if let Some(line) = self.buffer.lines.get(self.text_location.line_idx) {
                self.text_location.grapheme_idx = line.next_word_end(0).unwrap_or(0);
            }
        }
    }

    fn snap_to_valid_grapheme(&mut self) {
        self.text_location.grapheme_idx = self
            .buffer
            .lines
            .get(self.text_location.line_idx)
            .map_or(0, |line| {
                min(line.grapheme_count(), self.text_location.grapheme_idx)
            });
    }

    fn snap_to_valid_line(&mut self) {
        let max_line = self.buffer.height().saturating_sub(1);
        self.text_location.line_idx = min(self.text_location.line_idx, max_line);
    }

    fn scroll_vertically(&mut self, to: usize) {
        let Size { height, .. } = self.size;
        let offset_changed = if to < self.scroll_offset.row {
            self.scroll_offset.row = to;
            true
        } else if to >= self.scroll_offset.row.saturating_add(height) {
            self.scroll_offset.row = to.saturating_sub(height).saturating_add(1);
            true
        } else {
            false
        };

        if offset_changed {
            self.mark_redraw(true);
        }
    }

    fn scroll_horizontally(&mut self, to: usize) {
        let content_width = self
            .size
            .width
            .saturating_sub(Self::GUTTER_WIDTH + Self::GUTTER_PADDING);
        let offset_changed = if to < self.scroll_offset.col {
            self.scroll_offset.col = to;
            true
        } else if to >= self.scroll_offset.col.saturating_add(content_width) {
            self.scroll_offset.col = to.saturating_sub(content_width).saturating_add(1);
            true
        } else {
            false
        };

        if offset_changed {
            self.mark_redraw(true);
        }
    }

    fn scroll_text_location_into_view(&mut self) {
        let Position { row, col } = self.text_location_to_position();
        self.scroll_vertically(row);
        self.scroll_horizontally(col);
    }

    fn scroll_only_up(&mut self) {
        if self.scroll_offset.row > 0 {
            self.scroll_offset.row -= 1;
            self.mark_redraw(true);
        }
    }

    fn scroll_only_down(&mut self) {
        let Size { height, .. } = self.size;
        let max_row = self.buffer.height().saturating_sub(height);
        if self.scroll_offset.row < max_row {
            self.scroll_offset.row += 1;
            self.mark_redraw(true);
        }
    }

    fn start_selection(&mut self) {
        self.selection = Some(Selection::new(self.text_location, self.text_location));
        self.mark_redraw(true);
    }

    fn extend_selection(&mut self) {
        if let Some(selection) = &mut self.selection {
            selection.end = self.text_location;
            self.mark_redraw(true);
        } else {
            self.start_selection();
        }
    }

    pub fn clear_selection(&mut self) {
        if self.selection.is_some() {
            self.selection = None;
            self.mark_redraw(true);
        }
    }

    /// Returns the byte range of the selection on the given line, using that exact line
    /// for grapheme→byte conversion. Must be called with the same `line` reference
    /// that is later passed to `get_annotated_visible_substr`, so that coordinates match.
    fn selection_byte_range_for_line(
        sel: Selection,
        line: &Line,
        line_idx: usize,
    ) -> Option<std::ops::Range<usize>> {
        let n = sel.normalize();
        if n.is_empty() {
            return None;
        }
        if line_idx < n.start.line_idx || line_idx > n.end.line_idx {
            return None;
        }
        let start_byte = if line_idx == n.start.line_idx {
            line.grapheme_to_byte_idx(n.start.grapheme_idx)
        } else {
            0
        };
        let end_byte = if line_idx == n.end.line_idx {
            line.grapheme_to_byte_idx(n.end.grapheme_idx)
        } else {
            line.line_length()
        };
        if start_byte < end_byte {
            Some(start_byte..end_byte)
        } else {
            None
        }
    }

    fn delete_selection(&mut self) -> bool {
        let Some(selection) = self.selection else {
            return false;
        };

        let normalized = selection.normalize();
        if normalized.is_empty() {
            return false;
        }

        let at = normalized.start;
        let deleted_text = self.selection_to_string(&normalized);

        self.buffer.delete_range(normalized.start, normalized.end);

        self.text_location = at;
        self.selection = None;
        if self.buffer.is_file_loaded() {
            self.undo_history.clear_redo();
            if let Some(text) = deleted_text {
                self.undo_history.push_edit(EditOp::Delete { at, text });
            }
        }
        self.buffer.modified = true;
        self.cache_version += 1;
        self.mark_redraw(true);

        true
    }

    fn selection_to_string(&self, selection: &Selection) -> Option<String> {
        let ranges = selection.get_ranges(&self.buffer);
        if ranges.is_empty() {
            return None;
        }

        let mut result = String::new();
        for (idx, (line_idx, byte_range)) in ranges.iter().enumerate() {
            if let Some(line) = self.buffer.lines.get(*line_idx) {
                let slice = &line[byte_range.clone()];
                result.push_str(slice);
                if idx + 1 < ranges.len() {
                    result.push('\n');
                }
            }
        }

        Some(result)
    }

    fn copy_selection(&mut self) -> Result<(), &'static str> {
        let Some(selection) = self.selection else {
            return Ok(());
        };
        let Some(text) = self.selection_to_string(&selection) else {
            return Ok(());
        };
        Clipboard::new()
            .map_err(|_| "Clipboard unavailable")?
            .set_text(text)
            .map_err(|_| "Failed to copy to clipboard")
    }

    fn cut_selection(&mut self) -> Result<(), &'static str> {
        let result = self.copy_selection();
        let _ = self.delete_selection();
        result
    }

    /// Inserts the given text at the current cursor (or replaces selection).
    /// Used by both Ctrl+V paste and bracketed paste (`Event::Paste`).
    pub fn paste_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }

        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let _ = self.delete_selection();

        if self.buffer.is_file_loaded() {
            self.undo_history.clear_redo();
            let at = self.text_location;
            let new_loc = self.buffer.insert_string(at, &text);
            self.text_location = new_loc;
            self.undo_history.push_edit(EditOp::Insert {
                at,
                text: text.to_string(),
                cursor_after: new_loc,
            });
        } else {
            for (idx, line) in text.split('\n').enumerate() {
                if idx > 0 {
                    self.insert_newline();
                }
                for ch in line.chars() {
                    self.insert_char(ch);
                }
            }
        }
        self.cache_version += 1;
        self.mark_redraw(true);
    }

    fn paste_clipboard(&mut self) -> Result<(), &'static str> {
        let text = Clipboard::new()
            .map_err(|_| "Clipboard unavailable")?
            .get_text()
            .map_err(|_| "Failed to paste from clipboard")?;
        self.paste_text(&text);
        Ok(())
    }

    /// Returns an error message string if a clipboard operation failed, otherwise `None`.
    pub fn handle_edit_command(&mut self, command: Edit) -> Option<&'static str> {
        match command {
            Edit::Insert(character) => self.insert_char(character),
            Edit::InsertNewline => self.insert_newline(),
            Edit::Backspace => self.backspace(),
            Edit::Delete => self.delete(),
            Edit::Copy => return self.copy_selection().err(),
            Edit::Cut => return self.cut_selection().err(),
            Edit::Paste => return self.paste_clipboard().err(),
            Edit::SelectAll => self.select_all(),
            Edit::Undo => self.undo(),
            Edit::Redo => self.redo(),
            Edit::IndentLine => self.indent_selection_or_line(),
            Edit::DedentLine => self.dedent_selection_or_line(),
        }
        None
    }

    pub fn undo(&mut self) {
        if !self.buffer.is_file_loaded() || !self.undo_history.has_undo() {
            return;
        }
        let Some(op) = self.undo_history.pop_undo() else {
            return;
        };
        self.selection = None;
        match &op {
            EditOp::Insert { at, text, .. } => {
                self.buffer.delete_span(*at, text);
                self.text_location = *at;
            }
            EditOp::Delete { at, text } => {
                self.text_location = self.buffer.insert_string(*at, text);
            }
            EditOp::Group(ops) => {
                for sub in ops.iter().rev() {
                    match sub {
                        EditOp::Insert { at, text, .. } => {
                            self.buffer.delete_span(*at, text);
                            self.text_location = *at;
                        }
                        EditOp::Delete { at, text } => {
                            self.text_location = self.buffer.insert_string(*at, text);
                        }
                        EditOp::Group(_) => {}
                    }
                }
            }
        }
        self.undo_history.push_redo(op);
        self.buffer.modified = true;
        self.cache_version += 1;
        self.mark_redraw(true);
    }

    pub fn redo(&mut self) {
        if !self.buffer.is_file_loaded() || !self.undo_history.has_redo() {
            return;
        }
        let Some(op) = self.undo_history.pop_redo() else {
            return;
        };
        self.selection = None;
        match &op {
            EditOp::Insert { cursor_after, at, text } => {
                self.buffer.insert_string(*at, text);
                self.text_location = *cursor_after;
            }
            EditOp::Delete { at, text } => {
                self.buffer.delete_span(*at, text);
                self.text_location = *at;
            }
            EditOp::Group(ops) => {
                for sub in ops {
                    match sub {
                        EditOp::Insert { cursor_after, at, text } => {
                            self.buffer.insert_string(*at, text);
                            self.text_location = *cursor_after;
                        }
                        EditOp::Delete { at, text } => {
                            self.buffer.delete_span(*at, text);
                            self.text_location = *at;
                        }
                        EditOp::Group(_) => {}
                    }
                }
            }
        }
        self.undo_history.push_edit(op);
        self.buffer.modified = true;
        self.cache_version += 1;
        self.mark_redraw(true);
    }

    fn select_all(&mut self) {
        if self.buffer.is_empty() {
            return;
        }
        let last_line = self.buffer.height().saturating_sub(1);
        self.selection = Some(Selection::new(
            Location {
                line_idx: 0,
                grapheme_idx: 0,
            },
            Location {
                line_idx: last_line,
                grapheme_idx: self
                    .buffer
                    .lines
                    .get(last_line)
                    .map_or(0, Line::grapheme_count),
            },
        ));
        self.mark_redraw(true);
    }

    const INDENT: &'static str = "    "; // 4 spaces; TODO: make configurable

    fn indent_selection_or_line(&mut self) {
        if !self.buffer.is_file_loaded() {
            return;
        }
        let line_range = self.selected_line_range();
        self.undo_history.clear_redo();

        let mut ops: Vec<EditOp> = Vec::new();
        for line_idx in line_range {
            let at = Location { grapheme_idx: 0, line_idx };
            let cursor_after = self.buffer.insert_string(at, Self::INDENT);
            ops.push(EditOp::Insert { at, text: Self::INDENT.to_string(), cursor_after });
        }

        let op = if ops.len() == 1 { ops.remove(0) } else { EditOp::Group(ops) };
        self.undo_history.push_edit(op);

        // Shift cursor and selection right by indent width.
        self.text_location.grapheme_idx =
            self.text_location.grapheme_idx.saturating_add(Self::INDENT.len());
        if let Some(sel) = &mut self.selection {
            sel.start.grapheme_idx = sel.start.grapheme_idx.saturating_add(Self::INDENT.len());
            sel.end.grapheme_idx = sel.end.grapheme_idx.saturating_add(Self::INDENT.len());
        }
        self.buffer.modified = true;
        self.cache_version += 1;
        self.mark_redraw(true);
    }

    fn dedent_selection_or_line(&mut self) {
        if !self.buffer.is_file_loaded() {
            return;
        }
        let line_range = self.selected_line_range();
        self.undo_history.clear_redo();

        let mut ops: Vec<EditOp> = Vec::new();
        for line_idx in line_range {
            let leading = self
                .buffer
                .lines
                .get(line_idx)
                .map_or("", Line::leading_whitespace);
            // Remove up to INDENT_WIDTH spaces from the start.
            let remove_count = leading
                .chars()
                .take(Self::INDENT.len())
                .take_while(|c| *c == ' ')
                .count();
            if remove_count == 0 {
                continue;
            }
            let removed: String = " ".repeat(remove_count);
            let at = Location { grapheme_idx: 0, line_idx };
            self.buffer.delete_span(at, &removed);
            ops.push(EditOp::Delete { at, text: removed });
        }

        if ops.is_empty() {
            return;
        }
        // Pull cursor left by however many spaces were removed on its line.
        let cursor_line_removed = ops.iter().find_map(|op| {
            if let EditOp::Delete { at, text } = op
                && at.line_idx == self.text_location.line_idx
            {
                return Some(text.len());
            }
            None
        }).unwrap_or(0);
        self.text_location.grapheme_idx =
            self.text_location.grapheme_idx.saturating_sub(cursor_line_removed);

        let op = if ops.len() == 1 { ops.remove(0) } else { EditOp::Group(ops) };
        self.undo_history.push_edit(op);

        self.buffer.modified = true;
        self.cache_version += 1;
        self.mark_redraw(true);
    }

    /// Returns the range of line indices that should be affected by indent/dedent.
    /// If a selection spans multiple lines, returns that range; otherwise the cursor's line.
    fn selected_line_range(&self) -> std::ops::RangeInclusive<usize> {
        if let Some(sel) = self.selection {
            let norm = sel.normalize();
            norm.start.line_idx..=norm.end.line_idx
        } else {
            let l = self.text_location.line_idx;
            l..=l
        }
    }

    fn insert_char(&mut self, character: char) {
        // Selection wrapping: if text is selected and the character is an
        // opening bracket, wrap the selection instead of replacing it.
        if self.selection.is_some()
            && let Some(close) = closing_bracket_for(character)
        {
            self.wrap_selection_with(character, close);
            return;
        }

        let _ = self.delete_selection();

        // Closing bracket skip: if the cursor is already sitting before the
        // same closing bracket, just move the cursor past it.
        if is_closing_bracket(character) {
            let loc = self.text_location;
            let char_ahead = self
                .buffer
                .lines
                .get(loc.line_idx)
                .and_then(|l| l.grapheme_at(loc.grapheme_idx))
                .and_then(|s| s.chars().next());
            if char_ahead == Some(character) {
                self.handle_move_command(Move::right(false));
                return;
            }
        }

        // Auto-close: insert both the opening and closing bracket as one unit.
        let text_to_insert = if let Some(close) = closing_bracket_for(character) {
            format!("{character}{close}")
        } else {
            character.to_string()
        };

        let old_len = self
            .buffer
            .lines
            .get(self.text_location.line_idx)
            .map_or(0, Line::grapheme_count);

        if self.buffer.is_file_loaded() {
            self.undo_history.clear_redo();
            let at = self.text_location;
            self.buffer.insert_string(at, &text_to_insert);
            // For auto-close pairs, cursor sits between the pair (one step right of `at`).
            // For normal single-char insertion, that is also one step right of `at`.
            let cursor_after = Location {
                grapheme_idx: at.grapheme_idx + 1,
                line_idx: at.line_idx,
            };
            self.undo_history.push_edit(EditOp::Insert {
                at,
                text: text_to_insert,
                cursor_after,
            });
        } else {
            self.buffer.insert_char(character, self.text_location);
        }

        let new_len = self
            .buffer
            .lines
            .get(self.text_location.line_idx)
            .map_or(0, Line::grapheme_count);
        if new_len > old_len {
            self.handle_move_command(Move::right(false));
        }
        self.cache_version += 1;
        self.mark_redraw(true);
    }

    /// Wraps the current selection with `open` and `close` characters.
    /// The entire operation is recorded as two `EditOp`s (Delete + Insert) for atomic undo.
    fn wrap_selection_with(&mut self, open: char, close: char) {
        let Some(selection) = self.selection else {
            return;
        };
        let normalized = selection.normalize();
        let Some(selected_text) = self.selection_to_string(&normalized) else {
            return;
        };

        let at = normalized.start;
        let wrapped = format!("{open}{selected_text}{close}");

        // Remove the selection first, then insert the wrapped version.
        // The two ops on the undo stack will be undone in LIFO order:
        // 1) undo the Insert (removes wrapped text, cursor back to `at`)
        // 2) undo the Delete (restores selected text)
        let _ = self.delete_selection(); // pushes a Delete op
        if self.buffer.is_file_loaded() {
            let cursor_after = self.buffer.insert_string(at, &wrapped);
            self.text_location = cursor_after;
            self.undo_history.push_edit(EditOp::Insert {
                at,
                text: wrapped,
                cursor_after,
            });
        }
        self.cache_version += 1;
        self.mark_redraw(true);
    }

    fn insert_newline(&mut self) {
        let _ = self.delete_selection();

        let indent = self
            .buffer
            .lines
            .get(self.text_location.line_idx)
            .map(|line| line.leading_whitespace().to_string())
            .unwrap_or_default();
        let insert_text = format!("\n{indent}");

        if self.buffer.is_file_loaded() {
            self.undo_history.clear_redo();
            let at = self.text_location;
            let cursor_after = self.buffer.insert_string(at, &insert_text);
            self.text_location = cursor_after;
            self.undo_history.push_edit(EditOp::Insert {
                at,
                text: insert_text,
                cursor_after,
            });
        } else {
            self.buffer.insert_newline(self.text_location);
            self.text_location = Location {
                line_idx: self.text_location.line_idx + 1,
                grapheme_idx: 0,
            };
        }
        self.scroll_text_location_into_view();
        self.cache_version += 1;
        self.mark_redraw(true);
    }

    pub fn load(&mut self, file_name: &str) -> Result<(), Error> {
        let buffer = Buffer::load(file_name)?;
        self.buffer = buffer;
        self.undo_history.clear_all();
        self.highlight_cache.clear();
        self.cache_version += 1;
        self.mark_redraw(true);
        Ok(())
    }

    fn backspace(&mut self) {
        if self.delete_selection() {
            return;
        }

        if self.text_location.line_idx != 0 || self.text_location.grapheme_idx != 0 {
            if self.buffer.is_file_loaded() {
                self.undo_history.clear_redo();
            }
            self.handle_move_command(Move::left(false));
            let at = self.text_location;
            let text = self
                .buffer
                .content_deleted_at(at)
                .unwrap_or_default();
            self.buffer.delete(at);
            if self.buffer.is_file_loaded() && !text.is_empty() {
                self.undo_history.push_edit(EditOp::Delete { at, text });
            }
            self.cache_version += 1;
            self.mark_redraw(true);
        }
    }

    fn delete(&mut self) {
        if self.delete_selection() {
            return;
        }

        let at = self.text_location;
        let text = self
            .buffer
            .content_deleted_at(at)
            .unwrap_or_default();
        self.buffer.delete(self.text_location);
        if self.buffer.is_file_loaded() && !text.is_empty() {
            self.undo_history.clear_redo();
            self.undo_history.push_edit(EditOp::Delete { at, text });
        }
        self.cache_version += 1;
        self.mark_redraw(true);
    }

    pub fn save(&mut self) -> Result<(), Error> {
        self.buffer.save()
    }

    pub const fn is_file_loaded(&self) -> bool {
        self.buffer.is_file_loaded()
    }

    pub fn save_as(&mut self, file_name: &str) -> Result<(), Error> {
        self.buffer.save_as(file_name)
    }

    pub fn enter_search(&mut self) {
        self.search_info = Some(SearchInfo {
            prev_location: self.text_location,
            prev_scroll_offset: self.scroll_offset,
            query: None,
        });
    }

    pub fn exit_search(&mut self) {
        self.search_info = None;
        self.mark_redraw(true);
    }

    pub fn dismiss_search(&mut self) {
        if let Some(search_info) = &self.search_info {
            self.text_location = search_info.prev_location;
            self.scroll_offset = search_info.prev_scroll_offset;
            self.scroll_text_location_into_view();
        }
        self.search_info = None;
        self.mark_redraw(true);
    }

    /// Replaces all occurrences of `query` with `replacement`.
    /// Returns the number of replacements made.
    /// Note: this operation is not recorded in the undo history.
    pub fn replace_all(&mut self, query: &str, replacement: &str) -> usize {
        if query.is_empty() || !self.buffer.is_file_loaded() {
            return 0;
        }
        let matches = self.buffer.find_all_matches(query);
        let count = matches.len();
        if count == 0 {
            return 0;
        }
        self.undo_history.clear_redo();
        // Replace from end to start to preserve indices of earlier matches.
        for &at in matches.iter().rev() {
            self.buffer.delete_span(at, query);
            self.buffer.insert_string(at, replacement);
        }
        self.cache_version += 1;
        self.mark_redraw(true);
        count
    }

    pub fn search(&mut self, query: &str) {
        if let Some(search_info) = &mut self.search_info {
            search_info.query = Some(Line::from(query));
        }
        self.search_in_direction(self.text_location, SearchDirection::default());
    }

    fn get_search_query(&self) -> Option<&Line> {
        let query = self
            .search_info
            .as_ref()
            .and_then(|search_info| search_info.query.as_ref());

        debug_assert!(
            query.is_some(),
            "Attempting to search with malformed searchinfo present"
        );
        query
    }

    fn search_in_direction(&mut self, from: Location, direction: SearchDirection) {
        if let Some(location) = self.get_search_query().and_then(|query| {
            if query.is_empty() {
                None
            } else if direction == SearchDirection::Forward {
                self.buffer.search_forward(query, from)
            } else {
                self.buffer.search_backward(query, from)
            }
        }) {
            self.text_location = location;
            self.center_text_location();
        }
        self.mark_redraw(true);
    }

    pub fn search_next(&mut self) {
        let step_right = self
            .get_search_query()
            .map_or(1, |query| min(query.grapheme_count(), 1));

        let location = Location {
            line_idx: self.text_location.line_idx,
            grapheme_idx: self.text_location.grapheme_idx.saturating_add(step_right),
        };
        self.search_in_direction(location, SearchDirection::Forward);
    }

    pub fn search_prev(&mut self) {
        self.search_in_direction(self.text_location, SearchDirection::Backward);
    }

    fn center_text_location(&mut self) {
        let Size { height, width } = self.size;
        let content_width = width.saturating_sub(Self::GUTTER_WIDTH + Self::GUTTER_PADDING);
        let Position { row, col } = self.text_location_to_position();
        let vertical_mid = height.div_ceil(2);
        let horizontal_mid = content_width.div_ceil(2);
        self.scroll_offset.row = row.saturating_sub(vertical_mid);
        self.scroll_offset.col = col.saturating_sub(horizontal_mid);
        self.mark_redraw(true);
    }
}

impl UIComponent for View {
    fn mark_redraw(&mut self, value: bool) {
        self.needs_redraw = value;
    }

    fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }
    fn set_size(&mut self, size: Size) {
        self.size = size;
        self.scroll_text_location_into_view();
    }

    fn draw(&mut self, origin_y: usize) -> Result<(), Error> {
        if self.buffer.is_empty() && !self.buffer.is_file_loaded() {
            self.render_welcome_screen(origin_y)
        } else {
            self.render_buffer(origin_y)
        }
    }
}
