mod helpers;
mod https;
mod router;

use crate::https::{HttpMethod, Request, Response, StatusCode, response_with_body};
use crate::router::Router;

fn main() {
    let port: u16 = 9090;

    let mut router = Router::new();
    router.add_route("/", vec![HttpMethod::Get], handle_root);
    router.add_route("/health", vec![HttpMethod::Get], handle_health);

    let _ = router.listen_and_serve(port);
    println!("Server is running on port {port}");
}

fn handle_root(req: &Request) -> Response {
    let host = req.headers.get("host").unwrap_or("unknown-host");
    let body = format!("<html><body><h1>Welcome</h1><p>Host: {host}</p></body></html>");

    response_with_body(
        &req.version,
        StatusCode::Ok,
        "text/html; charset=utf-8",
        body.into_bytes(),
    )
}

fn handle_health(req: &Request) -> Response {
    let _ = req.body.len();

    response_with_body(
        &req.version,
        StatusCode::Ok,
        "text/plain; charset=utf-8",
        "OK".as_bytes().to_vec(),
    )
}
