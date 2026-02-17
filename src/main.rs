mod helpers;
mod https;
mod router;
mod conn;

use crate::https::{HttpMethod, Request, Response, StatusCode, response_with_body};
use crate::router::Router;

fn main() {
    let mut router = Router::new_on_ports(&[8080, 9090]);

    router.add_route(8080, "/", vec![HttpMethod::Get], handle_public_root);
    router.add_route(8080, "/health", vec![HttpMethod::Get], handle_public_health);
    router.add_route(8080, "/upload", vec![HttpMethod::Post], handle_upload);

    router.add_route(9090, "/", vec![HttpMethod::Get], handle_admin_root);
    router.add_route(9090, "/health", vec![HttpMethod::Get], handle_admin_health);

    println!("listening on 8080, 9090");
    loop {
        router.handle_connections().unwrap();
    }
}

fn handle_public_root(req: &Request) -> Response {
    let host = req.headers.get("host").unwrap_or("unknown-host");
    let body =
        format!("<html><body><h1>Public</h1><p>Host: {host}</p><p>Port: 8080</p></body></html>");

    response_with_body(
        &req.version,
        StatusCode::Ok,
        "text/html; charset=utf-8",
        body.into_bytes(),
    )
}

fn handle_public_health(req: &Request) -> Response {
    let _ = req.body.len();

    response_with_body(
        &req.version,
        StatusCode::Ok,
        "text/plain; charset=utf-8",
        "PUBLIC_OK".as_bytes().to_vec(),
    )
}

fn handle_admin_root(req: &Request) -> Response {
    let body = "<html><body><h1>Admin</h1><p>Port: 9090</p></body></html>";

    response_with_body(
        &req.version,
        StatusCode::Ok,
        "text/html; charset=utf-8",
        body.as_bytes().to_vec(),
    )
}

fn handle_admin_health(req: &Request) -> Response {
    let _ = req.body.len();

    response_with_body(
        &req.version,
        StatusCode::Ok,
        "text/plain; charset=utf-8",
        "ADMIN_OK".as_bytes().to_vec(),
    )
}

fn handle_upload(req: &Request) -> Response {
    println!(
        "received upload: {} bytes\n{}",
        req.body.len(),
        String::from_utf8_lossy(&req.body)
    );

    let body = format!("UPLOAD_OK {} bytes", req.body.len());
    response_with_body(
        &req.version,
        StatusCode::Ok,
        "text/plain; charset=utf-8",
        body.into_bytes(),
    )
}
