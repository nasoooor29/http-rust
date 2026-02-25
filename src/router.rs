use std::collections::HashMap;
use std::io;
use std::mem;
use std::os::fd::RawFd;
use std::time::{Duration, Instant};

use libc::{EPOLLERR, EPOLLHUP, EPOLLIN, EPOLLOUT, EPOLLRDHUP, epoll_event};
use rand::RngCore;
use rand::rngs::OsRng;

use crate::conn::Conn;
use crate::conn::ConnState;
use crate::helpers::{
    accept_nonblocking, close_fd, create_listen_socket, epoll_add, epoll_del,
    epoll_mod, last_err, recv_nonblocking, send_nonblocking, should_drop,
};
use crate::https::{
    HttpMethod, Request, Response, StatusCode, response_with_body,
};

const EPOLL_WAIT_MS: i32 = 1000;
const IDLE_TIMEOUT_SECS: u64 = 10; // NOTE: for testing I set it to 10seconds
const IDLE_TIMEOUT: Duration = Duration::from_secs(IDLE_TIMEOUT_SECS);

const SESSION_TTL_SECS: u64 = 60 * 30;
const SESSION_TTL: Duration = Duration::from_secs(SESSION_TTL_SECS);

pub type Handler = fn(&Request, &Data) -> Response;

pub struct Data {
    pub path_value: HashMap<String, String>,
    pub query_value: HashMap<String, String>,
    pub header_value: HashMap<String, String>,
    pub session_id: Option<String>,
    pub is_new_session: bool,
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
            match create_listen_socket(port) {
                Ok(listen_fd) => {
                    println!("listening on 0.0.0.0:{port}");
                    if let Err(err) = epoll_add(epfd, listen_fd, EPOLLIN as u32)
                    {
                        eprintln!(
                            "could not register listener on port {port} in epoll: {err}"
                        );
                        close_fd(listen_fd);
                        continue;
                    }
                    listen_fd_to_port.insert(listen_fd, port);
                }
                Err(err) => {
                    println!(
                        "could not create a listener on port: {port}, error: {err}"
                    );
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

    pub fn add_route(
        &mut self,
        port: u16,
        pattern: &str,
        methods: Vec<HttpMethod>,
        handler: Handler,
    ) {
        self.routes.entry(port).or_default().push(Route {
            methods,
            pattern: pattern.to_string(),
            handler,
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
                let Some(path_value) = match_pattern(&route.pattern, &req.path)
                else {
                    continue;
                };

                if !route.methods.iter().any(|m| *m == req.method) {
                    matched_path_but_wrong_method = true;
                    continue;
                }

                found = Some((route.handler, path_value));
                break;
            }

            (found, matched_path_but_wrong_method)
        };

        let (found, matched_path_but_wrong_method) = match_result;
        let Some((handler, path_value)) = found else {
            if matched_path_but_wrong_method {
                return error_response(
                    &req.version,
                    StatusCode::MethodNotAllowed,
                );
            }
            return error_response(&req.version, StatusCode::NotFound);
        };

        let now = Instant::now();
        let (session_id, is_new_session) =
            resolve_session(&mut self.sessions, req, now);

        let data = Data {
            path_value,
            query_value: parse_query(&req.query),
            header_value: collect_headers(req),
            session_id: session_id.clone(),
            is_new_session,
        };

        let mut resp = handler(req, &data);

        if is_new_session && let Some(sid) = session_id {
            let cookie = format!("sid={sid}; Path=/; HttpOnly; SameSite=Lax");
            resp.headers.insert("Set-Cookie", &cookie);
        }

        resp
    }

    pub fn handle_connections(&mut self) -> Result<(), io::Error> {
        let n = epoll_wait_blocking(self.epfd, &mut self.events)?;
        for i in 0..n {
            let (fd, flags) = {
                let ev = &self.events[i];
                (ev.u64 as RawFd, ev.events)
            };

            if let Some(&listen_port) = self.listen_fd_to_port.get(&fd) {
                self.handle_listen_ready(fd, listen_port)?;
                continue;
            }

            if should_drop(flags) {
                self.drop_conn(fd);
                continue;
            }

            if (flags & (EPOLLIN as u32)) != 0
                && let Err(e) = self.handle_client_readable(fd)
            {
                eprintln!("read error fd={fd}: {e}");
                self.drop_conn(fd);
                continue;
            }

            if (flags & (EPOLLOUT as u32)) == 0 {
                continue;
            }
            let Err(e) = self.handle_client_writable(fd) else {
                continue;
            };
            eprintln!("write error fd={fd}: {e}");
            self.drop_conn(fd);
            continue;
        }

        // NOTE: handle timeout after pocessing all epoll events
        let now = Instant::now();
        let timed_out = collect_timed_out_conns(&self.conns, now);
        for (fd, local_port) in timed_out {
            eprintln!(
                "dropped client connection fd={fd} on port={local_port} after {IDLE_TIMEOUT_SECS}s of inactivity",
            );
            self.drop_conn(fd);
        }

        cleanup_expired_sessions(&mut self.sessions, now);

        Ok(())
    }

    fn handle_listen_ready(
        &mut self,
        // epfd: RawFd,
        listen_fd: RawFd,
        listen_port: u16,
        // conns: &mut HashMap<RawFd, Conn>,
    ) -> io::Result<()> {
        loop {
            match accept_nonblocking(listen_fd) {
                Ok(Some(client_fd)) => {
                    self.conns.insert(
                        client_fd,
                        Conn {
                            local_port: listen_port,
                            in_buf: Vec::new(),
                            out_buf: Vec::new(),
                            state: ConnState::ReadingHeaders,
                            last_activity: Instant::now(),
                        },
                    );

                    let mask =
                        (EPOLLIN | EPOLLRDHUP | EPOLLERR | EPOLLHUP) as u32;
                    epoll_add(self.epfd, client_fd, mask)?;
                }
                Ok(None) => break,
                Err(e) => {
                    eprintln!("accept error: {e}");
                    break;
                }
            }
        }
        Ok(())
    }

    fn handle_client_writable(
        &mut self,
        // epfd: RawFd,
        fd: RawFd,
        // conns: &mut HashMap<RawFd, Conn>,
    ) -> io::Result<()> {
        let mut should_close = false;

        {
            let c = self.conns.get_mut(&fd).ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "conn missing")
            })?;

            while !c.out_buf.is_empty() {
                match send_nonblocking(fd, &c.out_buf)? {
                    Some(nsent) => {
                        c.out_buf.drain(..nsent);
                        c.last_activity = Instant::now();
                    }
                    None => break,
                }
            }

            if c.out_buf.is_empty() {
                should_close = true;
            }
        }

        if should_close {
            self.drop_conn(fd);
        }

        Ok(())
    }

    fn drop_conn(&mut self, fd: RawFd) {
        epoll_del(self.epfd, fd);
        self.conns.remove(&fd);
        close_fd(fd);
    }

    fn handle_client_readable(&mut self, fd: RawFd) -> io::Result<()> {
        let mut buf = [0u8; 4096];

        loop {
            match recv_nonblocking(fd, &mut buf)? {
                Some(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "peer closed",
                    ));
                }
                Some(nread) => {
                    let outcome = {
                        let c = self.conns.get_mut(&fd).ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::NotFound,
                                "conn missing",
                            )
                        })?;
                        c.last_activity = Instant::now();
                        c.read_outcome(&buf[..nread])
                    };

                    let response = match outcome {
                        ReadOutcome::Pending => continue,
                        ReadOutcome::Ready(parts) => {
                            match parse_request(
                                &parts.header_bytes,
                                &parts.body_bytes,
                            ) {
                                Ok(req) => self.handle(parts.local_port, &req),
                                Err((status, reason)) => {
                                    eprintln!("request rejected: {reason}");
                                    error_response("HTTP/1.1", status)
                                }
                            }
                        }
                        ReadOutcome::Error { status, reason } => {
                            eprintln!("request rejected: {reason}");
                            error_response("HTTP/1.1", status)
                        }
                    };

                    let c = self.conns.get_mut(&fd).ok_or_else(|| {
                        io::Error::new(io::ErrorKind::NotFound, "conn missing")
                    })?;
                    c.out_buf.extend_from_slice(&response.to_bytes());
                    c.state = ConnState::Responding;

                    let mask =
                        (EPOLLIN | EPOLLOUT | EPOLLRDHUP | EPOLLERR | EPOLLHUP)
                            as u32;
                    epoll_mod(self.epfd, fd, mask)?;
                    break;
                }
                None => break,
            }
        }

        Ok(())
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
    response_with_body(version, status, "text/html; charset=utf-8", body)
}

fn create_epoll() -> io::Result<RawFd> {
    let epfd = unsafe { libc::epoll_create1(0) };
    if epfd < 0 {
        return Err(last_err("epoll_create1"));
    }
    Ok(epfd)
}

fn epoll_wait_blocking(
    epfd: RawFd,
    events: &mut [epoll_event],
) -> io::Result<usize> {
    loop {
        let n = unsafe {
            libc::epoll_wait(
                epfd,
                events.as_mut_ptr(),
                events.len() as i32,
                EPOLL_WAIT_MS,
            )
        };
        if n < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(last_err("epoll_wait"));
        }
        return Ok(n as usize);
    }
}

fn parse_request(
    header_bytes: &[u8],
    body: &[u8],
) -> Result<Request, (StatusCode, String)> {
    let bad_request =
        |reason: &str| (StatusCode::BadRequest, reason.to_string());
    let text = std::str::from_utf8(header_bytes)
        .map_err(|_| bad_request("request headers are not valid UTF-8"))?;
    let mut lines = text.split("\r\n");

    let request_line = lines
        .next()
        .ok_or_else(|| bad_request("missing request line"))?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| bad_request("missing HTTP method"))?;
    let raw_path = parts
        .next()
        .ok_or_else(|| bad_request("missing request path"))?;
    let version = parts
        .next()
        .ok_or_else(|| bad_request("missing HTTP version"))?;

    if parts.next().is_some() {
        return Err(bad_request("request line has extra fields"));
    }

    if version != "HTTP/1.1" && version != "HTTP/1.0" {
        return Err((
            StatusCode::VersionNotSupported,
            "unsupported HTTP version".to_string(),
        ));
    }

    let method = HttpMethod::from_str(method);
    if matches!(method, HttpMethod::Post) && body.is_empty() {
        return Err(bad_request("POST request requires a non-empty body"));
    }

    let mut headers = crate::https::HeaderMap::default();
    for line in lines {
        if line.is_empty() {
            break;
        }

        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name, value);
        }
    }

    let (path, query) = raw_path
        .split_once('?')
        .map(|(p, q)| (p.to_string(), q.to_string()))
        .unwrap_or((raw_path.to_string(), String::new()));

    Ok(Request {
        method,
        path,
        query,
        version: version.to_string(),
        headers,
        body: body.to_vec(),
    })
}

fn match_pattern(
    pattern: &str,
    req_path: &str,
) -> Option<HashMap<String, String>> {
    let p = pattern.trim_matches('/');
    let r = req_path.trim_matches('/');

    let p_segs: Vec<&str> = if p.is_empty() {
        vec![]
    } else {
        p.split('/').collect()
    };
    let r_segs: Vec<&str> = if r.is_empty() {
        vec![]
    } else {
        r.split('/').collect()
    };

    if p_segs.len() != r_segs.len() {
        return None;
    }

    let mut out = HashMap::new();

    for (ps, rs) in p_segs.iter().zip(r_segs.iter()) {
        if let Some(name) = ps.strip_prefix(':') {
            if name.is_empty() {
                return None;
            }
            out.insert(name.to_string(), (*rs).to_string());
            continue;
        }

        if ps != rs {
            return None;
        }
    }

    Some(out)
}

fn parse_query(query: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if query.is_empty() {
        return out;
    }

    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = match pair.split_once('=') {
            Some((k, v)) => (k, v),
            None => (pair, ""),
        };
        if !k.is_empty() {
            out.insert(k.to_string(), v.to_string());
        }
    }

    out
}

fn collect_headers(req: &Request) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for (k, v) in req.headers.iter() {
        out.insert(k.clone(), v.clone());
    }
    out
}

// NOTE: helper function for stale connection sweeper
// Captures both port and last_activity to log more informative message when dropping stale connections
fn collect_timed_out_conns(
    conns: &HashMap<RawFd, Conn>,
    now: Instant,
) -> Vec<(RawFd, u16)> {
    let mut timed_out = Vec::new();
    for (fd, conn) in conns {
        if now.duration_since(conn.last_activity) > IDLE_TIMEOUT {
            timed_out.push((*fd, conn.local_port));
        }
    }
    timed_out
}

fn parse_cookie_header(cookie: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for part in cookie.split(';') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (k, v) = match trimmed.split_once('=') {
            Some((k, v)) => (k.trim(), v.trim()),
            None => continue,
        };
        if !k.is_empty() {
            out.insert(k.to_string(), v.to_string());
        }
    }
    out
}

fn generate_session_id() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn resolve_session(
    sessions: &mut HashMap<String, Session>,
    req: &Request,
    now: Instant,
) -> (Option<String>, bool) {
    let mut cookie_sid: Option<String> = None;

    if let Some(raw_cookie) = req.headers.get("cookie") {
        let cookies = parse_cookie_header(raw_cookie);
        if let Some(sid) = cookies.get("sid") {
            cookie_sid = Some(sid.clone());
        }
    }

    if let Some(sid) = cookie_sid {
        if let Some(sess) = sessions.get_mut(&sid) {
            sess.last_seen = now;
            sess.visits = sess.visits.saturating_add(1);
            return (Some(sid), false);
        }
    }

    let sid = generate_session_id();
    sessions.insert(
        sid.clone(),
        Session {
            id: sid.clone(),
            created_at: now,
            last_seen: now,
            visits: 1,
        },
    );

    (Some(sid), true)
}

fn cleanup_expired_sessions(
    sessions: &mut HashMap<String, Session>,
    now: Instant,
) {
    sessions.retain(|_, s| now.duration_since(s.last_seen) <= SESSION_TTL);
}
