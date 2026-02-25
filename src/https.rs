use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Delete,
    Unknown(String),
}

impl HttpMethod {
    pub fn from_str(s: &str) -> Self {
        match s {
            "GET" => HttpMethod::Get,
            "POST" => HttpMethod::Post,
            "DELETE" => HttpMethod::Delete,
            other => HttpMethod::Unknown(other.to_string()),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum StatusCode {
    Ok,
    Created,
    NoContent,
    BadRequest,
    Forbidden,
    NotFound,
    MethodNotAllowed,
    PayloadTooLarge,
    InternalServerError,
    VersionNotSupported,
}

impl StatusCode {
    pub fn code(self) -> u16 {
        match self {
            StatusCode::Ok => 200,
            StatusCode::BadRequest => 400,
            StatusCode::Created => 201,
            StatusCode::NoContent => 204,
            StatusCode::Forbidden => 403,
            StatusCode::NotFound => 404,
            StatusCode::MethodNotAllowed => 405,
            StatusCode::PayloadTooLarge => 413,
            StatusCode::InternalServerError => 500,
            StatusCode::VersionNotSupported => 505,
        }
    }

    pub fn reason(self) -> String {
        match self {
            StatusCode::Ok => "OK",
            StatusCode::BadRequest => "Bad Request",
            StatusCode::Forbidden => "Forbidden",
            StatusCode::Created => "Created",
            StatusCode::NoContent => "No Content",
            StatusCode::NotFound => "Not Found",
            StatusCode::MethodNotAllowed => "Method Not Allowed",
            StatusCode::PayloadTooLarge => "Payload Too Large",
            StatusCode::InternalServerError => "Internal Server Error",
            StatusCode::VersionNotSupported => "HTTP Version Not Supported",
        }
        .to_string()
    }
}

#[derive(Debug, Default, Clone)]
pub struct HeaderMap {
    headers: HashMap<String, String>,
}

impl HeaderMap {
    pub fn insert(&mut self, name: &str, value: &str) {
        self.headers
            .insert(name.to_ascii_lowercase(), value.trim().to_string());
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(|s| s.as_str())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.headers.iter()
    }
}

#[derive(Debug, Clone)]
pub struct Request {
    pub method: HttpMethod,
    pub path: String,
    pub query: String,
    pub version: String,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Response {
    pub version: String,
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

impl Response {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let status_line = format!(
            "{} {} {}\r\n",
            self.version,
            self.status.code(),
            self.status.reason()
        );
        out.extend_from_slice(status_line.as_bytes());

        for (k, v) in self.headers.iter() {
            let line = format!("{k}: {v}\r\n");
            out.extend_from_slice(line.as_bytes());
        }

        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(&self.body);
        out
    }
}

pub fn response_with_body(
    version: &str,
    status: StatusCode,
    content_type: &str,
    body: Vec<u8>,
) -> Response {
    let mut headers = HeaderMap::default();
    headers.insert("Content-Type", content_type);
    headers.insert("Content-Length", &body.len().to_string());
    headers.insert("Connection", "close");

    Response {
        version: version.to_string(),
        status,
        headers,
        body,
    }
}
