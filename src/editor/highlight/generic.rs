use super::Highlighter;
use crate::editor::annotated_string::AnnotationType;
use crate::editor::highlight::{
    HighlightAnnotation, HighlightState, LanguageConfig, StringType, load_language_config,
    merge_config,
};
use aho_corasick::AhoCorasick;

pub struct GenericHighlighter {
    config: LanguageConfig,
    language_name: String,
    /// Precompiled automaton covering all keywords then all `primitive_types`.
    /// Patterns `0..keyword_count` are keywords; the rest are primitive types.
    keyword_ac: Option<AhoCorasick>,
    keyword_count: usize,
}

impl GenericHighlighter {
    pub fn new(language: &str) -> Option<Self> {
        let config = if let Ok(lang_config) = load_language_config(language, None) {
            let default = LanguageConfig {
                keywords: vec![],
                primitive_types: vec![],
                line_comment_start: "//".to_string(),
                block_comment_start: "/*".to_string(),
                block_comment_end: "*/".to_string(),
                brackets: vec![],
            };
            merge_config(&default, Some(&lang_config))
        } else {
            #[cfg(debug_assertions)]
            {
                use std::path::Path;
                let path = format!("docs/examples/default/languages/{language}.toml");
                if let Ok(lang_config) = load_language_config(language, Some(Path::new(&path))) {
                    let default = LanguageConfig {
                        keywords: vec![],
                        primitive_types: vec![],
                        line_comment_start: "//".to_string(),
                        block_comment_start: "/*".to_string(),
                        block_comment_end: "*/".to_string(),
                        brackets: vec![],
                    };
                    merge_config(&default, Some(&lang_config))
                } else {
                    return None;
                }
            }

            #[cfg(not(debug_assertions))]
            {
                return None;
            }
        };

        let keyword_count = config.keywords.len();
        let all_patterns: Vec<&str> = config
            .keywords
            .iter()
            .chain(config.primitive_types.iter())
            .map(String::as_str)
            .collect();
        let keyword_ac = if all_patterns.is_empty() {
            None
        } else {
            AhoCorasick::new(&all_patterns).ok()
        };

        Some(Self {
            config,
            language_name: language.to_string(),
            keyword_ac,
            keyword_count,
        })
    }
}

#[allow(clippy::too_many_lines)]
fn find_string_ranges(
    string: &str,
    state: &mut HighlightState,
) -> (Vec<std::ops::Range<usize>>, Option<usize>) {
    let mut ranges = Vec::new();
    let mut escape_next = false;
    let mut start = if state.in_string.is_some() {
        Some(0)
    } else {
        None
    };

    let chars: Vec<(usize, char)> = string.char_indices().collect();
    let mut idx = 0;

    while idx < chars.len() {
        let (byte_idx, ch) = chars[idx];

        if escape_next {
            escape_next = false;
            idx += 1;
            continue;
        }

        if ch == '\\' && state.in_string.is_some() {
            escape_next = true;
            idx += 1;
            continue;
        }

        match state.in_string {
            Some(StringType::DoubleQuote) => {
                if ch == '"' {
                    let start_pos = start.unwrap_or(0);
                    ranges.push(start_pos..byte_idx + 1);
                    state.in_string = None;
                    start = None;
                }
                idx += 1;
            }
            Some(StringType::SingleQuote) => {
                if ch == '\'' {
                    let start_pos = start.unwrap_or(0);
                    ranges.push(start_pos..byte_idx + 1);
                    state.in_string = None;
                    start = None;
                }
                idx += 1;
            }
            Some(StringType::TripleDoubleQuote) => {
                // Check for """ to end triple quote
                if ch == '"' && idx + 2 < chars.len() {
                    let (_, ch2) = chars[idx + 1];
                    let (_, ch3) = chars[idx + 2];
                    if ch2 == '"' && ch3 == '"' {
                        let start_pos = start.unwrap_or(0);
                        ranges.push(start_pos..byte_idx + 3);
                        state.in_string = None;
                        start = None;
                        idx += 3;
                        continue;
                    }
                }
                idx += 1;
            }
            Some(StringType::TripleSingleQuote) => {
                // Check for ''' to end triple quote
                if ch == '\'' && idx + 2 < chars.len() {
                    let (_, ch2) = chars[idx + 1];
                    let (_, ch3) = chars[idx + 2];
                    if ch2 == '\'' && ch3 == '\'' {
                        let start_pos = start.unwrap_or(0);
                        ranges.push(start_pos..byte_idx + 3);
                        state.in_string = None;
                        start = None;
                        idx += 3;
                        continue;
                    }
                }
                idx += 1;
            }
            Some(StringType::Backtick) => {
                if ch == '`' {
                    let start_pos = start.unwrap_or(0);
                    ranges.push(start_pos..byte_idx + 1);
                    state.in_string = None;
                    start = None;
                }
                idx += 1;
            }
            None => {
                // Check for triple quotes first
                if ch == '"' && idx + 2 < chars.len() {
                    let (_, ch2) = chars[idx + 1];
                    let (_, ch3) = chars[idx + 2];
                    if ch2 == '"' && ch3 == '"' {
                        start = Some(byte_idx);
                        state.in_string = Some(StringType::TripleDoubleQuote);
                        idx += 3;
                        continue;
                    }
                }
                if ch == '\'' && idx + 2 < chars.len() {
                    let (_, ch2) = chars[idx + 1];
                    let (_, ch3) = chars[idx + 2];
                    if ch2 == '\'' && ch3 == '\'' {
                        start = Some(byte_idx);
                        state.in_string = Some(StringType::TripleSingleQuote);
                        idx += 3;
                        continue;
                    }
                }
                // Regular quotes
                if ch == '"' {
                    start = Some(byte_idx);
                    state.in_string = Some(StringType::DoubleQuote);
                } else if ch == '\'' {
                    start = Some(byte_idx);
                    state.in_string = Some(StringType::SingleQuote);
                } else if ch == '`' {
                    start = Some(byte_idx);
                    state.in_string = Some(StringType::Backtick);
                }
                idx += 1;
            }
        }
    }

    let continuation_start = if state.in_string.is_some() {
        start.or(Some(0))
    } else {
        None
    };

    (ranges, continuation_start)
}

fn is_word_boundary(ch: char) -> bool {
    !ch.is_alphanumeric() && ch != '_'
}

fn is_camel_case_boundary(prev_ch: char, next_ch: char) -> bool {
    prev_ch.is_lowercase() && next_ch.is_uppercase()
}

fn find_keyword_at(line: &str, keyword: &str, pos: usize) -> bool {
    if pos + keyword.len() > line.len() {
        return false;
    }

    let word = &line[pos..pos + keyword.len()];
    if word != keyword {
        return false;
    }

    let prev_char = if pos > 0 {
        let mut prev = None;
        for (byte_idx, ch) in line.char_indices() {
            if byte_idx >= pos {
                break;
            }
            prev = Some(ch);
        }
        prev
    } else {
        None
    };

    if let Some(keyword_first_char) = keyword.chars().next()
        && keyword_first_char.is_uppercase()
        && let Some(prev_ch) = prev_char
        && is_camel_case_boundary(prev_ch, keyword_first_char)
    {
        return false;
    }

    let before_ok = prev_char.is_none_or(is_word_boundary);

    let after_pos = pos + keyword.len();
    let after_ok = if after_pos >= line.len() {
        true
    } else {
        let mut next_char = None;
        for (byte_idx, ch) in line.char_indices() {
            if byte_idx == after_pos {
                next_char = Some(ch);
                break;
            }
        }
        next_char.is_none_or(is_word_boundary)
    };

    before_ok && after_ok
}

impl Highlighter for GenericHighlighter {
    fn language_name(&self) -> &str {
        &self.language_name
    }

    #[allow(clippy::too_many_lines)]
    fn highlight_line(
        &self,
        line: &str,
        _line_index: usize,
        mut state: HighlightState,
    ) -> (Vec<HighlightAnnotation>, HighlightState) {
        let mut annotations = Vec::new();

        let (string_ranges, continuation_start) = find_string_ranges(line, &mut state);
        for range in &string_ranges {
            annotations.push(HighlightAnnotation {
                start: range.start,
                end: range.end,
                annotation_type: AnnotationType::String,
            });
        }

        // If string continues to next line, highlight from start to end of line
        if let Some(start_pos) = continuation_start {
            annotations.push(HighlightAnnotation {
                start: start_pos,
                end: line.len(),
                annotation_type: AnnotationType::String,
            });
        }

        let continuation_range = continuation_start.map(|start| start..line.len());
        let is_in_string = |pos: usize| -> bool {
            string_ranges.iter().any(|range| range.contains(&pos))
                || continuation_range
                    .as_ref()
                    .is_some_and(|range| range.contains(&pos))
        };

        // Block comments
        let mut block_comment_ranges = Vec::new();
        if !self.config.block_comment_start.is_empty() && !self.config.block_comment_end.is_empty()
        {
            let mut pos = 0;
            let mut comment_start_pos = if state.in_block_comment {
                Some(0)
            } else {
                None
            };

            while pos < line.len() {
                if state.in_block_comment {
                    if let Some(end_pos) = line[pos..].find(self.config.block_comment_end.as_str())
                    {
                        let abs_end = pos + end_pos + self.config.block_comment_end.len();
                        let start = comment_start_pos.unwrap_or(0);
                        block_comment_ranges.push(start..abs_end);
                        state.in_block_comment = false;
                        comment_start_pos = None;
                        pos = abs_end;
                    } else {
                        block_comment_ranges.push(comment_start_pos.unwrap_or(0)..line.len());
                        break;
                    }
                } else if let Some(start_pos) =
                    line[pos..].find(self.config.block_comment_start.as_str())
                {
                    let abs_start = pos + start_pos;
                    if is_in_string(abs_start) {
                        pos = abs_start + 1;
                    } else {
                        state.in_block_comment = true;
                        comment_start_pos = Some(abs_start);
                        pos = abs_start + self.config.block_comment_start.len();
                    }
                } else {
                    break;
                }
            }
        }

        for range in &block_comment_ranges {
            annotations.push(HighlightAnnotation {
                start: range.start,
                end: range.end,
                annotation_type: AnnotationType::Comment,
            });
        }

        let is_in_block_comment = |pos: usize| -> bool {
            block_comment_ranges
                .iter()
                .any(|range| range.contains(&pos))
        };

        // Line comments
        if let Some(comment_start) = line.find(self.config.line_comment_start.as_str())
            && !is_in_string(comment_start)
            && !is_in_block_comment(comment_start)
        {
            annotations.push(HighlightAnnotation {
                start: comment_start,
                end: line.len(),
                annotation_type: AnnotationType::Comment,
            });
        }

        let is_in_comment = |pos: usize| -> bool {
            is_in_block_comment(pos) || {
                if let Some(comment_start) = line.find(self.config.line_comment_start.as_str()) {
                    !is_in_string(comment_start) && pos >= comment_start
                } else {
                    false
                }
            }
        };

        // Keywords and primitive types — single Aho-Corasick pass
        if let Some(ac) = &self.keyword_ac {
            for mat in ac.find_iter(line) {
                let abs_pos = mat.start();
                let pattern_idx = mat.pattern().as_usize();
                let word = if pattern_idx < self.keyword_count {
                    self.config.keywords[pattern_idx].as_str()
                } else {
                    self.config.primitive_types[pattern_idx - self.keyword_count].as_str()
                };
                if find_keyword_at(line, word, abs_pos)
                    && !is_in_string(abs_pos)
                    && !is_in_comment(abs_pos)
                {
                    let annotation_type = if pattern_idx < self.keyword_count {
                        AnnotationType::Keyword
                    } else {
                        AnnotationType::PrimitiveType
                    };
                    annotations.push(HighlightAnnotation {
                        start: abs_pos,
                        end: mat.end(),
                        annotation_type,
                    });
                }
            }
        }

        // Numbers — use char_indices() for byte offsets
        let mut chars = line.char_indices().peekable();
        while let Some((byte_idx, ch)) = chars.next() {
            if is_in_string(byte_idx) || is_in_comment(byte_idx) {
                continue;
            }

            if ch.is_ascii_digit()
                || (ch == '0'
                    && chars.peek().is_some_and(|(_, next_ch)| {
                        *next_ch == 'x' || *next_ch == 'b' || *next_ch == 'o'
                    }))
            {
                let start = byte_idx;
                let mut end = byte_idx + ch.len_utf8();

                if ch == '0'
                    && let Some(&(_, next_ch)) = chars.peek()
                    && (next_ch == 'x' || next_ch == 'b' || next_ch == 'o')
                {
                    chars.next();
                    end += 1; // 'x', 'b', 'o' are ASCII
                }

                while let Some(&(next_byte_idx, next_ch)) = chars.peek() {
                    if next_ch.is_alphanumeric() || next_ch == '_' || next_ch == '.' {
                        chars.next();
                        end = next_byte_idx + next_ch.len_utf8();
                    } else {
                        break;
                    }
                }

                annotations.push(HighlightAnnotation {
                    start,
                    end,
                    annotation_type: AnnotationType::Number,
                });
            }
        }

        let mut chars = line.char_indices().peekable();
        let mut prev_char: Option<char> = None;

        while let Some((byte_idx, ch)) = chars.next() {
            if is_in_string(byte_idx) || is_in_comment(byte_idx) {
                prev_char = Some(ch);
                continue;
            }

            if ch.is_uppercase() {
                // Only consider this a type name start if the previous character is a word boundary
                if let Some(prev) = prev_char
                    && !is_word_boundary(prev)
                {
                    prev_char = Some(ch);
                    continue;
                }

                let start = byte_idx;
                let mut end = start + ch.len_utf8();

                while let Some(&(next_byte_idx, next_ch)) = chars.peek() {
                    if next_ch.is_alphanumeric() || next_ch == '_' {
                        chars.next();
                        end = next_byte_idx + next_ch.len_utf8();
                    } else {
                        break;
                    }
                }

                let mut next_char: Option<char> = None;
                if let Some(&(_, ch_after)) = chars.peek() {
                    next_char = Some(ch_after);
                }

                let after_ok = next_char.is_none_or(is_word_boundary);

                if after_ok {
                    let word = &line[start..end];
                    if !self.config.keywords.iter().any(|kw| kw == word)
                        && !self.config.primitive_types.iter().any(|pt| pt == word)
                    {
                        annotations.push(HighlightAnnotation {
                            start,
                            end,
                            annotation_type: AnnotationType::Type,
                        });
                    }
                }
            }

            prev_char = Some(ch);
        }

        // Brackets — use char_indices() for byte offsets
        let mut paren_level = state.paren_level;
        let mut brace_level = state.brace_level;
        let mut bracket_level = state.bracket_level;

        for (byte_idx, ch) in line.char_indices() {
            if is_in_string(byte_idx) || is_in_comment(byte_idx) {
                continue;
            }

            if ch == '(' {
                let annotation_type = [
                    AnnotationType::Bracket0,
                    AnnotationType::Bracket1,
                    AnnotationType::Bracket2,
                    AnnotationType::Bracket3,
                ][paren_level % 4];
                annotations.push(HighlightAnnotation {
                    start: byte_idx,
                    end: byte_idx + 1,
                    annotation_type,
                });
                paren_level += 1;
            } else if ch == ')' {
                paren_level = paren_level.saturating_sub(1);
                let annotation_type = [
                    AnnotationType::Bracket0,
                    AnnotationType::Bracket1,
                    AnnotationType::Bracket2,
                    AnnotationType::Bracket3,
                ][paren_level % 4];
                annotations.push(HighlightAnnotation {
                    start: byte_idx,
                    end: byte_idx + 1,
                    annotation_type,
                });
            } else if ch == '{' {
                let annotation_type = [
                    AnnotationType::Bracket0,
                    AnnotationType::Bracket1,
                    AnnotationType::Bracket2,
                    AnnotationType::Bracket3,
                ][(brace_level + 1) % 4];
                annotations.push(HighlightAnnotation {
                    start: byte_idx,
                    end: byte_idx + 1,
                    annotation_type,
                });
                brace_level += 1;
            } else if ch == '}' {
                brace_level = brace_level.saturating_sub(1);
                let annotation_type = [
                    AnnotationType::Bracket0,
                    AnnotationType::Bracket1,
                    AnnotationType::Bracket2,
                    AnnotationType::Bracket3,
                ][(brace_level + 1) % 4];
                annotations.push(HighlightAnnotation {
                    start: byte_idx,
                    end: byte_idx + 1,
                    annotation_type,
                });
            } else if ch == '[' {
                let annotation_type = [
                    AnnotationType::Bracket0,
                    AnnotationType::Bracket1,
                    AnnotationType::Bracket2,
                    AnnotationType::Bracket3,
                ][(bracket_level + 2) % 4];
                annotations.push(HighlightAnnotation {
                    start: byte_idx,
                    end: byte_idx + 1,
                    annotation_type,
                });
                bracket_level += 1;
            } else if ch == ']' {
                bracket_level = bracket_level.saturating_sub(1);
                let annotation_type = [
                    AnnotationType::Bracket0,
                    AnnotationType::Bracket1,
                    AnnotationType::Bracket2,
                    AnnotationType::Bracket3,
                ][(bracket_level + 2) % 4];
                annotations.push(HighlightAnnotation {
                    start: byte_idx,
                    end: byte_idx + 1,
                    annotation_type,
                });
            }
        }

        state.paren_level = paren_level;
        state.brace_level = brace_level;
        state.bracket_level = bracket_level;

        (annotations, state)
    }
}
