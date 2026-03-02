use std::collections::HashMap;

use crate::https::{HttpMethod, Request, StatusCode};

use super::Data;

pub(super) fn parse_request(
    header_bytes: &[u8],
    body: &[u8],
) -> Result<Request, (StatusCode, String)> {
    let bad_request = |reason: &str| (StatusCode::BadRequest, reason.to_string());
    let text = std::str::from_utf8(header_bytes)
        .map_err(|_| bad_request("request headers are not valid UTF-8"))?;
    let mut lines = text.split("\r\n");

    let request_line = lines
        .next()
        .ok_or_else(|| bad_request("missing request line"))?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| bad_request("missing HTTP method"))?;
    let raw_path = parts
        .next()
        .ok_or_else(|| bad_request("missing request path"))?;
    let version = parts
        .next()
        .ok_or_else(|| bad_request("missing HTTP version"))?;

    if parts.next().is_some() {
        return Err(bad_request("request line has extra fields"));
    }

    if version != "HTTP/1.1" && version != "HTTP/1.0" {
        return Err((
            StatusCode::VersionNotSupported,
            "unsupported HTTP version".to_string(),
        ));
    }

    let method = HttpMethod::from_str(method);
    if matches!(method, HttpMethod::Post) && body.is_empty() {
        return Err(bad_request("POST request requires a non-empty body"));
    }

    let mut headers = crate::https::HeaderMap::default();
    for line in lines {
        if line.is_empty() {
            break;
        }

        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name, value);
        }
    }

    let (path, query) = raw_path
        .split_once('?')
        .map(|(p, q)| (p.to_string(), q.to_string()))
        .unwrap_or((raw_path.to_string(), String::new()));

    Ok(Request {
        method,
        path,
        query,
        version: version.to_string(),
        headers,
        data: Data {
            body: body.to_vec(),
            path_value: HashMap::new(),
            query_value: HashMap::new(),
            session_id: None,
            is_new_session: false,
        },
    })
}
