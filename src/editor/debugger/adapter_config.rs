use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Deserialize)]
pub struct AdapterConfig {
    pub id: String,
    pub display_name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub file_extensions: Vec<String>,
    #[serde(default)]
    pub dap_adapter_type: String,
    #[serde(default)]
    pub launch_overrides: Value,
}

#[derive(Debug)]
pub enum AdapterConfigError {
    NotFound,
    Io(std::io::Error),
    Parse(toml::de::Error),
}

pub fn discover_adapter_configs() -> Vec<AdapterConfig> {
    let mut map: HashMap<String, AdapterConfig> = HashMap::new();

    if let Ok(mut config_dir) = get_config_dir() {
        config_dir.push("debuggers");
        load_from_dir(&config_dir, &mut map, false);
    }

    let default_dir = Path::new("docs/examples/default/debuggers");
    load_from_dir(default_dir, &mut map, true);

    map.into_values().collect()
}

fn load_from_dir(dir: &Path, map: &mut HashMap<String, AdapterConfig>, is_default: bool) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };

    for entry in read_dir {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
            continue;
        }

        let Ok(cfg) = load_adapter_config(&path) else {
            continue;
        };

        if is_default && map.contains_key(&cfg.id) {
            continue;
        }
        map.insert(cfg.id.clone(), cfg);
    }
}

pub fn load_adapter_config(path: &Path) -> Result<AdapterConfig, AdapterConfigError> {
    if !path.exists() {
        return Err(AdapterConfigError::NotFound);
    }
    let contents = fs::read_to_string(path).map_err(AdapterConfigError::Io)?;
    toml::from_str(&contents).map_err(AdapterConfigError::Parse)
}

fn get_config_dir() -> Result<PathBuf, AdapterConfigError> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA")
            .map(|appdata| PathBuf::from(appdata).join("den"))
            .map_err(|_| AdapterConfigError::NotFound)
    }

    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME")
            .map(|home| PathBuf::from(home).join(".config").join("den"))
            .map_err(|_| AdapterConfigError::NotFound)
    }
}
