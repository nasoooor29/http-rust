use crate::config::model::{
    Config, FileServerConfig, RedirectConfig, RouteRule,
};
use crate::config::parse::parse_route_key;

impl Config {
    pub fn validate(&self) -> Result<(), String> {
        let mut errors: Vec<String> = Vec::new();

        if self.routes.is_empty() {
            errors.push("config has no routes".to_string());
        }
        let mut existing_routes = Vec::<String>::new();

        for (route_key, rule) in &self.routes {
            if route_key.trim().is_empty() {
                errors.push("route key cannot be empty".to_string());
                continue;
            }
            if existing_routes.contains(route_key) {
                errors.push(format!("duplicate route key: '{route_key}'"));
                continue;
            }
            existing_routes.push(route_key.clone());

            if let Err(err) = parse_route_key(route_key) {
                errors.push(err);
                continue;
            }

            match rule {
                RouteRule::FileServer(cfg) => {
                    validate_file_server(route_key, cfg, &mut errors);
                }
                RouteRule::Cgi(cfg) => {
                    if cfg.root.trim().is_empty() {
                        errors.push(format!(
                            "route '{route_key}' (cgi): 'root' is required and must be non-empty"
                        ));
                    }
                }
                RouteRule::Redirect(cfg) => {
                    validate_redirect(route_key, cfg, &mut errors);
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            let mut out = String::from("config validation failed:\n");
            for err in errors {
                out.push_str(" - ");
                out.push_str(&err);
                out.push('\n');
            }
            Err(out)
        }
    }
}

fn validate_file_server(
    route_key: &str,
    cfg: &FileServerConfig,
    errors: &mut Vec<String>,
) {
    if cfg.root.trim().is_empty() {
        errors.push(format!(
            "route '{route_key}' (file_server): 'root' must be non-empty"
        ));
    }

    if let Some(limit) = cfg.size_limit {
        if limit == 0 {
            errors.push(format!(
                "route '{route_key}' (file_server): 'size_limit' must be > 0"
            ));
        }
    }

    if let Some(error_pages) = &cfg.error_pages {
        for code_str in error_pages.keys() {
            match code_str.parse::<u16>() {
                Ok(code) if (400..=599).contains(&code) => {}
                Ok(code) => errors.push(format!(
                    "route '{route_key}' (file_server): error_pages key '{code}' must be in 400..=599"
                )),
                Err(_) => errors.push(format!(
                    "route '{route_key}' (file_server): error_pages key '{code_str}' is not a valid status code"
                )),
            }
        }
    }
}

fn validate_redirect(
    route_key: &str,
    cfg: &RedirectConfig,
    errors: &mut Vec<String>,
) {
    let target = cfg.target.trim();
    if target.is_empty() {
        errors.push(format!(
            "route '{route_key}' (redirect): 'target' is required and must be non-empty"
        ));
        return;
    }

    if !(target.starts_with("http://") || target.starts_with("https://")) {
        errors.push(format!(
            "route '{route_key}' (redirect): 'target' must start with http:// or https://"
        ));
    }
}
