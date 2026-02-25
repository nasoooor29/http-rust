use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

mod conn;
mod helpers;
mod https;
mod router;

use crate::https::{response_with_body, HttpMethod, Request, Response, StatusCode};
use crate::router::{error_response, Data, Router};

fn main() {
    let mut router = Router::new_on_ports(&[8080, 9090]);

    router.add_route(
        8080,
        "/files/:name",
        vec![HttpMethod::Get, HttpMethod::Post, HttpMethod::Delete],
        handle_file_by_name,
    );

    router.add_route(8080, "/", vec![HttpMethod::Get], handle_public_root);
    router.add_route(8080, "/health", vec![HttpMethod::Get], handle_public_health);
    router.add_route(8080, "/upload", vec![HttpMethod::Post], handle_upload);
    router.add_route(
        8080,
        "/upload_thing",
        vec![HttpMethod::Get],
        handle_get_uploaded,
    );

    router.add_route(9090, "/", vec![HttpMethod::Get], handle_admin_root);
    router.add_route(9090, "/health", vec![HttpMethod::Get], handle_admin_health);

    println!("listening on 8080, 9090");
    loop {
        if let Err(err) = router.handle_connections() {
            eprintln!("server loop error: {err}");
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}

fn handle_public_root(req: &Request, data: &Data) -> Response {
    let host = req.headers.get("host").unwrap_or("unknown-host");
    let sid = data.session_id.as_deref().unwrap_or("none");
    let session_kind = if data.is_new_session {
        "new"
    } else {
        "existing"
    };
    let body = format!(
        "<html><body><h1>Public</h1><p>Host: {host}</p><p>Port: 8080</p><p>Session: {sid} ({session_kind})</p></body></html>"
    );

    response_with_body(
        &req.version,
        StatusCode::Ok,
        "text/html; charset=utf-8",
        body.into_bytes(),
    )
}

fn handle_public_health(req: &Request, _data: &Data) -> Response {
    let _ = req.body.len();

    response_with_body(
        &req.version,
        StatusCode::Ok,
        "text/plain; charset=utf-8",
        "PUBLIC_OK".as_bytes().to_vec(),
    )
}

fn handle_admin_root(req: &Request, _data: &Data) -> Response {
    let body = "<html><body><h1>Admin</h1><p>Port: 9090</p></body></html>";

    response_with_body(
        &req.version,
        StatusCode::Ok,
        "text/html; charset=utf-8",
        body.as_bytes().to_vec(),
    )
}

fn handle_admin_health(req: &Request, _data: &Data) -> Response {
    let _ = req.body.len();

    response_with_body(
        &req.version,
        StatusCode::Ok,
        "text/plain; charset=utf-8",
        "ADMIN_OK".as_bytes().to_vec(),
    )
}

fn handle_upload(req: &Request, _data: &Data) -> Response {
    println!(
        "received upload: {} bytes\n{}",
        req.body.len(),
        String::from_utf8_lossy(&req.body)
    );

    if let Err(e) = fs::write("uploaded", &req.body) {
        eprintln!("failed to save uploaded body: {e}");
        return response_with_body(
            &req.version,
            StatusCode::InternalServerError,
            "text/plain; charset=utf-8",
            b"failed to save upload".to_vec(),
        );
    }

    response_with_body(
        &req.version,
        StatusCode::Ok,
        "text/plain; charset=utf-8",
        b"ok".to_vec(),
    )
}

fn handle_get_uploaded(req: &Request, _data: &Data) -> Response {
    println!("  handling get uploaded");
    let body = match fs::read("uploaded") {
        Ok(bytes) => bytes,
        Err(_) => b"no uploaded file".to_vec(),
    };

    response_with_body(
        &req.version,
        StatusCode::Ok,
        "text/plain; charset=utf-8",
        body,
    )
}

fn handle_file_by_name(req: &Request, data: &Data) -> Response {
    let Some(name) = data.path_value.get("name") else {
        return response_with_body(
            &req.version,
            StatusCode::BadRequest,
            "text/plain; charset=utf-8",
            b"missing file name".to_vec(),
        );
    };

    let path = match file_path_from_name(name) {
        Ok(path) => path,
        Err(msg) => {
            return response_with_body(
                &req.version,
                StatusCode::BadRequest,
                "text/plain; charset=utf-8",
                msg.into_bytes(),
            );
        }
    };

    match req.method {
        HttpMethod::Get => handle_file_get(req, &path),
        HttpMethod::Post => handle_file_post(req, &path),
        HttpMethod::Delete => handle_file_delete(req, &path),
        _ => error_response(&req.version, StatusCode::MethodNotAllowed),
    }
}

fn handle_file_get(req: &Request, path: &Path) -> Response {
    match fs::read(path) {
        Ok(bytes) => response_with_body(
            &req.version,
            StatusCode::Ok,
            "application/octet-stream",
            bytes,
        ),
        Err(e) if e.kind() == ErrorKind::NotFound => response_with_body(
            &req.version,
            StatusCode::NotFound,
            "text/plain; charset=utf-8",
            b"file not found".to_vec(),
        ),
        Err(_) => response_with_body(
            &req.version,
            StatusCode::InternalServerError,
            "text/plain; charset=utf-8",
            b"failed to read file".to_vec(),
        ),
    }
}

fn handle_file_post(req: &Request, path: &Path) -> Response {
    if let Err(e) = fs::create_dir_all("data") {
        eprintln!("failed to create data dir: {e}");
        return response_with_body(
            &req.version,
            StatusCode::InternalServerError,
            "text/plain; charset=utf-8",
            b"failed to prepare storage".to_vec(),
        );
    }

    let existed = path.exists();
    match fs::write(path, &req.body) {
        Ok(()) => {
            let status = if existed {
                StatusCode::Ok
            } else {
                StatusCode::Created
            };
            response_with_body(
                &req.version,
                status,
                "text/plain; charset=utf-8",
                b"file saved".to_vec(),
            )
        }
        Err(_) => response_with_body(
            &req.version,
            StatusCode::InternalServerError,
            "text/plain; charset=utf-8",
            b"failed to save file".to_vec(),
        ),
    }
}

fn handle_file_delete(req: &Request, path: &Path) -> Response {
    match fs::remove_file(path) {
        Ok(()) => response_with_body(
            &req.version,
            StatusCode::NoContent,
            "text/plain; charset=utf-8",
            Vec::new(),
        ),
        Err(e) if e.kind() == ErrorKind::NotFound => response_with_body(
            &req.version,
            StatusCode::NotFound,
            "text/plain; charset=utf-8",
            b"file not found".to_vec(),
        ),
        Err(_) => response_with_body(
            &req.version,
            StatusCode::InternalServerError,
            "text/plain; charset=utf-8",
            b"failed to delete file".to_vec(),
        ),
    }
}

fn file_path_from_name(name: &str) -> Result<PathBuf, String> {
    if name.is_empty() {
        return Err("empty file name".to_string());
    }
    if name == "." || name == ".." || name.contains("..") {
        return Err("invalid file name".to_string());
    }
    if name.contains('/') || name.contains('\\') {
        return Err("nested paths are not allowed".to_string());
    }

    Ok(PathBuf::from("data").join(name))
}
