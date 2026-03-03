mod config;
mod conn;
mod handlers;
mod https;
mod router;
mod utils;

use crate::handlers::register_routes;
use crate::router::Router;

fn main() {
    // TODO: read config with default conf location or with -f flag
    // TODO: parse it with serde
    // Add validation with clear startup errors (invalid syntax, invalid route options, duplicate/conflicting listen declarations).

    // TODO: loop over over the ports put them in an array
    let mut router = Router::new_on_ports(&[8080, 9090]);
    // TODO: loop over the config and deal with the routes
    // if type is cgi run CGI factory (it can be empty naser will deal with it)
    // if type is dir run the dir serve factory
    // if type is file run the file serve factory
    // if type is redirect run the redirect factory
    register_routes(&mut router);

    info!("Starting server...");
    info!("Server started on ports 8080 and 9090");
    router.listen_and_serve()
}
