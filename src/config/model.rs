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
