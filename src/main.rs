mod config;
mod conn;
mod handlers;
mod https;
mod route;
mod router;
mod utils;

use crate::{config::AppConfig, router::Router};

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

    route::register_routes(&app_config, &mut router);

    info!("Starting server...");
    info!("Server started on ports", "ports" => format!("{:?}", app_config.listener_ports));
    router.listen_and_serve()
}
