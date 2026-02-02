use libc::*;
use std::collections::HashMap;
use std::io;
use std::mem;
use std::net::Ipv4Addr;
use std::os::fd::RawFd;
use std::ptr;

const MAX_EVENTS: usize = 128;
const READ_BUF_SIZE: usize = 8192;

#[derive(Debug)]
struct Conn {
    fd: RawFd,
    out: Vec<u8>,
    written: usize,
}

fn last_err() -> io::Error {
    io::Error::last_os_error()
}

fn close_fd(fd: RawFd) {
    unsafe {
        close(fd);
    }
}

fn epoll_add(epfd: RawFd, fd: RawFd, events: u32) -> io::Result<()> {
    unsafe {
        let mut ev: epoll_event = mem::zeroed();
        ev.events = events;
        ev.u64 = fd as u64; // stash fd
        if epoll_ctl(epfd, EPOLL_CTL_ADD, fd, &mut ev as *mut _) < 0 {
            return Err(last_err());
        }
    }
    Ok(())
}

fn epoll_mod(epfd: RawFd, fd: RawFd, events: u32) -> io::Result<()> {
    unsafe {
        let mut ev: epoll_event = mem::zeroed();
        ev.events = events;
        ev.u64 = fd as u64;
        if epoll_ctl(epfd, EPOLL_CTL_MOD, fd, &mut ev as *mut _) < 0 {
            return Err(last_err());
        }
    }
    Ok(())
}

fn epoll_del(epfd: RawFd, fd: RawFd) {
    unsafe {
        // For DEL, the event ptr is ignored (can be null)
        epoll_ctl(epfd, EPOLL_CTL_DEL, fd, ptr::null_mut());
    }
}

fn make_listener(port: u16) -> io::Result<RawFd> {
    unsafe {
        let fd = socket(AF_INET, SOCK_STREAM | SOCK_NONBLOCK | SOCK_CLOEXEC, 0);
        if fd < 0 {
            return Err(last_err());
        }

        // reuseaddr
        let yes: c_int = 1;
        if setsockopt(
            fd,
            SOL_SOCKET,
            SO_REUSEADDR,
            &yes as *const _ as *const c_void,
            mem::size_of_val(&yes) as socklen_t,
        ) < 0
        {
            close_fd(fd);
            return Err(last_err());
        }

        let addr = sockaddr_in {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: in_addr {
                s_addr: u32::from(Ipv4Addr::UNSPECIFIED).to_be(),
            },
            sin_zero: [0; 8],
        };

        if bind(
            fd,
            &addr as *const _ as *const sockaddr,
            mem::size_of_val(&addr) as socklen_t,
        ) < 0
        {
            close_fd(fd);
            return Err(last_err());
        }

        if listen(fd, 1024) < 0 {
            close_fd(fd);
            return Err(last_err());
        }

        Ok(fd)
    }
}

fn build_response() -> Vec<u8> {
    let body = b"Hello from epoll (libc)!\n";
    let hdr = format!(
        "HTTP/1.1 200 OK\r\n\
         Connection: close\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         Content-Length: {}\r\n\
         \r\n",
        body.len()
    );
    let mut out = hdr.into_bytes();
    out.extend_from_slice(body);
    out
}

fn main() -> io::Result<()> {
    // Bind 0.0.0.0:8080
    let listen_fd = make_listener(8080)?;
    eprintln!("Listening on http://0.0.0.0:8080");

    // epoll instance
    let epfd = unsafe { epoll_create1(EPOLL_CLOEXEC) };
    if epfd < 0 {
        close_fd(listen_fd);
        return Err(last_err());
    }

    // Monitor listener for incoming connections
    epoll_add(epfd, listen_fd, (EPOLLIN) as u32)?;

    let mut conns: HashMap<RawFd, Conn> = HashMap::new();
    let mut events: Vec<epoll_event> = vec![unsafe { mem::zeroed() }; MAX_EVENTS];

    loop {
        let n = unsafe { epoll_wait(epfd, events.as_mut_ptr(), MAX_EVENTS as c_int, -1) };
        if n < 0 {
            let err = last_err();
            if err.raw_os_error() == Some(EINTR) {
                continue;
            }
            break Err(err);
        }

        for i in 0..(n as usize) {
            let ev = events[i];
            let fd = ev.u64 as RawFd;
            let flags = ev.events;

            if fd == listen_fd {
                // Accept all pending connections (edge-triggered)
                loop {
                    let mut addr: sockaddr_in = unsafe { mem::zeroed() };
                    let mut len: socklen_t = mem::size_of::<sockaddr_in>() as socklen_t;

                    let cfd = unsafe {
                        accept4(
                            listen_fd,
                            &mut addr as *mut _ as *mut sockaddr,
                            &mut len as *mut _,
                            SOCK_NONBLOCK | SOCK_CLOEXEC,
                        )
                    };

                    if cfd < 0 {
                        let err = last_err();
                        match err.raw_os_error() {
                            Some(EAGAIN) | Some(EWOULDBLOCK) => break,
                            _ => {
                                eprintln!("accept error: {err}");
                                break;
                            }
                        }
                    }

                    // Monitor connection for readable + hangup + errors, edge-triggered
                    epoll_add(
                        epfd,
                        cfd,
                        (EPOLLIN | EPOLLRDHUP | EPOLLHUP | EPOLLERR) as u32,
                    )?;

                    // Create conn state with a ready-to-send response
                    conns.insert(
                        cfd,
                        Conn {
                            fd: cfd,
                            out: build_response(),
                            written: 0,
                        },
                    );
                }
                continue;
            }

            // If connection has error/hup, close it
            if (flags & (EPOLLERR as u32)) != 0
                || (flags & (EPOLLHUP as u32)) != 0
                || (flags & (EPOLLRDHUP as u32)) != 0
            {
                epoll_del(epfd, fd);
                conns.remove(&fd);
                close_fd(fd);
                continue;
            }

            // Read available data (we don't parse fully; just drain)
            if (flags & (EPOLLIN as u32)) != 0 {
                let mut buf = [0u8; READ_BUF_SIZE];
                loop {
                    let r = unsafe { read(fd, buf.as_mut_ptr() as *mut c_void, buf.len()) };
                    if r > 0 {
                        // ignore content; in a real server, parse request incrementally
                        continue;
                    } else if r == 0 {
                        // if r == 0 means EOF
                        // peer closed
                        epoll_del(epfd, fd);
                        conns.remove(&fd);
                        close_fd(fd);
                        break;
                    } else {
                        let err = last_err();
                        match err.raw_os_error() {
                            Some(EAGAIN) | Some(EWOULDBLOCK) => break, // drained
                            _ => {
                                eprintln!("read error on {fd}: {err}");
                                epoll_del(epfd, fd);
                                conns.remove(&fd);
                                close_fd(fd);
                                break;
                            }
                        }
                    }
                }

                // Switch interest to writable to send response
                if conns.contains_key(&fd) {
                    epoll_mod(
                        epfd,
                        fd,
                        (EPOLLOUT | EPOLLRDHUP | EPOLLHUP | EPOLLERR) as u32,
                    )?;
                }
            }

            // Write response (edge-triggered: write until EAGAIN)
            if (flags & (EPOLLOUT as u32)) != 0 {
                let done = if let Some(conn) = conns.get_mut(&fd) {
                    loop {
                        if conn.written >= conn.out.len() {
                            break true;
                        }
                        let slice = &conn.out[conn.written..];
                        let w = unsafe { write(fd, slice.as_ptr() as *const c_void, slice.len()) };
                        if w > 0 {
                            conn.written += w as usize;
                            continue;
                        } else if w < 0 {
                            let err = last_err();
                            match err.raw_os_error() {
                                Some(EAGAIN) | Some(EWOULDBLOCK) => break false,
                                _ => {
                                    eprintln!("write error on {fd}: {err}");
                                    break true; // close on error
                                }
                            }
                        } else {
                            // write returned 0: treat as done/close
                            break true;
                        }
                    }
                } else {
                    true
                };

                if done {
                    epoll_del(epfd, fd);
                    conns.remove(&fd);
                    close_fd(fd);
                }
            }
        }
    }
}
