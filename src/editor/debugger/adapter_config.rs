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
    pub dap_transport: String,
    #[serde(default)]
    pub session_cwd_template: String,
    #[serde(default)]
    pub health_check_command: String,
    /// `None` means "not set in this file"; see `client_addr_arg` doc comment.
    #[serde(default)]
    pub health_check_args: Option<Vec<String>>,
    #[serde(default)]
    pub health_check_fallback_command: String,
    #[serde(default)]
    pub health_check_fallback_args: Vec<String>,
    /// Launch args used when `health_check_fallback_command` starts the adapter.
    /// `None` keeps `args`.
    #[serde(default)]
    pub fallback_launch_args: Option<Vec<String>>,
    #[serde(default)]
    pub not_found_hint: String,
    /// `None` means "not set in this file" (falls back to the default adapter's
    /// value, or `DEFAULT_CLIENT_ADDR_ARG` if there is none). Kept as `Option`
    /// rather than a sentinel string so an explicit override can't be confused
    /// with "unset" during default-merging.
    #[serde(default)]
    pub client_addr_arg: Option<String>,
    #[serde(default)]
    pub auto_continue_stop_reasons: Vec<String>,
    /// `None` means "not set in this file"; see `client_addr_arg` doc comment.
    #[serde(default)]
    pub sync_breakpoints_before_auto_continue: Option<bool>,
    #[serde(default)]
    pub stacktrace_continue_error_patterns: Vec<String>,
    #[serde(default)]
    pub stacktrace_retry_error_patterns: Vec<String>,
    #[serde(default)]
    pub launch_template: Value,
    #[serde(default)]
    pub launch_overrides: Value,
}

const DEFAULT_CLIENT_ADDR_ARG: &str = "--client-addr";

const DEFAULT_ADAPTER_CONFIGS: &[&str] = &[
    include_str!("../../../docs/examples/default/debuggers/go.toml"),
    include_str!("../../../docs/examples/default/debuggers/python.toml"),
    include_str!("../../../docs/examples/default/debuggers/rust.toml"),
];

fn default_health_check_args() -> Vec<String> {
    vec!["--version".to_string()]
}

impl AdapterConfig {
    pub fn client_addr_arg(&self) -> &str {
        self.client_addr_arg.as_deref().unwrap_or(DEFAULT_CLIENT_ADDR_ARG)
    }

    pub fn sync_breakpoints_before_auto_continue(&self) -> bool {
        self.sync_breakpoints_before_auto_continue.unwrap_or(false)
    }

    pub fn health_check_args(&self) -> Vec<String> {
        self.health_check_args
            .clone()
            .unwrap_or_else(default_health_check_args)
    }

    pub fn with_fallback_launch(&self) -> Self {
        let mut resolved = self.clone();
        resolved.command = self.health_check_fallback_command.clone();
        if let Some(args) = &self.fallback_launch_args {
            resolved.args = args.clone();
        }
        resolved
    }
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
        load_from_dir(&config_dir, &mut map);
    }

    for cfg in embedded_default_configs() {
        if let Some(existing) = map.get_mut(&cfg.id) {
            merge_missing_fields_from_default(existing, &cfg);
        } else {
            map.insert(cfg.id.clone(), cfg);
        }
    }

    map.into_values().collect()
}

fn embedded_default_configs() -> Vec<AdapterConfig> {
    DEFAULT_ADAPTER_CONFIGS
        .iter()
        .map(|contents| parse_adapter_config(contents).expect("embedded debugger config"))
        .collect()
}

fn load_from_dir(dir: &Path, map: &mut HashMap<String, AdapterConfig>) {
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
        map.insert(cfg.id.clone(), cfg);
    }
}

fn merge_missing_fields_from_default(current: &mut AdapterConfig, default_cfg: &AdapterConfig) {
    if current.display_name.is_empty() {
        current.display_name = default_cfg.display_name.clone();
    }
    if current.command.is_empty() {
        current.command = default_cfg.command.clone();
    }
    if current.args.is_empty() {
        current.args = default_cfg.args.clone();
    }
    if current.file_extensions.is_empty() {
        current.file_extensions = default_cfg.file_extensions.clone();
    }
    if current.dap_adapter_type.is_empty() {
        current.dap_adapter_type = default_cfg.dap_adapter_type.clone();
    }
    if current.dap_transport.is_empty() {
        current.dap_transport = default_cfg.dap_transport.clone();
    }
    if current.session_cwd_template.is_empty() {
        current.session_cwd_template = default_cfg.session_cwd_template.clone();
    }
    if current.health_check_command.is_empty() {
        current.health_check_command = default_cfg.health_check_command.clone();
    }
    if current.health_check_args.is_none() {
        current.health_check_args = default_cfg.health_check_args.clone();
    }
    if current.health_check_fallback_command.is_empty() {
        current.health_check_fallback_command = default_cfg.health_check_fallback_command.clone();
    }
    if current.health_check_fallback_args.is_empty() {
        current.health_check_fallback_args = default_cfg.health_check_fallback_args.clone();
    }
    if current.fallback_launch_args.is_none() {
        current.fallback_launch_args = default_cfg.fallback_launch_args.clone();
    }
    if current.not_found_hint.is_empty() {
        current.not_found_hint = default_cfg.not_found_hint.clone();
    }
    if current.client_addr_arg.is_none() {
        current.client_addr_arg = default_cfg.client_addr_arg.clone();
    }
    if current.auto_continue_stop_reasons.is_empty() {
        current.auto_continue_stop_reasons = default_cfg.auto_continue_stop_reasons.clone();
    }
    if current.sync_breakpoints_before_auto_continue.is_none() {
        current.sync_breakpoints_before_auto_continue = default_cfg.sync_breakpoints_before_auto_continue;
    }
    if current.stacktrace_continue_error_patterns.is_empty() {
        current.stacktrace_continue_error_patterns =
            default_cfg.stacktrace_continue_error_patterns.clone();
    }
    if current.stacktrace_retry_error_patterns.is_empty() {
        current.stacktrace_retry_error_patterns =
            default_cfg.stacktrace_retry_error_patterns.clone();
    }
    if current.launch_template.is_null() {
        current.launch_template = default_cfg.launch_template.clone();
    }
    if current.launch_overrides.is_null() {
        current.launch_overrides = default_cfg.launch_overrides.clone();
    }
}

pub fn load_adapter_config(path: &Path) -> Result<AdapterConfig, AdapterConfigError> {
    if !path.exists() {
        return Err(AdapterConfigError::NotFound);
    }
    let contents = fs::read_to_string(path).map_err(AdapterConfigError::Io)?;
    parse_adapter_config(&contents)
}

fn parse_adapter_config(contents: &str) -> Result<AdapterConfig, AdapterConfigError> {
    toml::from_str(contents).map_err(AdapterConfigError::Parse)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_defaults_are_the_shipped_adapters() {
        let configs = embedded_default_configs();
        let ids: Vec<_> = configs.iter().map(|cfg| cfg.id.as_str()).collect();
        assert!(ids.contains(&"go-delve"));
        assert!(ids.contains(&"python-debugpy"));
        assert!(ids.contains(&"rust-codelldb"));
        assert!(!ids.contains(&"my-adapter"));

        let go = configs.iter().find(|cfg| cfg.id == "go-delve").unwrap();
        assert_eq!(go.dap_transport, "tcp-server-dials-client");
        assert_eq!(go.session_cwd_template, "${moduleRoot}");
        assert!(go.sync_breakpoints_before_auto_continue());
    }

    #[test]
    fn partial_user_config_inherits_embedded_go_defaults() {
        let mut current = parse_adapter_config(
            r#"
            id = "go-delve"
            display_name = "Go (Delve)"
            command = "dlv"
            args = ["dap"]
            file_extensions = ["go"]
            dap_adapter_type = "dlv-dap"
            launch_overrides = { output = "keep" }
            "#,
        )
        .unwrap();
        let default_cfg = embedded_default_configs()
            .into_iter()
            .find(|cfg| cfg.id == "go-delve")
            .unwrap();
        merge_missing_fields_from_default(&mut current, &default_cfg);
        assert_eq!(current.dap_transport, "tcp-server-dials-client");
        assert_eq!(current.session_cwd_template, "${moduleRoot}");
        assert_eq!(current.auto_continue_stop_reasons, ["entry"]);
        assert_eq!(current.launch_template["program"], ".");
        assert_eq!(current.launch_overrides["output"], "keep");
        assert_eq!(current.health_check_args(), ["version"]);
    }

    #[test]
    fn explicit_health_check_args_override_the_default() {
        let mut current = parse_adapter_config(
            r#"
            id = "go-delve"
            display_name = "Go (Delve)"
            command = "dlv"
            health_check_args = ["--version"]
            "#,
        )
        .unwrap();
        let default_cfg = embedded_default_configs()
            .into_iter()
            .find(|cfg| cfg.id == "go-delve")
            .unwrap();
        merge_missing_fields_from_default(&mut current, &default_cfg);
        assert_eq!(current.health_check_args(), ["--version"]);
    }

    #[test]
    fn fallback_launch_switches_command_and_args() {
        let adapter = embedded_default_configs()
            .into_iter()
            .find(|cfg| cfg.id == "python-debugpy")
            .unwrap();
        let resolved = adapter.with_fallback_launch();
        assert_eq!(resolved.command, "py");
        assert_eq!(resolved.args, ["-3", "-m", "debugpy.adapter"]);
        assert_eq!(adapter.command, "python");
    }

    #[test]
    fn fallback_launch_keeps_args_when_unset() {
        let adapter = parse_adapter_config(
            r#"
            id = "x"
            display_name = "X"
            command = "python"
            args = ["-m", "debugpy.adapter"]
            health_check_fallback_command = "py"
            "#,
        )
        .unwrap();
        let resolved = adapter.with_fallback_launch();
        assert_eq!(resolved.command, "py");
        assert_eq!(resolved.args, ["-m", "debugpy.adapter"]);
    }
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
