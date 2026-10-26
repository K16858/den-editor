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
                line_prefix_keyword_markers: vec![],
                line_prefix_comment_markers: vec![],
                full_line_comment_prefixes: vec![],
                highlight_capitalized_type_names: true,
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
                        line_prefix_keyword_markers: vec![],
                        line_prefix_comment_markers: vec![],
                        full_line_comment_prefixes: vec![],
                        highlight_capitalized_type_names: true,
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

        Some(Self::from_config(language, config))
    }

    pub fn from_config(language: &str, config: LanguageConfig) -> Self {
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

        Self {
            config,
            language_name: language.to_string(),
            keyword_ac,
            keyword_count,
        }
    }
}

#[allow(clippy::too_many_lines)]
fn scan_strings_and_comments(
    line: &str,
    state: &mut HighlightState,
    line_comment: &str,
    block_start: &str,
    block_end: &str,
) -> (Vec<std::ops::Range<usize>>, Vec<std::ops::Range<usize>>) {
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let mut strings = Vec::new();
    let mut comments = Vec::new();
    let mut i = 0;
    let mut escape_next = false;
    let mut string_from = if state.in_string.is_some() { Some(0) } else { None };
    let mut block_from = if state.in_block_comment { Some(0) } else { None };

    let at_byte = |chars: &[(usize, char)], byte: usize| -> usize {
        chars.iter().position(|(b, _)| *b >= byte).unwrap_or(chars.len())
    };

    while i < chars.len() {
        let (byte_idx, ch) = chars[i];

        if state.in_block_comment {
            if !block_end.is_empty() && line[byte_idx..].starts_with(block_end) {
                let end = byte_idx + block_end.len();
                comments.push(block_from.unwrap_or(0)..end);
                state.in_block_comment = false;
                block_from = None;
                i = at_byte(&chars, end);
                continue;
            }
            comments.push(block_from.unwrap_or(0)..line.len());
            break;
        }

        if state.in_string.is_some() {
            if escape_next {
                escape_next = false;
                i += 1;
                continue;
            }
            if ch == '\\' {
                escape_next = true;
                i += 1;
                continue;
            }
            let kind = state.in_string.unwrap();
            let triple = matches!(
                kind,
                StringType::TripleDoubleQuote | StringType::TripleSingleQuote
            );
            let closed = match kind {
                StringType::DoubleQuote => ch == '"',
                StringType::SingleQuote => ch == '\'',
                StringType::Backtick => ch == '`',
                StringType::TripleDoubleQuote => {
                    ch == '"'
                        && i + 2 < chars.len()
                        && chars[i + 1].1 == '"'
                        && chars[i + 2].1 == '"'
                }
                StringType::TripleSingleQuote => {
                    ch == '\''
                        && i + 2 < chars.len()
                        && chars[i + 1].1 == '\''
                        && chars[i + 2].1 == '\''
                }
            };
            if closed {
                let end = if triple { byte_idx + 3 } else { byte_idx + ch.len_utf8() };
                strings.push(string_from.unwrap_or(0)..end);
                state.in_string = None;
                string_from = None;
                i += if triple { 3 } else { 1 };
                continue;
            }
            i += 1;
            continue;
        }

        if !line_comment.is_empty() && line[byte_idx..].starts_with(line_comment) {
            comments.push(byte_idx..line.len());
            break;
        }
        if !block_start.is_empty()
            && !block_end.is_empty()
            && line[byte_idx..].starts_with(block_start)
        {
            state.in_block_comment = true;
            block_from = Some(byte_idx);
            i = at_byte(&chars, byte_idx + block_start.len());
            continue;
        }
        if ch == '"' && i + 2 < chars.len() && chars[i + 1].1 == '"' && chars[i + 2].1 == '"' {
            state.in_string = Some(StringType::TripleDoubleQuote);
            string_from = Some(byte_idx);
            i += 3;
            continue;
        }
        if ch == '\'' && i + 2 < chars.len() && chars[i + 1].1 == '\'' && chars[i + 2].1 == '\'' {
            state.in_string = Some(StringType::TripleSingleQuote);
            string_from = Some(byte_idx);
            i += 3;
            continue;
        }
        if ch == '"' {
            state.in_string = Some(StringType::DoubleQuote);
            string_from = Some(byte_idx);
        } else if ch == '\'' {
            state.in_string = Some(StringType::SingleQuote);
            string_from = Some(byte_idx);
        } else if ch == '`' {
            state.in_string = Some(StringType::Backtick);
            string_from = Some(byte_idx);
        }
        i += 1;
    }

    if state.in_string.is_some() {
        strings.push(string_from.unwrap_or(0)..line.len());
    }
    if state.in_block_comment {
        let start = block_from.unwrap_or(0);
        if !comments
            .iter()
            .any(|range| range.start == start && range.end == line.len())
        {
            comments.push(start..line.len());
        }
    }
    (strings, comments)
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
        let trimmed = line.trim_start();
        let indent = line.len().saturating_sub(trimmed.len());

        for prefix in &self.config.full_line_comment_prefixes {
            if !prefix.is_empty() && trimmed.starts_with(prefix) {
                annotations.push(HighlightAnnotation {
                    start: indent,
                    end: line.len(),
                    annotation_type: AnnotationType::Comment,
                });
                return (annotations, state);
            }
        }

        for prefix in &self.config.line_prefix_keyword_markers {
            if !prefix.is_empty() && trimmed.starts_with(prefix) {
                let end = indent + prefix.len();
                annotations.push(HighlightAnnotation {
                    start: indent,
                    end: end.min(line.len()),
                    annotation_type: AnnotationType::Keyword,
                });
                break;
            }
        }

        for prefix in &self.config.line_prefix_comment_markers {
            if !prefix.is_empty() && trimmed.starts_with(prefix) {
                let end = indent + prefix.len();
                annotations.push(HighlightAnnotation {
                    start: indent,
                    end: end.min(line.len()),
                    annotation_type: AnnotationType::Comment,
                });
                break;
            }
        }

        let (string_ranges, comment_ranges) = scan_strings_and_comments(
            line,
            &mut state,
            &self.config.line_comment_start,
            &self.config.block_comment_start,
            &self.config.block_comment_end,
        );
        for range in &string_ranges {
            annotations.push(HighlightAnnotation {
                start: range.start,
                end: range.end,
                annotation_type: AnnotationType::String,
            });
        }
        for range in &comment_ranges {
            annotations.push(HighlightAnnotation {
                start: range.start,
                end: range.end,
                annotation_type: AnnotationType::Comment,
            });
        }

        let is_in_string = |pos: usize| -> bool {
            string_ranges.iter().any(|range| range.contains(&pos))
        };
        let is_in_comment = |pos: usize| -> bool {
            comment_ranges.iter().any(|range| range.contains(&pos))
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

        if self.config.highlight_capitalized_type_names {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn highlighter() -> GenericHighlighter {
        GenericHighlighter::from_config(
            "test",
            LanguageConfig {
                keywords: vec!["let".to_string()],
                primitive_types: vec![],
                line_comment_start: "//".to_string(),
                block_comment_start: "/*".to_string(),
                block_comment_end: "*/".to_string(),
                line_prefix_keyword_markers: vec![],
                line_prefix_comment_markers: vec![],
                full_line_comment_prefixes: vec![],
                highlight_capitalized_type_names: false,
            },
        )
    }

    fn covers(annotations: &[HighlightAnnotation], kind: AnnotationType, pos: usize) -> bool {
        annotations
            .iter()
            .any(|ann| ann.annotation_type == kind && (ann.start..ann.end).contains(&pos))
    }

    #[test]
    fn line_comment_after_url_in_string_is_a_comment() {
        let line = "let s = \"http://example.com\"; // real";
        let (annotations, state) = highlighter().highlight_line(line, 0, HighlightState::default());
        let inside_url = line.find("http://").unwrap() + 5;
        let comment = line.find("// real").unwrap();
        assert!(covers(&annotations, AnnotationType::String, inside_url));
        assert!(!covers(
            &annotations,
            AnnotationType::Comment,
            inside_url
        ));
        assert!(covers(&annotations, AnnotationType::Comment, comment));
        assert!(state.in_string.is_none());
    }

    #[test]
    fn quote_inside_line_comment_does_not_open_a_string() {
        let hl = highlighter();
        let (_, state) = hl.highlight_line("// \"hello", 0, HighlightState::default());
        assert!(state.in_string.is_none());
        let (annotations, state) = hl.highlight_line("let x = 1;", 1, state);
        assert!(state.in_string.is_none());
        assert!(!covers(&annotations, AnnotationType::String, 0));
    }

    #[test]
    fn block_comment_inside_line_comment_does_not_continue() {
        let hl = highlighter();
        let (_, state) = hl.highlight_line("// /*", 0, HighlightState::default());
        assert!(!state.in_block_comment);
        let (annotations, _) = hl.highlight_line("let x = 1;", 1, state);
        assert!(!covers(&annotations, AnnotationType::Comment, 0));
    }
}

