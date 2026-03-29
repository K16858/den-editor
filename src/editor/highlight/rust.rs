use super::Highlighter;
use crate::editor::annotated_string::AnnotationType;
use crate::editor::highlight::{
    HighlightAnnotation, HighlightState, LanguageConfig, default_rust_config, load_language_config,
    merge_config,
};

pub struct RustHighlighter {
    config: LanguageConfig,
}

impl RustHighlighter {
    pub fn new() -> Self {
        let default_config = default_rust_config();
        let merged_config = if let Ok(lang_config) = load_language_config("rust", None) {
            merge_config(&default_config, Some(&lang_config))
        } else {
            default_config
        };

        Self {
            config: merged_config,
        }
    }
}

fn find_string_ranges(string: &str) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    let mut in_double_quote = false;
    let mut in_single_quote = false;
    let mut string_start = 0;
    let mut chars = string.char_indices().peekable();

    while let Some((byte_idx, ch)) = chars.next() {
        if !in_double_quote && !in_single_quote {
            if ch == '"' {
                in_double_quote = true;
                string_start = byte_idx;
            } else if ch == '\'' {
                in_single_quote = true;
                string_start = byte_idx;
            }
        } else {
            if ch == '\\' {
                chars.next();
                continue;
            }
            if ch == '"' && in_double_quote {
                in_double_quote = false;
                ranges.push(string_start..byte_idx + ch.len_utf8());
            } else if ch == '\'' && in_single_quote {
                in_single_quote = false;
                ranges.push(string_start..byte_idx + ch.len_utf8());
            }
        }
    }
    ranges
}

fn find_number_ranges(string: &str) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    let mut i = 0;
    let chars: Vec<_> = string.char_indices().collect();

    while i < chars.len() {
        let (byte_idx, ch) = chars[i];

        if ch == '0' && i + 1 < chars.len() {
            let next_ch = chars[i + 1].1;
            if next_ch == 'x' || next_ch == 'X' {
                let start = byte_idx;
                let mut j = i + 2;
                while j < chars.len() {
                    let digit_ch = chars[j].1;
                    if digit_ch.is_ascii_hexdigit() {
                        j += 1;
                    } else {
                        break;
                    }
                }
                if j > i + 2 {
                    let end_byte = if j < chars.len() { chars[j].0 } else { string.len() };
                    ranges.push(start..end_byte);
                }
                i = j;
                continue;
            } else if next_ch == 'o' || next_ch == 'O' {
                let start = byte_idx;
                let mut j = i + 2;
                while j < chars.len() {
                    let digit_ch = chars[j].1;
                    if ('0'..='7').contains(&digit_ch) {
                        j += 1;
                    } else {
                        break;
                    }
                }
                if j > i + 2 {
                    let end_byte = if j < chars.len() { chars[j].0 } else { string.len() };
                    ranges.push(start..end_byte);
                }
                i = j;
                continue;
            } else if next_ch == 'b' || next_ch == 'B' {
                let start = byte_idx;
                let mut j = i + 2;
                while j < chars.len() {
                    let digit_ch = chars[j].1;
                    if digit_ch == '0' || digit_ch == '1' {
                        j += 1;
                    } else {
                        break;
                    }
                }
                if j > i + 2 {
                    let end_byte = if j < chars.len() { chars[j].0 } else { string.len() };
                    ranges.push(start..end_byte);
                }
                i = j;
                continue;
            }
        }

        if ch.is_ascii_digit() {
            let start = byte_idx;
            let mut j = i;
            let mut has_dot = false;

            while j < chars.len() {
                let digit_ch = chars[j].1;
                if digit_ch.is_ascii_digit() {
                    j += 1;
                } else if digit_ch == '.'
                    && !has_dot
                    && j + 1 < chars.len()
                    && chars[j + 1].1.is_ascii_digit()
                {
                    has_dot = true;
                    j += 1;
                } else {
                    break;
                }
            }
            let end_byte = if j < chars.len() { chars[j].0 } else { string.len() };
            ranges.push(start..end_byte);
            i = j;
        } else {
            i += 1;
        }
    }

    ranges
}

fn find_type_ranges(string: &str) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    let mut in_type = false;
    let mut type_start = 0;

    for (byte_idx, ch) in string.char_indices() {
        if in_type {
            if ch.is_alphanumeric() || ch == '_' {
            } else {
                in_type = false;
                ranges.push(type_start..byte_idx);
            }
        } else if ch.is_uppercase() {
            in_type = true;
            type_start = byte_idx;
        }
    }

    if in_type {
        ranges.push(type_start..string.len());
    }

    ranges
}

impl Highlighter for RustHighlighter {
    #[allow(clippy::too_many_lines)]
    fn highlight_line(
        &self,
        line: &str,
        _line_idx: usize,
        mut state: HighlightState,
    ) -> (Vec<HighlightAnnotation>, HighlightState) {
        let mut annotations = Vec::new();

        for keyword in &self.config.keywords {
            let mut search_start = 0;
            while let Some(rel_pos) = line[search_start..].find(keyword.as_str()) {
                let start = search_start + rel_pos;
                let end = start + keyword.len();

                let is_word_boundary_before = start == 0
                    || !line[..start]
                        .chars()
                        .last()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_');
                let is_word_boundary_after = end >= line.len()
                    || !line[end..]
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_');

                if is_word_boundary_before && is_word_boundary_after {
                    annotations.push(HighlightAnnotation {
                        annotation_type: AnnotationType::Keyword,
                        start,
                        end,
                    });
                }
                search_start = start + 1;
            }
        }
        let string_ranges = find_string_ranges(line);

        for range in &string_ranges {
            annotations.push(HighlightAnnotation {
                annotation_type: AnnotationType::String,
                start: range.start,
                end: range.end,
            });
        }

        let is_in_string = |byte_idx: usize| -> bool {
            string_ranges.iter().any(|range| range.contains(&byte_idx))
        };

        let mut comment_ranges = Vec::new();
        if state.in_block_comment {
            if let Some(close_pos) = line.find(self.config.block_comment_end.as_str()) {
                let close_byte = close_pos + self.config.block_comment_end.len();
                comment_ranges.push(0..close_byte);
                state.in_block_comment = false;
            } else {
                comment_ranges.push(0..line.len());
            }
        } else if let Some(comment_start) = line.find(self.config.line_comment_start.as_str())
        && !is_in_string(comment_start) {
            comment_ranges.push(comment_start..line.len());
        }
        let mut search_start = if state.in_block_comment {
            line.len()
        } else {
            0
        };
        while let Some(rel_pos) =
            line[search_start..].find(self.config.block_comment_start.as_str())
        {
            let open_byte = search_start + rel_pos;
            if is_in_string(open_byte) {
                search_start = open_byte + self.config.block_comment_start.len();
            } else if let Some(rel_close_pos) = line
                [open_byte + self.config.block_comment_start.len()..]
                .find(self.config.block_comment_end.as_str())
            {
                let close_byte = open_byte
                    + self.config.block_comment_start.len()
                    + rel_close_pos
                    + self.config.block_comment_end.len();
                comment_ranges.push(open_byte..close_byte);
                search_start = close_byte;
            } else {
                comment_ranges.push(open_byte..line.len());
                state.in_block_comment = true;
                break;
            }
        }
        let is_in_comment = |byte_idx: usize| -> bool {
            comment_ranges.iter().any(|range| range.contains(&byte_idx))
        };

        for primitive_type in &self.config.primitive_types {
            let mut search_start = 0;
            while let Some(rel_pos) = line[search_start..].find(primitive_type.as_str()) {
                let start = search_start + rel_pos;
                let end = start + primitive_type.len();

                let is_word_boundary_before = start == 0
                    || !line[..start]
                        .chars()
                        .last()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_');
                let is_word_boundary_after = end >= line.len()
                    || !line[end..]
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_');

                if is_word_boundary_before && is_word_boundary_after
                    && !is_in_string(start) && !is_in_comment(start) {
                        annotations.push(HighlightAnnotation {
                            annotation_type: AnnotationType::PrimitiveType,
                            start,
                            end,
                        });
                    }
                search_start = start + 1;
            }
        }

        let number_ranges = find_number_ranges(line);
        for range in &number_ranges {
            if !is_in_string(range.start) && !is_in_comment(range.start) {
                let is_word_boundary_before = range.start == 0
                    || !line[..range.start]
                        .chars()
                        .last()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_');
                let is_word_boundary_after = range.end >= line.len()
                    || !line[range.end..]
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_');

                if is_word_boundary_before && is_word_boundary_after {
                    annotations.push(HighlightAnnotation {
                        annotation_type: AnnotationType::Number,
                        start: range.start,
                        end: range.end,
                    });
                }
            }
        }

        let type_ranges = find_type_ranges(line);
        for range in &type_ranges {
            if !is_in_string(range.start) && !is_in_comment(range.start) {
                let word = &line[range.start..range.end];
                let is_keyword = self.config.keywords.iter().any(|kw| kw.as_str() == word);
                if !is_keyword {
                    annotations.push(HighlightAnnotation {
                        annotation_type: AnnotationType::Type,
                        start: range.start,
                        end: range.end,
                    });
                }
            }
        }

        for range in &comment_ranges {
            annotations.push(HighlightAnnotation {
                annotation_type: AnnotationType::Comment,
                start: range.start,
                end: range.end,
            });
        }

        // 括弧のハイライト
        // 注意: HighlightState の paren_level, brace_level, bracket_level は
        // 現在の実装では順序に依存しているため、設定構造体のインデックスと対応させる
        let mut bracket_levels: [usize; 3] =
            [state.paren_level, state.brace_level, state.bracket_level];

        for (byte_idx, ch) in line.char_indices() {
            if !is_in_string(byte_idx) && !is_in_comment(byte_idx) {
                let mut bracket_type = None;

                for (bracket_idx, bracket_config) in self.config.brackets.iter().enumerate() {
                    if ch == bracket_config.open {
                        let level = (bracket_levels[bracket_idx] + bracket_config.color_offset) % 4;
                        bracket_levels[bracket_idx] += 1;
                        bracket_type = Some([
                            AnnotationType::Bracket0,
                            AnnotationType::Bracket1,
                            AnnotationType::Bracket2,
                            AnnotationType::Bracket3,
                        ][level]);
                        break;
                    } else if ch == bracket_config.close {
                        bracket_levels[bracket_idx] = bracket_levels[bracket_idx].saturating_sub(1);
                        let level = (bracket_levels[bracket_idx] + bracket_config.color_offset) % 4;
                        bracket_type = Some([
                            AnnotationType::Bracket0,
                            AnnotationType::Bracket1,
                            AnnotationType::Bracket2,
                            AnnotationType::Bracket3,
                        ][level]);
                        break;
                    }
                }

                if let Some(annotation_type) = bracket_type {
                    annotations.push(HighlightAnnotation {
                        annotation_type,
                        start: byte_idx,
                        end: byte_idx + ch.len_utf8(),
                    });
                }
            }
        }

        state.paren_level = bracket_levels[0];
        state.brace_level = bracket_levels[1];
        state.bracket_level = bracket_levels[2];

        (annotations, state)
    }

    fn language_name(&self) -> &'static str {
        "Rust"
    }
}
