pub mod highlighter;
pub use highlighter::{HighlightAnnotation, HighlightState, Highlighter, StringType};

pub mod config;
pub use config::{LanguageConfig, embedded_rust_config, merge_config};

pub mod config_file;
pub use config_file::load_language_config;

pub mod generic;

pub mod registry;
pub use registry::HighlighterRegistry;
