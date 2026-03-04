use std::collections::HashMap;
use std::path::PathBuf;

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
}
