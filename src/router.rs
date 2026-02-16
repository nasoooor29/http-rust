use std::collections::HashMap;
use std::io;
use std::mem;
use std::os::fd::RawFd;

use libc::{EPOLLERR, EPOLLHUP, EPOLLIN, EPOLLOUT, EPOLLRDHUP, epoll_event};

use crate::helpers::{
    accept_nonblocking, create_listen_socket, epoll_add, epoll_del, epoll_mod,
    last_err, recv_nonblocking, send_nonblocking, should_drop,
};
use crate::https::{
    HttpMethod, Request, Response, StatusCode, response_with_body,
};

pub type Handler = fn(&Request) -> Response;

pub struct Route {
    pub methods: Vec<HttpMethod>,
    pub handler: Handler,
}

pub struct Router {
    routes: HashMap<u16, HashMap<String, Route>>,
    epfd: i32,
    conns: HashMap<RawFd, Conn>,
    events: Vec<epoll_event>,
    listen_fd_to_port: HashMap<RawFd, u16>,
}

#[derive(Debug)]
pub struct Conn {
    local_port: u16,
    in_buf: Vec<u8>,
    out_buf: Vec<u8>,
    state: ConnState,
}

#[derive(Debug)]
enum ConnState {
    ReadingHeaders,
    Responding,
}

impl Router {
    pub fn new(port: u16) -> Self {
        Self::new_on_ports(&[port])
    }

    pub fn new_on_ports(ports: &[u16]) -> Self {
        let epfd = create_epoll().unwrap();
        let mut listen_fd_to_port: HashMap<RawFd, u16> = HashMap::new();

        for &port in ports {
            let listen_fd = create_listen_socket(port).unwrap();
            println!("listening on 0.0.0.0:{port}");
            epoll_add(epfd, listen_fd, EPOLLIN as u32).unwrap();
            listen_fd_to_port.insert(listen_fd, port);
        }

        let conns: HashMap<RawFd, Conn> = HashMap::new();
        let events: Vec<epoll_event> = vec![unsafe { mem::zeroed() }; 128];

        Self {
            routes: HashMap::new(),
            epfd,
            conns,
            events,
            listen_fd_to_port,
        }
    }

    pub fn add_route(
        &mut self,
        port: u16,
        path: &str,
        methods: Vec<HttpMethod>,
        handler: Handler,
    ) {
        self.routes
            .entry(port)
            .or_default()
            .insert(path.to_string(), Route { methods, handler });
    }

    pub fn handle(&self, local_port: u16, req: &Request) -> Response {
        match self
            .routes
            .get(&local_port)
            .and_then(|port_routes| port_routes.get(&req.path))
        {
            Some(route) => {
                if route.methods.iter().any(|m| *m == req.method) {
                    (route.handler)(req)
                } else {
                    error_response(&req.version, StatusCode::MethodNotAllowed)
                }
            }
            None => error_response(&req.version, StatusCode::NotFound),
        }
    }

    pub fn handle_connections(&mut self) -> Result<(), io::Error> {
        let n = epoll_wait_blocking(self.epfd, &mut self.events)?;
        for i in 0..n {
            let (fd, flags) = {
                let ev = &self.events[i];
                (ev.u64 as RawFd, ev.events)
            };

            if let Some(&listen_port) = self.listen_fd_to_port.get(&fd) {
                handle_listen_ready(
                    self.epfd,
                    fd,
                    listen_port,
                    &mut self.conns,
                )?;
                continue;
            }

            if should_drop(flags) {
                drop_conn(self.epfd, fd, &mut self.conns);
                continue;
            }

            if (flags & (EPOLLIN as u32)) != 0
                && let Err(e) = self.handle_client_readable(self.epfd, fd)
            {
                eprintln!("read error fd={fd}: {e}");
                drop_conn(self.epfd, fd, &mut self.conns);
                continue;
            }

            if (flags & (EPOLLOUT as u32)) == 0 {
                continue;
            }
            let Err(e) = handle_client_writable(self.epfd, fd, &mut self.conns)
            else {
                continue;
            };
            eprintln!("write error fd={fd}: {e}");
            drop_conn(self.epfd, fd, &mut self.conns);
            continue;
        }

        Ok(())
    }

    fn handle_client_readable(
        &mut self,
        epfd: RawFd,
        fd: RawFd,
    ) -> io::Result<()> {
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
                    let req_bytes = {
                        let c = self.conns.get_mut(&fd).ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::NotFound,
                                "conn missing",
                            )
                        })?;
                        c.in_buf.extend_from_slice(&buf[..nread]);

                        if matches!(c.state, ConnState::ReadingHeaders) {
                            find_header_end(&c.in_buf).map(|header_end| {
                                (c.in_buf[..header_end].to_vec(), c.local_port)
                            })
                        } else {
                            None
                        }
                    };

                    if let Some((req_bytes, local_port)) = req_bytes {
                        let response = match parse_request(&req_bytes) {
                            Ok(req) => self.handle(local_port, &req),
                            Err(status) => error_response("HTTP/1.1", status),
                        };

                        let c = self.conns.get_mut(&fd).ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::NotFound,
                                "conn missing",
                            )
                        })?;
                        c.out_buf.extend_from_slice(&response.to_bytes());
                        c.state = ConnState::Responding;

                        let mask = (EPOLLIN
                            | EPOLLOUT
                            | EPOLLRDHUP
                            | EPOLLERR
                            | EPOLLHUP)
                            as u32;
                        epoll_mod(epfd, fd, mask)?;
                    }
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
            libc::epoll_wait(epfd, events.as_mut_ptr(), events.len() as i32, -1)
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

fn handle_listen_ready(
    epfd: RawFd,
    listen_fd: RawFd,
    listen_port: u16,
    conns: &mut HashMap<RawFd, Conn>,
) -> io::Result<()> {
    loop {
        match accept_nonblocking(listen_fd) {
            Ok(Some(client_fd)) => {
                conns.insert(
                    client_fd,
                    Conn {
                        local_port: listen_port,
                        in_buf: Vec::new(),
                        out_buf: Vec::new(),
                        state: ConnState::ReadingHeaders,
                    },
                );

                let mask = (EPOLLIN | EPOLLRDHUP | EPOLLERR | EPOLLHUP) as u32;
                epoll_add(epfd, client_fd, mask)?;
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
    epfd: RawFd,
    fd: RawFd,
    conns: &mut HashMap<RawFd, Conn>,
) -> io::Result<()> {
    let mut should_close = false;

    {
        let c = conns.get_mut(&fd).ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "conn missing")
        })?;

        while !c.out_buf.is_empty() {
            match send_nonblocking(fd, &c.out_buf)? {
                Some(nsent) => {
                    c.out_buf.drain(..nsent);
                }
                None => break,
            }
        }

        if c.out_buf.is_empty() {
            should_close = true;
        }
    }

    if should_close {
        drop_conn(epfd, fd, conns);
    }

    Ok(())
}

fn drop_conn(epfd: RawFd, fd: RawFd, conns: &mut HashMap<RawFd, Conn>) {
    epoll_del(epfd, fd);
    conns.remove(&fd);
    unsafe { libc::close(fd) };
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
}

fn parse_request(buf: &[u8]) -> Result<Request, StatusCode> {
    let text = std::str::from_utf8(buf).map_err(|_| StatusCode::BadRequest)?;
    let mut lines = text.split("\r\n");

    let request_line = lines.next().ok_or(StatusCode::BadRequest)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().ok_or(StatusCode::BadRequest)?;
    let raw_path = parts.next().ok_or(StatusCode::BadRequest)?;
    let version = parts.next().ok_or(StatusCode::BadRequest)?;

    if parts.next().is_some() {
        return Err(StatusCode::BadRequest);
    }

    if version != "HTTP/1.1" && version != "HTTP/1.0" {
        return Err(StatusCode::VersionNotSupported);
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

    let path = raw_path
        .split_once('?')
        .map(|(p, _)| p)
        .unwrap_or(raw_path)
        .to_string();

    Ok(Request {
        method: HttpMethod::from_str(method),
        path,
        version: version.to_string(),
        headers,
        body: Vec::new(),
    })
}
