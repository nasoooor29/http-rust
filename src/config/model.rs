use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use serde::Deserialize;

use crate::config::parse::parse_route_key;
use crate::https::HttpMethod;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub schema: Option<String>,
    #[serde(flatten)]
    pub routes: HashMap<String, RouteRule>,
}

impl Config {
    pub fn listener_ports(&self) -> Result<Vec<u16>, String> {
        let mut unique_ports: HashSet<u16> = HashSet::new();

        for route_key in self.routes.keys() {
            let parsed = parse_route_key(route_key)?;
            unique_ports.insert(parsed.port);
        }

        if unique_ports.is_empty() {
            return Err("config has no listener ports".to_string());
        }

        let mut ports: Vec<u16> = unique_ports.into_iter().collect();
        ports.sort_unstable();
        Ok(ports)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum RouteRule {
    FileServer(FileServerConfig),
    Cgi(CgiConfig),
    Redirect(RedirectConfig),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileServerConfig {
    pub root: String,
    pub size_limit: Option<usize>,
    pub error_pages: Option<HashMap<String, String>>,
    pub directory_listing: Option<bool>,
    pub allowed_verbs: Option<Vec<HttpMethod>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CgiConfig {
    pub root: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedirectConfig {
    pub target: String,
}

#[derive(Debug)]
pub struct AppConfig {
    pub config_path: PathBuf,
    pub config: Config,
    pub listener_ports: Vec<u16>,
}
