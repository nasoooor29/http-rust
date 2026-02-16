use std::{io, mem, net::Ipv4Addr, os::fd::RawFd};

fn is_would_block(e: &io::Error) -> bool {
    matches!(
        e.raw_os_error(),
        Some(code) if code == libc::EAGAIN || code == libc::EWOULDBLOCK
    )
}

pub fn accept_nonblocking(listen_fd: RawFd) -> io::Result<Option<RawFd>> {
    // accept4 with libc::SOCK_NONBLOCK so the client libc::socket is nonblocking too.
    let fd = unsafe {
        libc::accept4(
            listen_fd,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            libc::SOCK_NONBLOCK,
        )
    };
    if fd < 0 {
        let e = io::Error::last_os_error();
        if is_would_block(&e) { Ok(None) } else { Err(e) }
    } else {
        Ok(Some(fd))
    }
}

pub fn recv_nonblocking(fd: RawFd, buf: &mut [u8]) -> io::Result<Option<usize>> {
    let n = unsafe { libc::recv(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len(), 0) };
    if n < 0 {
        let e = io::Error::last_os_error();
        if is_would_block(&e) { Ok(None) } else { Err(e) }
    } else {
        Ok(Some(n as usize))
    }
}

pub fn send_nonblocking(fd: RawFd, buf: &[u8]) -> io::Result<Option<usize>> {
    // MSG_NOSIGNAL avoids SIGPIPE on some systems when peer closed.
    let n = unsafe {
        libc::send(
            fd,
            buf.as_ptr() as *const libc::c_void,
            buf.len(),
            libc::MSG_NOSIGNAL,
        )
    };
    if n < 0 {
        let e = io::Error::last_os_error();
        if is_would_block(&e) { Ok(None) } else { Err(e) }
    } else {
        Ok(Some(n as usize))
    }
}

pub fn epoll_add(epfd: RawFd, fd: RawFd, events: u32) -> io::Result<()> {
    let mut ev: libc::epoll_event = unsafe { mem::zeroed() };
    ev.events = events;
    ev.u64 = fd as u64;

    let rc = unsafe { libc::epoll_ctl(epfd, libc::EPOLL_CTL_ADD, fd, &mut ev as *mut _) };
    if rc < 0 {
        return Err(last_err("epoll_ctl(ADD)"));
    }
    Ok(())
}

pub fn epoll_mod(epfd: RawFd, fd: RawFd, events: u32) -> io::Result<()> {
    let mut ev: libc::epoll_event = unsafe { mem::zeroed() };
    ev.events = events;
    ev.u64 = fd as u64;

    let rc = unsafe { libc::epoll_ctl(epfd, libc::EPOLL_CTL_MOD, fd, &mut ev as *mut _) };
    if rc < 0 {
        return Err(last_err("epoll_ctl(MOD)"));
    }
    Ok(())
}

pub fn epoll_del(epfd: RawFd, fd: RawFd) {
    // For DEL, event is ignored (can be null).
    unsafe {
        libc::epoll_ctl(epfd, libc::EPOLL_CTL_DEL, fd, std::ptr::null_mut());
    }
}

pub fn last_err(ctx: &str) -> io::Error {
    io::Error::new(
        io::Error::last_os_error().kind(),
        format!("{ctx}: {}", io::Error::last_os_error()),
    )
}

pub fn create_listen_socket(port: u16) -> io::Result<RawFd> {
    let fd = unsafe {
        // libc::SOCK_NONBLOCK here means the listening libc::socket is nonblocking.
        let fd = libc::socket(libc::AF_INET, libc::SOCK_STREAM | libc::SOCK_NONBLOCK, 0);
        if fd < 0 {
            return Err(last_err("libc::socket"));
        }
        fd
    };

    // SO_REUSEADDR so you can restart quickly after Ctrl+C.
    let yes: i32 = 1;
    let rc = unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_REUSEADDR,
            &yes as *const _ as *const libc::c_void,
            mem::size_of::<i32>() as u32,
        )
    };
    if rc < 0 {
        unsafe { libc::close(fd) };
        return Err(last_err("libc::setsockopt(SO_REUSEADDR)"));
    }

    let addr = libc::sockaddr_in {
        sin_family: libc::AF_INET as u16,
        sin_port: port.to_be(), // network byte order
        sin_addr: libc::in_addr {
            s_addr: u32::from(Ipv4Addr::UNSPECIFIED).to_be(), // 0.0.0.0
        },
        sin_zero: [0; 8],
    };

    let rc = unsafe {
        libc::bind(
            fd,
            &addr as *const _ as *const libc::sockaddr,
            mem::size_of::<libc::sockaddr_in>() as u32,
        )
    };
    if rc < 0 {
        unsafe { libc::close(fd) };
        return Err(last_err("bind"));
    }

    let rc = unsafe { libc::listen(fd, 1024) };
    if rc < 0 {
        unsafe { libc::close(fd) };
        return Err(last_err("listen"));
    }

    Ok(fd)
}

pub fn should_drop(flags: u32) -> bool {
    (flags & (libc::EPOLLERR as u32)) != 0
        || (flags & (libc::EPOLLHUP as u32)) != 0
        || (flags & (libc::EPOLLRDHUP as u32)) != 0
}
