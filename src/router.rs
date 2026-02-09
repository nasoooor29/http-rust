use std::collections::HashMap;

use crate::https::{HeaderMap, HttpMethod, Request, Response, StatusCode};

pub type Handler = fn(&Request) -> Response;

pub struct Route {
    pub methods: Vec<HttpMethod>,
    pub handler: Handler,
}

pub struct Router {
    routes: HashMap<String, Route>,
}

impl Router {
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
        }
    }

    pub fn add_route(&mut self, path: &str, methods: Vec<HttpMethod>, handler: Handler) {
        self.routes
            .insert(path.to_string(), Route { methods, handler });
    }

    pub fn handle(&self, req: &Request) -> Response {
        match self.routes.get(&req.path) {
            Some(route) => {
                if route.methods.iter().any(|m| *m == req.method) {
                    (route.handler)(req)
                } else {
                    error_response(&req.version, StatusCode::MethodNotAllowed)
                }
            }
            None => error_response(&req.version, StatusCode::NotFound),
        }
    }
}

pub fn error_response(version: &str, status: StatusCode) -> Response {
    let reason = status.reason();
    let body = format!(
        "<html><body><h1>{} {}</h1></body></html>",
        status.code(),
        reason
    )
    .into_bytes();

    let mut headers = HeaderMap::default();
    headers.insert("Content-Type", "text/html; charset=utf-8");
    headers.insert("Content-Length", &body.len().to_string());
    headers.insert("Connection", "close");

    Response {
        version: version.to_string(),
        status,
        headers,
        body,
    }
}
