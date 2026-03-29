use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
pub struct ColorsConfigFile {
    pub colors: Option<ColorsConfig>,
}

#[derive(Deserialize)]
pub struct ColorsConfig {
    pub keyword: Option<ColorRgb>,
    pub number: Option<ColorRgb>,
    pub type_name: Option<ColorRgb>,
    pub primitive_type: Option<ColorRgb>,
    pub string: Option<ColorRgb>,
    pub comment: Option<ColorRgb>,
    pub brackets: Option<Vec<ColorRgb>>,
}

#[derive(Deserialize)]
pub struct LanguageConfigFile {
    pub keywords: Option<Vec<String>>,
    pub primitive_types: Option<Vec<String>>,
    pub line_comment_start: Option<String>,
    pub block_comment_start: Option<String>,
    pub block_comment_end: Option<String>,
    pub brackets: Option<Vec<BracketConfigFile>>,
    pub extensions: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct BracketConfigFile {
    pub open: String,
    pub close: String,
    pub color_offset: Option<usize>,
}

#[derive(Deserialize)]
pub struct ColorRgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Debug)]
pub enum ConfigError {
    FileNotFound,
    #[allow(dead_code)]
    IoError(std::io::Error),
    #[allow(dead_code)]
    ParseError(toml::de::Error),
}

pub fn load_colors_config(custom_path: Option<&Path>) -> Result<ColorsConfigFile, ConfigError> {
    let config_path = if let Some(path) = custom_path {
        path.to_path_buf()
    } else {
        get_config_dir()?.join("colors.toml")
    };

    if !config_path.exists() {
        return Err(ConfigError::FileNotFound);
    }

    let contents = fs::read_to_string(&config_path).map_err(ConfigError::IoError)?;
    let config: ColorsConfigFile = toml::from_str(&contents).map_err(ConfigError::ParseError)?;

    Ok(config)
}

pub fn load_language_config(
    language: &str,
    custom_path: Option<&Path>,
) -> Result<LanguageConfigFile, ConfigError> {
    let config_path = if let Some(path) = custom_path {
        path.to_path_buf()
    } else {
        get_config_dir()?
            .join("languages")
            .join(format!("{language}.toml"))
    };

    if !config_path.exists() {
        return Err(ConfigError::FileNotFound);
    }

    let contents = fs::read_to_string(&config_path).map_err(ConfigError::IoError)?;
    let config: LanguageConfigFile = toml::from_str(&contents).map_err(ConfigError::ParseError)?;

    Ok(config)
}

pub fn discover_language_extensions() -> Vec<(String, Vec<String>)> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();

    if let Ok(mut config_dir) = get_config_dir() {
        config_dir.push("languages");
        load_languages_from_dir(&config_dir, &mut map, false);
    }

    #[cfg(debug_assertions)]
    {
        let default_dir = Path::new("docs/examples/default/languages");
        load_languages_from_dir(default_dir, &mut map, true);
    }

    map.into_iter().collect()
}

fn load_languages_from_dir(dir: &Path, map: &mut HashMap<String, Vec<String>>, is_default: bool) {
    let Ok(read_dir) = fs::read_dir(dir) else { return };

    for entry in read_dir {
        let Ok(entry) = entry else { continue };
        let path = entry.path();

        if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
            continue;
        }

        let file_stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(stem) => stem.to_string(),
            None => continue,
        };

        if is_default && map.contains_key(&file_stem) {
            continue;
        }

        let Ok(config) = load_language_config(&file_stem, Some(&path)) else { continue };

        let exts = config
            .extensions
            .clone()
            .unwrap_or_else(|| vec![file_stem.clone()]);

        map.insert(file_stem, exts);
    }
}

fn get_config_dir() -> Result<PathBuf, ConfigError> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA")
            .map(|appdata| PathBuf::from(appdata).join("den"))
            .map_err(|_| ConfigError::FileNotFound)
    }

    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME")
            .map(|home| PathBuf::from(home).join(".config").join("den"))
            .map_err(|_| ConfigError::FileNotFound)
    }
}
