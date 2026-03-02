mod conn;
mod handlers;
mod https;
mod router;
mod utils;

use crate::handlers::register_routes;
use crate::router::Router;

fn main() {
    let mut router = Router::new_on_ports(&[8080, 9090]);
    register_routes(&mut router);

    info!("Starting server...");
    info!("Server started on ports 8080 and 9090");
    loop {
        if let Err(err) = router.handle_connections() {
            eprintln!("server loop error: {err}");
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}
