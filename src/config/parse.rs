#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRouteKey {
    pub host: Option<String>,
    pub port: u16,
    pub path: String,
}

pub fn parse_route_key(route_key: &str) -> Result<ParsedRouteKey, String> {
    let key = route_key.trim();
    if key.is_empty() {
        return Err("route key cannot be empty".to_string());
    }

    let (authority, path) = match key.split_once('/') {
        Some((auth, rest)) => (auth, format!("/{rest}")),
        None => (key, "/".to_string()),
    };

    let (host_raw, port_raw) = authority
        .rsplit_once(':')
        .ok_or_else(|| format!("route key '{route_key}' is invalid: missing ':' separator"))?;

    if port_raw.is_empty() {
        return Err(format!("route key '{route_key}' is invalid: missing port"));
    }

    let port = port_raw.parse::<u16>().map_err(|_| {
        format!("route key '{route_key}' is invalid: port '{port_raw}' is not a valid u16")
    })?;

    if port == 0 {
        return Err(format!(
            "route key '{route_key}' is invalid: port must be in 1..=65535"
        ));
    }

    let host = {
        let h = host_raw.trim();
        if h.is_empty() {
            None
        } else {
            Some(h.to_string())
        }
    };

    if !path.starts_with('/') {
        return Err(format!(
            "route key '{route_key}' is invalid: path must start with '/'"
        ));
    }

    Ok(ParsedRouteKey { host, port, path })
}
