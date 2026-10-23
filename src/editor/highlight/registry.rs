use super::Highlighter;
use super::config_file::discover_language_extensions;
use super::generic::GenericHighlighter;
use super::load_language_config;
use super::{embedded_rust_config, merge_config};
use std::collections::HashMap;

pub struct HighlighterRegistry {
    highlighters: Vec<Box<dyn Highlighter>>,
    extension_map: HashMap<String, usize>,
}

impl HighlighterRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            highlighters: Vec::new(),
            extension_map: HashMap::new(),
        };

        let rust_config = if let Ok(user) = load_language_config("rust", None) {
            merge_config(&embedded_rust_config(), Some(&user))
        } else {
            embedded_rust_config()
        };
        registry.register(
            Box::new(GenericHighlighter::from_config("rust", rust_config)),
            vec!["rs".to_string()],
        );

        for (language, extensions) in discover_language_extensions() {
            if language.eq_ignore_ascii_case("rust") {
                continue;
            }

            if let Some(highlighter) = GenericHighlighter::new(&language) {
                registry.register(Box::new(highlighter), extensions);
            }
        }

        registry
    }

    pub fn register(&mut self, highlighter: Box<dyn Highlighter>, extensions: Vec<String>) {
        let index = self.highlighters.len();
        self.highlighters.push(highlighter);

        for ext in extensions {
            self.extension_map.insert(ext, index);
        }
    }

    pub fn get_highlighter(&self, file_extension: Option<&str>) -> Option<&dyn Highlighter> {
        let ext = file_extension?;
        let index = self.extension_map.get(ext)?;
        self.highlighters
            .get(*index)
            .map(std::convert::AsRef::as_ref)
    }
}

impl Default for HighlighterRegistry {
    fn default() -> Self {
        Self::new()
    }
}
