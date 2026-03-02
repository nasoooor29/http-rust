use std::io;
use std::os::fd::RawFd;
use std::time::Instant;

use libc::{EPOLLERR, EPOLLHUP, EPOLLIN, EPOLLOUT, EPOLLRDHUP};

use crate::conn::ConnState;
use crate::handlers::error_response;
use crate::utils::helpers::{
    accept_nonblocking, close_fd, epoll_add, epoll_del, epoll_mod, epoll_wait_blocking,
    recv_nonblocking, send_nonblocking, should_drop,
};

use super::{Conn, IDLE_TIMEOUT, IDLE_TIMEOUT_SECS, ReadOutcome, Router};

impl Router {
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

        let now = Instant::now();
        let timed_out = self.collect_timed_out_conns(now);
        for (fd, local_port) in timed_out {
            eprintln!(
                "dropped client connection fd={fd} on port={local_port} after {IDLE_TIMEOUT_SECS}s of inactivity",
            );
            self.drop_conn(fd);
        }

        super::session::cleanup_expired_sessions(&mut self.sessions, now);

        Ok(())
    }

    fn collect_timed_out_conns(&self, now: Instant) -> Vec<(RawFd, u16)> {
        let mut timed_out = Vec::new();
        for (&fd, conn) in &self.conns {
            if now.duration_since(conn.last_activity) > IDLE_TIMEOUT {
                timed_out.push((fd, conn.local_port));
            }
        }
        timed_out
    }

    fn handle_listen_ready(&mut self, listen_fd: RawFd, listen_port: u16) -> io::Result<()> {
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

                    let mask = (EPOLLIN | EPOLLRDHUP | EPOLLERR | EPOLLHUP) as u32;
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

    fn handle_client_writable(&mut self, fd: RawFd) -> io::Result<()> {
        let mut should_close = false;

        {
            let c = self
                .conns
                .get_mut(&fd)
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "conn missing"))?;

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
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "peer closed"));
                }
                Some(nread) => {
                    let outcome = {
                        let c = self.conns.get_mut(&fd).ok_or_else(|| {
                            io::Error::new(io::ErrorKind::NotFound, "conn missing")
                        })?;
                        c.last_activity = Instant::now();
                        c.read_outcome(&buf[..nread])
                    };

                    let response = match outcome {
                        ReadOutcome::Pending => continue,
                        ReadOutcome::Ready(parts) => {
                            match super::request_parsing::parse_request(
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

                    let c = self
                        .conns
                        .get_mut(&fd)
                        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "conn missing"))?;
                    c.out_buf.extend_from_slice(&response.to_bytes());
                    c.state = ConnState::Responding;

                    let mask = (EPOLLIN | EPOLLOUT | EPOLLRDHUP | EPOLLERR | EPOLLHUP) as u32;
                    epoll_mod(self.epfd, fd, mask)?;
                    break;
                }
                None => break,
            }
        }

        Ok(())
    }
}
