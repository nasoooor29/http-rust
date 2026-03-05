mod config;
mod conn;
mod handlers;
mod https;
mod router;
mod utils;

use std::fs;

use crate::config::AppConfig;
use crate::config::model::RouteRule;
use crate::config::parse::parse_route_key;
use crate::handlers::register_routes;
use crate::https::{Request, Response, StatusCode, response_with_body};
use crate::router::{Data, Router};

fn main() {
    let app_config = match AppConfig::load_from_args() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Error loading config: {}", e);
            std::process::exit(1);
        }
    };
    info!("Config loaded", "path" => app_config.config_path.display());

    // TODO: parse it with serde
    // Add validation with clear startup errors (invalid syntax, invalid route options, duplicate/conflicting listen declarations).

    let mut router = Router::new_on_ports(&app_config.listener_ports);
    // TODO: loop over the config and deal with the routes
    // if type is cgi run CGI factory (it can be empty naser will deal with it)
    // if type is dir run the dir serve factory
    // if type is file run the file serve factory
    // if type is redirect run the redirect factory
    register_routes(&mut router);

    for (path, route) in app_config.config.routes.iter() {
        let route_key = parse_route_key(path).unwrap();

        match route {
            RouteRule::FileServer(file_server_config) => router.add_route(
                route_key.port,
                &route_key.path,
                file_server_config.allowed_verbs.unwrap_or_else(|| vec![
                    crate::https::HttpMethod::Get,
                    crate::https::HttpMethod::Post,
                    crate::https::HttpMethod::Delete,
                ]),
                file_server_factory(file_server_config),
            ),
            RouteRule::Cgi(cgi_config) => todo!(),
            RouteRule::Redirect(redirect_config) => todo!(),
        }
    }

    info!("Starting server...");
    info!("Server started on ports", "ports" => format!("{:?}", app_config.listener_ports));
    router.listen_and_serve()
}

fn file_server_factory(
    fs_conf: &config::model::FileServerConfig,
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
