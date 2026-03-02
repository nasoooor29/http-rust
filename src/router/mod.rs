use std::collections::HashMap;
use crate::log;
use std::mem;
use std::os::fd::RawFd;
use std::sync::Arc;
use std::time::{Duration, Instant};

use libc::{EPOLLIN, epoll_event};

use crate::conn::Conn;
use crate::handlers::error_response;
use crate::https::{HttpMethod, Request, Response, StatusCode};
use crate::info;
use crate::utils::helpers::create_epoll;
use crate::utils::helpers::{close_fd, create_listen_socket, epoll_add};

mod event_loop;
mod request_parsing;
mod route_matching;
mod session;

const IDLE_TIMEOUT_SECS: u64 = 10;
const IDLE_TIMEOUT: Duration = Duration::from_secs(IDLE_TIMEOUT_SECS);

const SESSION_TTL_SECS: u64 = 60 * 30;
const SESSION_TTL: Duration = Duration::from_secs(SESSION_TTL_SECS);

pub type Handler = Arc<dyn Fn(&Request, &Data) -> Response + Send + Sync>;

#[derive(Debug, Clone)]
pub struct Data {
    pub path_value: HashMap<String, String>,
    pub query_value: HashMap<String, String>,
    pub session_id: Option<String>,
    pub is_new_session: bool,
    pub body: Vec<u8>,
}

pub struct Route {
    pub methods: Vec<HttpMethod>,
    pub pattern: String,
    pub handler: Handler,
}

pub struct Router {
    routes: HashMap<u16, Vec<Route>>,
    epfd: i32,
    conns: HashMap<RawFd, Conn>,
    events: Vec<epoll_event>,
    listen_fd_to_port: HashMap<RawFd, u16>,
    sessions: HashMap<String, Session>,
}

#[derive(Debug)]
pub struct Session {
    pub id: String,
    pub created_at: Instant,
    pub last_seen: Instant,
    pub visits: u64,
}

pub struct PendingRequest {
    pub header_bytes: Vec<u8>,
    pub body_bytes: Vec<u8>,
    pub local_port: u16,
}

pub enum ReadOutcome {
    Pending,
    Ready(PendingRequest),
    Error { status: StatusCode, reason: String },
}

impl Router {
    // TODO: change to addresses instead of ports (NO NEED)
    pub fn new_on_ports(ports: &[u16]) -> Self {
        let epfd = match create_epoll() {
            Ok(fd) => fd,
            Err(err) => {
                eprintln!("could not create epoll instance: {err}");
                -1
            }
        };
        let mut listen_fd_to_port: HashMap<RawFd, u16> = HashMap::new();

        for &port in ports {
            // TODO: pass the address here (NO NEED)
            match create_listen_socket(port) {
                Ok(listen_fd) => {
                    info!("listening on 0.0.0.0:{port}");
                    if let Err(err) = epoll_add(epfd, listen_fd, EPOLLIN as u32) {
                        eprintln!("could not register listener on port {port} in epoll: {err}");
                        close_fd(listen_fd);
                        continue;
                    }
                    listen_fd_to_port.insert(listen_fd, port);
                }
                Err(err) => {
                    println!("could not create a listener on port: {port}, error: {err}");
                }
            };
        }

        let conns: HashMap<RawFd, Conn> = HashMap::new();
        let events: Vec<epoll_event> = vec![unsafe { mem::zeroed() }; 128];

        Self {
            routes: HashMap::new(),
            epfd,
            conns,
            events,
            listen_fd_to_port,
            sessions: HashMap::new(),
        }
    }

    pub fn add_route<H>(&mut self, port: u16, pattern: &str, methods: Vec<HttpMethod>, handler: H)
    where
        H: Fn(&Request, &Data) -> Response + Send + Sync + 'static,
    {
        self.routes.entry(port).or_default().push(Route {
            methods,
            pattern: pattern.to_string(),
            handler: Arc::new(handler),
        });
    }

    pub fn handle(&mut self, local_port: u16, req: &Request) -> Response {
        let match_result = {
            let Some(routes) = self.routes.get(&local_port) else {
                return error_response(&req.version, StatusCode::NotFound);
            };

            let mut matched_path_but_wrong_method = false;
            let mut found: Option<(Handler, HashMap<String, String>)> = None;

            for route in routes {
                let Some(path_value) = route_matching::match_pattern(&route.pattern, &req.path)
                else {
                    continue;
                };

                if !route.methods.iter().any(|m| *m == req.method) {
                    matched_path_but_wrong_method = true;
                    continue;
                }

                found = Some((route.handler.clone(), path_value));
                break;
            }

            (found, matched_path_but_wrong_method)
        };

        let (found, matched_path_but_wrong_method) = match_result;
        let Some((handler, path_value)) = found else {
            if matched_path_but_wrong_method {
                return error_response(&req.version, StatusCode::MethodNotAllowed);
            }
            return error_response(&req.version, StatusCode::NotFound);
        };

        let now = Instant::now();
        let (session_id, is_new_session) = session::resolve_session(&mut self.sessions, req, now);

        let data = Data {
            path_value,
            query_value: route_matching::parse_query(&req.query),
            session_id: session_id.clone(),
            is_new_session,
            body: req.data.body.clone(),
        };

        let mut resp = handler(req, &data);

        if is_new_session && let Some(sid) = session_id {
            let cookie = format!("sid={sid}; Path=/; HttpOnly; SameSite=Lax");
            resp.headers.insert("Set-Cookie", &cookie);
        }

        resp
    }
}
