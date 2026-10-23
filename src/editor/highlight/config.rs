#[derive(Clone)]
pub struct LanguageConfig {
    pub keywords: Vec<String>,
    pub primitive_types: Vec<String>,
    pub line_comment_start: String,
    pub block_comment_start: String,
    pub block_comment_end: String,
    pub line_prefix_keyword_markers: Vec<String>,
    pub line_prefix_comment_markers: Vec<String>,
    pub full_line_comment_prefixes: Vec<String>,
    pub highlight_capitalized_type_names: bool,
}

pub fn embedded_rust_config() -> LanguageConfig {
    let file: LanguageConfigFile = toml::from_str(include_str!(
        "../../../docs/examples/default/languages/rust.toml"
    ))
    .expect("embedded rust.toml");
    merge_config(&blank_language_config(), Some(&file))
}

fn blank_language_config() -> LanguageConfig {
    LanguageConfig {
        keywords: vec![],
        primitive_types: vec![],
        line_comment_start: "//".to_string(),
        block_comment_start: "/*".to_string(),
        block_comment_end: "*/".to_string(),
        line_prefix_keyword_markers: vec![],
        line_prefix_comment_markers: vec![],
        full_line_comment_prefixes: vec![],
        highlight_capitalized_type_names: true,
    }
}

use crate::editor::highlight::config_file::LanguageConfigFile;

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
        highlight_capitalized_type_names: file_config
            .highlight_capitalized_type_names
            .unwrap_or(default.highlight_capitalized_type_names),
    }
}
