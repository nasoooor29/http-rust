use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::https::HttpMethod;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub schema: Option<String>,
    #[serde(flatten)]
    pub routes: HashMap<String, RouteRule>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RouteRule {
    FileServer(FileServerConfig),
    Cgi(CgiConfig),
    Redirect(RedirectConfig),
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileServerConfig {
    pub root: Option<String>,
    pub size_limit: Option<usize>,
    pub error_pages: Option<HashMap<String, String>>,
    pub directory_listing: Option<bool>,
    pub allowed_verbs: Option<Vec<HttpMethod>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CgiConfig {
    pub root: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RedirectConfig {
    pub target: String,
}

#[derive(Debug)]
pub struct AppConfig {
    pub config_path: PathBuf,
    pub config: Config,
}

pub fn load_from_args() -> Result<AppConfig, String> {
    let config_path = resolve_config_path()?;
    let config = load_config_from_path(&config_path)?;

    Ok(AppConfig {
        config_path,
        config,
    })
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
                return Err(
                    "usage: cargo run -- [-f|--config <path>]".to_string()
                );
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }

    Ok(config_path)
}

fn load_config_from_path(path: &Path) -> Result<Config, String> {
    let raw = fs::read_to_string(path).map_err(|e| {
        format!("failed to read config '{}': {e}", path.display())
    })?;

    serde_json::from_str::<Config>(&raw).map_err(|e| {
        format!("failed to parse config '{}': {e}", path.display())
    })
}
