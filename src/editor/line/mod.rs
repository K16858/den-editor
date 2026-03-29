use std::{
    cmp::min,
    fmt,
    ops::{Deref, Range},
};

mod grapheme_width;
mod text_fragment;
use grapheme_width::GraphemeWidth;
use text_fragment::TextFragment;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::highlight::{HighlightAnnotation, Highlighter};
use super::{AnnotatedString, AnnotationType};
use crate::editor::highlight::HighlightState;

/// Parameters for `get_annotated_visible_substr` to avoid too many arguments.
pub struct GetAnnotatedVisibleSubstrParams<'a> {
    pub range: Range<usize>,
    pub query: Option<&'a str>,
    pub selected_match: Option<usize>,
    pub highlighter: Option<&'a dyn Highlighter>,
    pub state: HighlightState,
    pub cached_annotations: Option<&'a [HighlightAnnotation]>,
    pub selection_range: Option<Range<usize>>,
}

#[derive(Default, Clone)]
pub struct Line {
    fragments: Vec<TextFragment>,
    string: String,
}

impl Line {
    pub fn from(line_str: &str) -> Self {
        let fragments = Self::str_to_fragments(line_str);
        Self {
            fragments,
            string: String::from(line_str),
        }
    }

    fn str_to_fragments(line_str: &str) -> Vec<TextFragment> {
        line_str
            .grapheme_indices(true)
            .map(|(byte_idx, grapheme)| {
                let (replacement, rendered_width) = Self::get_replacement_character(grapheme)
                    .map_or_else(
                        || {
                            // Note: width_cjk is used to get the width of the string in CJK.
                            // This is because of Ambiguous Width problem.
                            let unicode_width = grapheme.width_cjk();
                            let rendered_width = match unicode_width {
                                0 | 1 => GraphemeWidth::Half,
                                _ => GraphemeWidth::Full,
                            };
                            (None, rendered_width)
                        },
                        |replacement| {
                            let rendered_width = if grapheme.width_cjk() > 1 {
                                GraphemeWidth::Full
                            } else {
                                GraphemeWidth::Half
                            };
                            (Some(replacement), rendered_width)
                        },
                    );

                TextFragment {
                    grapheme: grapheme.to_string(),
                    rendered_width,
                    replacement,
                    start: byte_idx,
                }
            })
            .collect()
    }

    fn rebuild_fragments(&mut self) {
        self.fragments = Self::str_to_fragments(&self.string);
    }

    fn get_replacement_character(for_str: &str) -> Option<char> {
        // Note: width_cjk is used to get the width of the string in CJK.
        // This is because of Ambiguous Width problem.
        let width = for_str.width_cjk();
        match for_str {
            " " => None,
            "\t" => Some(' '),
            _ if width > 0 && for_str.trim().is_empty() => Some('␣'),
            _ if width == 0 => {
                let mut chars = for_str.chars();
                if let Some(ch) = chars.next()
                    && ch.is_control()
                    && chars.next().is_none()
                {
                    return Some('▯');
                }
                Some('·')
            }
            _ => None,
        }
    }

    pub fn get_visible_graphemes(&self, range: Range<usize>) -> String {
        self.get_annotated_visible_substr(GetAnnotatedVisibleSubstrParams {
            range,
            query: None,
            selected_match: None,
            highlighter: None,
            state: HighlightState::default(),
            cached_annotations: None,
            selection_range: None,
        })
        .0
        .to_string()
    }

    pub fn grapheme_count(&self) -> usize {
        self.fragments.len()
    }

    /// Returns the leading whitespace (spaces and tabs) of the line as a string slice.
    pub fn leading_whitespace(&self) -> &str {
        let end = self
            .string
            .find(|c: char| c != ' ' && c != '\t')
            .unwrap_or(self.string.len());
        &self.string[..end]
    }

    /// Returns the grapheme at the given index, for undo/redo recording.
    pub fn grapheme_at(&self, grapheme_idx: usize) -> Option<String> {
        self.fragments.get(grapheme_idx).map(|f| f.grapheme.clone())
    }

    pub fn get_annotated_visible_substr(
        &self,
        params: GetAnnotatedVisibleSubstrParams<'_>,
    ) -> (AnnotatedString, HighlightState) {
        let GetAnnotatedVisibleSubstrParams {
            range,
            query,
            selected_match,
            highlighter,
            state,
            cached_annotations,
            selection_range,
        } = params;
        if range.start >= range.end {
            return (AnnotatedString::default(), state);
        }

        let mut result = AnnotatedString::from(&self.string);
        let mut new_state = state;
        if let Some(cached) = cached_annotations {
            for highlight in cached {
                result.add_annotation(highlight.annotation_type, highlight.start, highlight.end);
            }
        } else if let Some(hl) = highlighter {
            let (highlights, updated_state) = hl.highlight_line(&self.string, 0, state);
            new_state = updated_state;
            for highlight in highlights {
                result.add_annotation(highlight.annotation_type, highlight.start, highlight.end);
            }
        }

        if let Some(query) = query
            && !query.is_empty()
        {
            self.find_all(query, 0..self.string.len())
                .iter()
                .for_each(|(start, grapheme_idx)| {
                    if let Some(selected_match) = selected_match
                        && *grapheme_idx == selected_match
                    {
                        result.add_annotation(
                            AnnotationType::SelectedMatch,
                            *start,
                            start.saturating_add(query.len()),
                        );
                        return;
                    }
                    result.add_annotation(
                        AnnotationType::Match,
                        *start,
                        start.saturating_add(query.len()),
                    );
                });
        }

        if let Some(sel_range) = selection_range {
            let end = min(sel_range.end, self.string.len());
            if sel_range.start < end {
                result.add_annotation(AnnotationType::Selection, sel_range.start, end);
            }
        }

        let byte_start = self.display_width_to_byte_pos(range.start);
        let byte_end = self.display_width_to_byte_pos(range.end);

        let _left_truncated_bytes = if byte_start > 0 {
            result.truncate_left_until(byte_start);
            byte_start
        } else {
            0
        };
        if byte_end < self.string.len() {
            result.truncate_right_from(byte_end);
        }

        for fragment in &self.fragments {
            if fragment.start >= byte_end
                || fragment.start.saturating_add(fragment.grapheme.len()) <= byte_start
            {
                continue;
            }

            if let Some(replacement) = fragment.replacement {
                let start = fragment.start.saturating_sub(byte_start);
                let end = start.saturating_add(fragment.grapheme.len());
                let result_len = result.to_string().len();
                if start < result_len && end <= result_len {
                    result.replace(start, end, &replacement.to_string());
                }
            }
        }

        (result, new_state)
    }

    pub fn width_until(&self, grapheme_idx: usize) -> usize {
        self.fragments
            .iter()
            .take(grapheme_idx)
            .map(|fragment| match fragment.rendered_width {
                GraphemeWidth::Half => 1,
                GraphemeWidth::Full => 2,
            })
            .sum()
    }

    pub fn display_width_to_byte_pos(&self, display_width: usize) -> usize {
        let mut current_width = 0;
        for fragment in &self.fragments {
            let fragment_width: usize = fragment.rendered_width.into();
            // Note:
            // If the display_width falls within this fragment (including partial full-width characters),
            // return the fragment's start byte position.
            if current_width + fragment_width > display_width {
                return fragment.start;
            }
            current_width += fragment_width;
            // Note:
            // If the display_width exactly matches the end of this fragment,
            // return the byte position after this fragment (start of next fragment).
            if current_width == display_width {
                return fragment.start.saturating_add(fragment.grapheme.len());
            }
        }
        self.string.len()
    }

    pub fn insert_char(&mut self, character: char, at: usize) {
        if let Some(fragment) = self.fragments.get(at) {
            self.string.insert(fragment.start, character);
        } else {
            self.string.push(character);
        }

        self.rebuild_fragments();
    }

    pub fn width(&self) -> usize {
        self.width_until(self.grapheme_count())
    }

    pub fn append_char(&mut self, character: char) {
        self.insert_char(character, self.grapheme_count());
    }

    pub fn delete_last(&mut self) {
        self.delete(self.grapheme_count().saturating_sub(1));
    }

    pub fn delete(&mut self, at: usize) {
        if let Some(fragment) = self.fragments.get(at) {
            let start = fragment.start;
            let end = fragment.start.saturating_add(fragment.grapheme.len());
            self.string.drain(start..end);
            self.rebuild_fragments();
        }
    }

    pub fn delete_byte_range(&mut self, range: Range<usize>) {
        if range.is_empty() {
            return;
        }

        let start = min(range.start, self.string.len());
        let end = min(range.end, self.string.len());

        if start >= end {
            return;
        }

        self.string.drain(start..end);
        self.rebuild_fragments();
    }

    pub fn append(&mut self, other: &Self) {
        self.string.push_str(&other.string);
        self.rebuild_fragments();
    }

    pub fn split(&mut self, at: usize) -> Self {
        if let Some(fragment) = self.fragments.get(at) {
            let remainder = self.string.split_off(fragment.start);
            self.rebuild_fragments();
            Self::from(&remainder)
        } else {
            Self::default()
        }
    }

    pub fn byte_idx_to_grapheme_idx(&self, byte_idx: usize) -> Option<usize> {
        if byte_idx > self.string.len() {
            return None;
        }
        self.fragments
            .iter()
            .position(|f| byte_idx >= f.start && byte_idx < f.start + f.grapheme.len())
    }

    fn grapheme_idx_to_byte_idx(&self, grapheme_idx: usize) -> usize {
        self.fragments
            .get(grapheme_idx)
            .map_or(0, |fragment| fragment.start)
    }

    pub fn grapheme_to_byte_idx(&self, grapheme_idx: usize) -> usize {
        if grapheme_idx >= self.grapheme_count() {
            self.string.len()
        } else {
            self.grapheme_idx_to_byte_idx(grapheme_idx)
        }
    }

    pub fn line_length(&self) -> usize {
        self.string.len()
    }

    #[allow(dead_code)]
    pub fn is_whitespace_at(&self, grapheme_idx: usize) -> bool {
        self.fragments
            .get(grapheme_idx)
            .is_some_and(|f| f.grapheme.chars().all(char::is_whitespace))
    }

    /// Returns true if the grapheme at `grapheme_idx` is a word delimiter (whitespace, `,`, `;`, newline).
    #[allow(dead_code)]
    pub fn is_word_delimiter_at(&self, grapheme_idx: usize) -> bool {
        self.fragments.get(grapheme_idx).is_some_and(|f| {
            f.grapheme
                .chars()
                .all(|c| c.is_whitespace() || c == ',' || c == ';' || c == '\n' || c == '\r')
        })
    }

    /// Word for Ctrl+Left/Right: identifier (alphanumeric + `_`) or angle-bracket block `<`..`>`.
    fn is_identifier_at(&self, grapheme_idx: usize) -> bool {
        self.fragments
            .get(grapheme_idx)
            .is_some_and(|f| f.grapheme.chars().all(|c| c.is_alphanumeric() || c == '_'))
    }

    fn is_angle_open_at(&self, grapheme_idx: usize) -> bool {
        self.fragments
            .get(grapheme_idx)
            .is_some_and(|f| f.grapheme == "<")
    }

    fn is_angle_close_at(&self, grapheme_idx: usize) -> bool {
        self.fragments
            .get(grapheme_idx)
            .is_some_and(|f| f.grapheme == ">")
    }

    fn is_skip_at(&self, grapheme_idx: usize) -> bool {
        !self.is_identifier_at(grapheme_idx) && !self.is_angle_open_at(grapheme_idx)
    }

    pub fn prev_word_start(&self, grapheme_idx: usize) -> Option<usize> {
        if grapheme_idx == 0 {
            return None;
        }
        let mut idx = grapheme_idx;
        while idx > 0 && !self.is_identifier_at(idx - 1) && !self.is_angle_close_at(idx - 1) {
            idx -= 1;
        }
        if idx == 0 {
            return Some(0);
        }
        idx -= 1;
        if self.is_angle_close_at(idx) {
            while idx > 0 && !self.is_angle_open_at(idx) {
                idx -= 1;
            }
            Some(idx)
        } else {
            while idx > 0 && self.is_identifier_at(idx - 1) {
                idx -= 1;
            }
            Some(idx)
        }
    }

    pub fn next_word_end(&self, grapheme_idx: usize) -> Option<usize> {
        let len = self.grapheme_count();
        let mut idx = grapheme_idx;

        while idx < len && self.is_skip_at(idx) {
            idx += 1;
        }
        if idx >= len {
            return None;
        }
        if self.is_angle_open_at(idx) {
            idx += 1;
            while idx < len && !self.is_angle_close_at(idx) {
                idx += 1;
            }
            if idx < len { Some(idx + 1) } else { None }
        } else {
            while idx < len && self.is_identifier_at(idx) {
                idx += 1;
            }
            Some(idx)
        }
    }

    /// Returns the grapheme index of the start of the next word. Kept for compatibility (e.g. next-line first word).
    #[allow(dead_code)]
    pub fn next_word_start(&self, grapheme_idx: usize) -> Option<usize> {
        let len = self.grapheme_count();
        let mut idx = grapheme_idx;
        while idx < len && self.is_skip_at(idx) {
            idx += 1;
        }
        if idx < len { Some(idx) } else { None }
    }

    pub fn search_forward(&self, query: &str, from_grapheme_idx: usize) -> Option<usize> {
        debug_assert!(from_grapheme_idx <= self.grapheme_count());
        if from_grapheme_idx == self.grapheme_count() {
            return None;
        }
        let start = self.grapheme_idx_to_byte_idx(from_grapheme_idx);
        self.find_all(query, start..self.string.len())
            .first()
            .map(|(_, grapheme_idx)| *grapheme_idx)
    }

    pub fn search_backward(&self, query: &str, from_grapheme_idx: usize) -> Option<usize> {
        debug_assert!(from_grapheme_idx <= self.grapheme_count());

        if from_grapheme_idx == 0 {
            return None;
        }
        let end_byte_index = if from_grapheme_idx == self.grapheme_count() {
            self.string.len()
        } else {
            self.grapheme_idx_to_byte_idx(from_grapheme_idx)
        };
        self.find_all(query, 0..end_byte_index)
            .last()
            .map(|(_, grapheme_idx)| *grapheme_idx)
    }

    fn find_all(&self, query: &str, range: Range<usize>) -> Vec<(usize, usize)> {
        let end = min(range.end, self.string.len());
        let start = range.start;

        debug_assert!(start <= end);
        debug_assert!(start <= self.string.len());

        self.string.get(start..end).map_or_else(Vec::new, |substr| {
            let potential_matches: Vec<usize> = substr
                .match_indices(query)
                .map(|(relative_start_idx, _)| relative_start_idx.saturating_add(start))
                .collect();
            self.match_graphme_clusters(&potential_matches, query)
        })
    }

    fn match_graphme_clusters(&self, matches: &[usize], query: &str) -> Vec<(usize, usize)> {
        let grapheme_count = query.graphemes(true).count();
        matches
            .iter()
            .filter_map(|&start| {
                self.byte_idx_to_grapheme_idx(start)
                    .and_then(|grapheme_idx| {
                        self.fragments
                            .get(grapheme_idx..grapheme_idx.saturating_add(grapheme_count))
                            .and_then(|fragments| {
                                let substring = fragments
                                    .iter()
                                    .map(|fragment| fragment.grapheme.as_str())
                                    .collect::<String>();
                                (substring == query).then_some((start, grapheme_idx))
                            })
                    })
            })
            .collect()
    }
}

impl fmt::Display for Line {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "{}", self.string)
    }
}

impl Deref for Line {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.string
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::highlight::HighlightState;

    #[test]
    fn selection_annotation_applied_for_mid_line_range() {
        let line = Line::from("abcdef");
        let (ann, _) = line.get_annotated_visible_substr(GetAnnotatedVisibleSubstrParams {
            range: 0..80,
            query: None,
            selected_match: None,
            highlighter: None,
            state: HighlightState::default(),
            cached_annotations: None,
            selection_range: Some(2..3),
        });
        let mut found_selection = false;
        for part in &ann {
            if part.annotation_type == Some(AnnotationType::Selection) {
                found_selection = true;
                assert_eq!(part.string, "c");
            }
        }
        assert!(found_selection, "Selection annotation should cover 'c'");
    }
}
