mod helpers;
use crate::helpers::should_drop;
use crate::helpers::{
    accept_nonblocking, create_listen_socket, drop_conn, epoll_add, epoll_mod, last_err,
    recv_nonblocking, send_nonblocking,
};
use libc::{EPOLLERR, EPOLLHUP, EPOLLIN, EPOLLOUT, EPOLLRDHUP, epoll_event};
use std::collections::HashMap;
use std::io;
use std::mem;
use std::os::unix::io::RawFd;

#[derive(Debug)]
struct Conn {
    out_buf: Vec<u8>,
}

fn main() -> io::Result<()> {
    let port: u16 = 9090;

    let listen_fd = create_listen_socket(port)?;
    println!("listening on 0.0.0.0:{port}");

    let epfd = create_epoll()?;
    epoll_add(epfd, listen_fd, EPOLLIN as u32)?;

    let mut conns: HashMap<RawFd, Conn> = HashMap::new();
    let mut events: Vec<epoll_event> = vec![unsafe { mem::zeroed() }; 128];

    loop {
        let n = epoll_wait_blocking(epfd, &mut events)?;
        for ev in events.iter().take(n) {
            let fd = ev.u64 as RawFd;
            let flags = ev.events;

            // 1) Listening socket => accept new clients
            if fd == listen_fd {
                handle_listen_ready(epfd, listen_fd, &mut conns)?;
                continue;
            }

            // 2) Client socket => inline logic here (no handle_client_event)
            if should_drop(flags) {
                drop_conn(epfd, fd, &mut conns);
                continue;
            }

            if (flags & (EPOLLIN as u32)) != 0 {
                if let Err(e) = handle_client_readable(epfd, fd, &mut conns) {
                    eprintln!("read error fd={fd}: {e}");
                    drop_conn(epfd, fd, &mut conns);
                    continue;
                }
            }

            if (flags & (EPOLLOUT as u32)) != 0 {
                if let Err(e) = handle_client_writable(epfd, fd, &mut conns) {
                    eprintln!("write error fd={fd}: {e}");
                    drop_conn(epfd, fd, &mut conns);
                    continue;
                }
            }
        }
    }
}

// ------------------------- helpers -------------------------

fn create_epoll() -> io::Result<RawFd> {
    let epfd = unsafe { libc::epoll_create1(0) };
    if epfd < 0 {
        return Err(last_err("epoll_create1"));
    }
    Ok(epfd)
}

fn epoll_wait_blocking(epfd: RawFd, events: &mut [epoll_event]) -> io::Result<usize> {
    loop {
        let n = unsafe { libc::epoll_wait(epfd, events.as_mut_ptr(), events.len() as i32, -1) };
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
    conns: &mut HashMap<RawFd, Conn>,
) -> io::Result<()> {
    loop {
        match accept_nonblocking(listen_fd) {
            Ok(Some(client_fd)) => {
                conns.insert(
                    client_fd,
                    Conn {
                        out_buf: Vec::new(),
                    },
                );

                let mask = (EPOLLIN | EPOLLRDHUP | EPOLLERR | EPOLLHUP) as u32;
                epoll_add(epfd, client_fd, mask)?;
            }
            Ok(None) => break, // EAGAIN/EWOULDBLOCK
            Err(e) => {
                eprintln!("accept error: {e}");
                break;
            }
        }
    }
    Ok(())
}

fn handle_client_readable(
    epfd: RawFd,
    fd: RawFd,
    conns: &mut HashMap<RawFd, Conn>,
) -> io::Result<()> {
    let mut buf = [0u8; 4096];

    loop {
        match recv_nonblocking(fd, &mut buf)? {
            Some(0) => {
                // peer closed
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "peer closed"));
            }
            Some(nread) => {
                // respond with the length of this read chunk
                let reply = format!("{nread}\n");

                let c = conns
                    .get_mut(&fd)
                    .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "conn missing"))?;
                c.out_buf.extend_from_slice(reply.as_bytes());

                // If we have pending output, ensure EPOLLOUT is enabled.
                if !c.out_buf.is_empty() {
                    let mask = (EPOLLIN | EPOLLOUT | EPOLLRDHUP | EPOLLERR | EPOLLHUP) as u32;
                    epoll_mod(epfd, fd, mask)?;
                }
            }
            None => break, // EAGAIN => no more data right now
        }
    }

    Ok(())
}

fn handle_client_writable(
    epfd: RawFd,
    fd: RawFd,
    conns: &mut HashMap<RawFd, Conn>,
) -> io::Result<()> {
    let c = conns
        .get_mut(&fd)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "conn missing"))?;

    while !c.out_buf.is_empty() {
        match send_nonblocking(fd, &c.out_buf)? {
            Some(nsent) => {
                c.out_buf.drain(..nsent);
            }
            None => break, // EAGAIN => can't write more now
        }
    }

    // If buffer is empty, disable EPOLLOUT.
    if c.out_buf.is_empty() {
        let mask = (EPOLLIN | EPOLLRDHUP | EPOLLERR | EPOLLHUP) as u32;
        let _ = epoll_mod(epfd, fd, mask);
    }

    Ok(())
}
