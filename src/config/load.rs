use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::model::{AppConfig, Config};

impl Config {
    fn load_from_path(path: &Path) -> Result<Self, String> {
        let raw = fs::read_to_string(path)
            .map_err(|e| format!("failed to read config '{}': {e}", path.display()))?;

        serde_json::from_str::<Config>(&raw)
            .map_err(|e| format!("failed to parse config '{}': {e}", path.display()))
    }
}

impl AppConfig {
    pub fn load_from_args() -> Result<Self, String> {
        let config_path = resolve_config_path()?;
        Self::from_path(config_path)
    }

    fn from_path(config_path: PathBuf) -> Result<Self, String> {
        let config = Config::load_from_path(&config_path)?;
        Ok(Self {
            config_path,
            config,
        })
    }
}

fn resolve_config_path() -> Result<PathBuf, String> {
    let mut args = env::args().skip(1);
    let mut config_path = PathBuf::from("config.jsonc");

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-f" | "--config" => {
                let path = args
                    .next()
                    .ok_or_else(|| format!("{arg} requires a file path"))?;
                config_path = PathBuf::from(path);
            }
            "-h" | "--help" => {
                return Err("usage: cargo run -- [-f|--config <path>]".to_string());
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }

    Ok(config_path)
}
