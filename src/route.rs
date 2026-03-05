use std::fs;
use std::path::Path;

use crate::config;
use crate::config::AppConfig;
use crate::https::Request;
use crate::https::Response;
use crate::https::StatusCode;

use crate::https::response_with_body;

use crate::router::Data;

use crate::config::model::RouteRule;

use crate::config::parse::parse_route_key;

use crate::router::Router;

pub fn register_routes(app_config: &AppConfig, router: &mut Router) {
    for (path, route) in app_config.config.routes.iter() {
        let route_key = parse_route_key(path).unwrap();
        match route {
            RouteRule::FileServer(file_server_config) => {
                let pp = Path::new(&file_server_config.root);
                // if file_server_config.root.
                let port = route_key.port;
                let pattern = &route_key.path;
                let methods = file_server_config.allowed_verbs.clone().unwrap_or_else(|| {
                    vec![
                        crate::https::HttpMethod::Get,
                        crate::https::HttpMethod::Post,
                        crate::https::HttpMethod::Delete,
                    ]
                });

                if pp.is_dir() {
                    return router.add_route(
                        port,
                        pattern,
                        methods,
                        dir_server_factory(file_server_config.clone()),
                    );
                } else if pp.is_file() {
                    return router.add_route(
                        port,
                        pattern,
                        methods,
                        file_server_factory(file_server_config.clone()),
                    );
                }
                println!(
                    "Warning: FileServer root '{}' does not exist. Skipping route '{}'",
                    file_server_config.root, path
                );
            }
            RouteRule::Cgi(_cgi_config) => todo!(),
            RouteRule::Redirect(_redirect_config) => todo!(),
        }
    }
}

pub fn cgi_factory(
    cgi_config: config::model::CgiConfig,
) -> impl Fn(&Request, &Data) -> Response + Send + Sync {
    move |req: &Request, _data: &Data| -> Response {
        response_with_body(
            &req.version,
            StatusCode::Ok,
            "text/plain; charset=utf-8",
            "cgi not implemented yet".as_bytes().to_vec(),
        )
    }
}
pub fn redirect_factory(
    redirect_config: config::model::RedirectConfig,
) -> impl Fn(&Request, &Data) -> Response + Send + Sync {
    move |req: &Request, _data: &Data| -> Response {
        response_with_body(
            &req.version,
            StatusCode::Ok,
            "text/plain; charset=utf-8",
            format!("Redirecting to {}", redirect_config.target)
                .as_bytes()
                .to_vec(),
        )
    }
}

pub fn file_server_factory(
    fs_conf: config::model::FileServerConfig,
) -> impl Fn(&Request, &Data) -> Response + Send + Sync {
    move |req: &Request, _data: &Data| -> Response {
        println!("  handling get uploaded");
        let body = match fs::read(fs_conf.root.as_str()) {
            Ok(bytes) => bytes,
            Err(_) => b"no uploaded file".to_vec(),
        };
        match fs_conf.directory_listing {
            Some(true) => {
                println!("  file content:\n{}", String::from_utf8_lossy(&body));
            }
            _ => {}
        }

        response_with_body(
            &req.version,
            StatusCode::Ok,
            "text/plain; charset=utf-8",
            body,
        )
    }
}

pub fn dir_server_factory(
    fs_conf: config::model::FileServerConfig,
) -> impl Fn(&Request, &Data) -> Response + Send + Sync {
    move |req: &Request, _data: &Data| -> Response {
        println!("  handling get uploaded");
        let body = match fs::read(fs_conf.root.as_str()) {
            Ok(bytes) => bytes,
            Err(_) => b"no uploaded file".to_vec(),
        };
        match fs_conf.directory_listing {
            Some(true) => {
                println!("  file content:\n{}", String::from_utf8_lossy(&body));
            }
            _ => {}
        }

        response_with_body(
            &req.version,
            StatusCode::Ok,
            "text/plain; charset=utf-8",
            body,
        )
    }
}
