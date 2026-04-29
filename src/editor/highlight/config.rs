#[derive(Clone)]
pub struct BracketConfig {
    pub open: char,
    pub close: char,
    pub color_offset: usize,
}

#[derive(Clone)]
pub struct LanguageConfig {
    pub keywords: Vec<String>,
    pub primitive_types: Vec<String>,
    pub line_comment_start: String,
    pub block_comment_start: String,
    pub block_comment_end: String,
    pub brackets: Vec<BracketConfig>,
    pub line_prefix_keyword_markers: Vec<String>,
    pub line_prefix_comment_markers: Vec<String>,
    pub full_line_comment_prefixes: Vec<String>,
}

pub fn default_rust_config() -> LanguageConfig {
    #[cfg(debug_assertions)]
    {
        // Debug build: try to load from docs/examples/default/languages/rust.toml
        use std::path::Path;
        if let Ok(lang_config) = load_language_config(
            "rust",
            Some(Path::new("docs/examples/default/languages/rust.toml")),
        ) {
            return merge_config(&hardcoded_rust_config(), Some(&lang_config));
        }
    }

    // Release build or fallback: use hardcoded values
    hardcoded_rust_config()
}

fn hardcoded_rust_config() -> LanguageConfig {
    LanguageConfig {
        keywords: vec![
            "fn".to_string(),
            "let".to_string(),
            "mut".to_string(),
            "if".to_string(),
            "else".to_string(),
            "for".to_string(),
            "while".to_string(),
            "match".to_string(),
        ],
        primitive_types: vec![
            "i8".to_string(),
            "i16".to_string(),
            "i32".to_string(),
            "i64".to_string(),
            "i128".to_string(),
            "u8".to_string(),
            "u16".to_string(),
            "u32".to_string(),
            "u64".to_string(),
            "u128".to_string(),
            "f32".to_string(),
            "f64".to_string(),
        ],
        line_comment_start: "//".to_string(),
        block_comment_start: "/*".to_string(),
        block_comment_end: "*/".to_string(),
        brackets: vec![
            BracketConfig {
                open: '(',
                close: ')',
                color_offset: 0,
            },
            BracketConfig {
                open: '{',
                close: '}',
                color_offset: 1,
            },
            BracketConfig {
                open: '[',
                close: ']',
                color_offset: 2,
            },
        ],
        line_prefix_keyword_markers: vec![],
        line_prefix_comment_markers: vec![],
        full_line_comment_prefixes: vec![],
    }
}

use crate::editor::highlight::config_file::{BracketConfigFile, LanguageConfigFile};

#[cfg(debug_assertions)]
use crate::editor::highlight::config_file::load_language_config;

pub fn merge_config(
    default: &LanguageConfig,
    file_config: Option<&LanguageConfigFile>,
) -> LanguageConfig {
    let Some(file_config) = file_config else {
        return default.clone();
    };

    LanguageConfig {
        keywords: file_config
            .keywords
            .clone()
            .unwrap_or_else(|| default.keywords.clone()),
        primitive_types: file_config
            .primitive_types
            .clone()
            .unwrap_or_else(|| default.primitive_types.clone()),
        line_comment_start: file_config
            .line_comment_start
            .clone()
            .unwrap_or_else(|| default.line_comment_start.clone()),
        block_comment_start: file_config
            .block_comment_start
            .clone()
            .unwrap_or_else(|| default.block_comment_start.clone()),
        block_comment_end: file_config
            .block_comment_end
            .clone()
            .unwrap_or_else(|| default.block_comment_end.clone()),
        brackets: merge_brackets(&default.brackets, file_config.brackets.as_ref()),
        line_prefix_keyword_markers: file_config
            .line_prefix_keyword_markers
            .clone()
            .unwrap_or_else(|| default.line_prefix_keyword_markers.clone()),
        line_prefix_comment_markers: file_config
            .line_prefix_comment_markers
            .clone()
            .unwrap_or_else(|| default.line_prefix_comment_markers.clone()),
        full_line_comment_prefixes: file_config
            .full_line_comment_prefixes
            .clone()
            .unwrap_or_else(|| default.full_line_comment_prefixes.clone()),
    }
}

fn merge_brackets(
    default: &[BracketConfig],
    file_brackets: Option<&Vec<BracketConfigFile>>,
) -> Vec<BracketConfig> {
    let Some(file_brackets) = file_brackets else {
        return default.to_vec();
    };

    file_brackets
        .iter()
        .map(|b| BracketConfig {
            open: b.open.chars().next().unwrap_or('('),
            close: b.close.chars().next().unwrap_or(')'),
            color_offset: b.color_offset.unwrap_or(0),
        })
        .collect()
}
