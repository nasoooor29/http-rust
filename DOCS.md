# HTTP Server with Epoll - Complete Guide

## Table of Contents
1. [What is This Project?](#what-is-this-project)
2. [Why Does This Exist?](#why-does-this-exist)
3. [Key Concepts Explained](#key-concepts-explained)
4. [How the Server Works](#how-the-server-works)
5. [Code Walkthrough](#code-walkthrough)
6. [Running the Server](#running-the-server)
7. [Testing the Server](#testing-the-server)
8. [Common Questions](#common-questions)

---

## What is This Project?

This is a **minimal HTTP web server** written in Rust that handles multiple client connections simultaneously. When you visit `http://localhost:8080` in your browser, it responds with "Hello from epoll (libc)!".

What makes this special is that it's built using **low-level system calls** directly, rather than using high-level frameworks like Tokio or async-std. This means you can see exactly how a server works "under the hood."

### What You'll Learn

By studying this project, you'll understand:
- How web servers accept and handle multiple connections at once
- How operating systems notify programs about network events
- How non-blocking I/O works
- The basics of HTTP request/response

---

## Why Does This Exist?

Modern web frameworks hide a lot of complexity. They make building servers easy, but you might not understand what's happening behind the scenes.

This project shows you the fundamentals by:
- Using **epoll** - Linux's efficient way to monitor many network connections
- Making direct **system calls** through Rust's FFI (Foreign Function Interface)
- Avoiding async/await syntax to show the raw mechanics

Think of it like learning to drive a manual transmission car before driving an automatic. You don't need to know this for everyday work, but understanding it makes you a better programmer.

---

## Key Concepts Explained

### What is a TCP Socket?

A **socket** is like a telephone line between two computers. When you want to communicate over the network:
1. The server creates a socket and "listens" for calls
2. A client (like your browser) creates a socket and "calls" the server
3. Once connected, they can send and receive data
4. When done, they "hang up" (close the connection)

### What is Blocking vs Non-Blocking I/O?

**Blocking I/O** (the simple way):
```
connection = accept()  // Wait here until someone connects... could be hours!
data = read()          // Wait here until data arrives... could be forever!
```
Your program stops and waits. This is simple but inefficient.

**Non-Blocking I/O** (the efficient way):
```
connection = try_accept()  // If no one is connecting, return immediately with "not ready"
data = try_read()          // If no data yet, return immediately with "not ready"
```
Your program checks if something is ready and moves on if not. This lets you handle many connections at once.

### What is Epoll?

**Epoll** is Linux's way of monitoring many file descriptors (like sockets) efficiently.

**Without epoll** (inefficient):
```
for each connection:
    check if it has data to read
    check if it's ready for writing
    check if it's closed
// This is slow when you have 10,000 connections!
```

**With epoll** (efficient):
```
epoll tells you: "Connection #5 has data, Connection #42 is ready for writing"
// Only process the connections that are actually ready!
```

Think of it like a receptionist who tells you which phone lines have calls waiting, instead of you checking every phone yourself.

### Edge-Triggered vs Level-Triggered

This is a mode for epoll:

**Level-Triggered** (easier but less efficient):
- Epoll says: "There is data available"
- Even if you don't read it all, epoll will keep telling you on the next check

**Edge-Triggered** (harder but more efficient):
- Epoll says: "New data just arrived!" (only once)
- If you don't read all of it, epoll won't tell you again
- You must read until there's nothing left (you get an EAGAIN error)

This project uses edge-triggered mode for maximum performance.

### What is HTTP?

**HTTP** (HyperText Transfer Protocol) is the language browsers and servers speak.

**Request** (what the browser sends):
```
GET / HTTP/1.1
Host: localhost:8080
```

**Response** (what the server sends back):
```
HTTP/1.1 200 OK
Content-Length: 27
Connection: close

Hello from epoll (libc)!
```

This server keeps it simple - it sends the same response no matter what you request.

---

## How the Server Works

### The Big Picture

```
1. Server starts and listens on port 8080
2. Server creates an epoll instance to monitor connections
3. Main loop begins:
   ↓
   ├─→ epoll_wait() - "Tell me what's happening"
   ├─→ Process events:
   │   ├─→ New connection? Accept it and add to epoll
   │   ├─→ Connection has data? Read it
   │   ├─→ Ready to write? Send HTTP response
   │   └─→ Error or done? Close connection
   └─→ Repeat forever
```

### Connection Lifecycle

```
[Browser connects]
        ↓
    Accept connection
        ↓
    Add to epoll (monitor for incoming data)
        ↓
    [Browser sends HTTP request]
        ↓
    Read request (we don't actually parse it)
        ↓
    Switch to write mode
        ↓
    Send HTTP response
        ↓
    Close connection
```

### State Machine

Each connection goes through states:

1. **Reading State**: Waiting for the browser to send its request
   - We're monitoring for `EPOLLIN` (data available to read)
   
2. **Writing State**: Sending our response back
   - We're monitoring for `EPOLLOUT` (socket ready for writing)
   
3. **Closed**: All done, resources cleaned up

---

## Code Walkthrough

Let's go through the main parts of the code:

### 1. The Connection Structure

```rust
struct Conn {
    fd: c_int,              // File descriptor (the connection's ID)
    out_buf: Vec<u8>,       // The HTTP response we want to send
    written: usize,         // How many bytes we've sent so far
}
```

This tracks each active connection. The server keeps a `HashMap<c_int, Conn>` to remember all connections.

### 2. Creating the Listening Socket

The `make_listener()` function:
1. Creates a socket with `socket()` system call
2. Sets `SO_REUSEADDR` so we can restart the server quickly
3. Binds to `0.0.0.0:8080` (listen on all network interfaces)
4. Calls `listen()` to start accepting connections

### 3. Building the HTTP Response

```rust
fn build_response() -> Vec<u8> {
    let body = "Hello from epoll (libc)!";
    format!(
        "HTTP/1.1 200 OK\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n\
         {}",
        body.len(), body
    ).into_bytes()
}
```

This creates a valid HTTP response with:
- Status line: `HTTP/1.1 200 OK`
- Headers: Content-Length and Connection: close
- Body: Our message

### 4. The Main Event Loop

The loop has four phases:

#### Phase 1: Accept New Connections
```rust
if events[i].events & libc::EPOLLIN != 0 && ev_fd == listen_fd {
    // New connection available!
    loop {
        let conn_fd = accept4(...);
        if conn_fd < 0 { break; }  // No more pending connections
        
        // Add new connection to epoll
        epoll_add(epoll_fd, conn_fd, EPOLLIN | EPOLLRDHUP | ...);
        
        // Create connection state
        conns.insert(conn_fd, Conn { ... });
    }
}
```

#### Phase 2: Handle Errors and Hangups
```rust
if events[i].events & (libc::EPOLLERR | libc::EPOLLHUP) != 0 {
    // Something went wrong or client disconnected
    close_connection(...);
}
```

#### Phase 3: Read Incoming Data
```rust
if events[i].events & libc::EPOLLIN != 0 {
    loop {
        let n = read(fd, buffer, ...);
        if n < 0 {
            if errno == EAGAIN { break; }  // Nothing more to read
            // Error - close connection
        }
        if n == 0 {
            // Client closed connection
        }
    }
    // Done reading, switch to write mode
    epoll_mod(epoll_fd, fd, EPOLLOUT | ...);
}
```

#### Phase 4: Write Response Data
```rust
if events[i].events & libc::EPOLLOUT != 0 {
    let remaining = &conn.out_buf[conn.written..];
    let n = write(fd, remaining, ...);
    
    conn.written += n;
    
    if conn.written == conn.out_buf.len() {
        // Sent everything! Close connection
        close_connection(...);
    }
}
```

### 5. Helper Functions

- `epoll_add()`: Register a file descriptor with epoll
- `epoll_mod()`: Change what events we're monitoring for
- `epoll_del()`: Stop monitoring a file descriptor
- `close_fd()`: Safely close a socket and handle errors

---

## Running the Server

### Prerequisites

- **Linux** (this uses Linux-specific epoll)
- **Rust** (install from https://rustup.rs)

### Build and Run

```bash
# Build the project
cargo build --release

# Run the server
cargo run --release
```

You should see:
```
Listening on 0.0.0.0:8080
```

The server is now running and waiting for connections!

---

## Testing the Server

### Using a Web Browser

1. Open your browser
2. Go to: `http://localhost:8080`
3. You should see: "Hello from epoll (libc)!"

### Using curl

```bash
curl http://localhost:8080
```

Output:
```
Hello from epoll (libc)!
```

### Using telnet

```bash
telnet localhost 8080
```

Then type:
```
GET / HTTP/1.1
Host: localhost

```
(Press Enter twice after the Host line)

You'll see the full HTTP response.

### Load Testing

Test with many concurrent connections:

```bash
# Install Apache Bench
sudo apt-get install apache2-utils

# Send 10000 requests with 100 concurrent connections
ab -n 10000 -c 100 http://localhost:8080/
```

---

## Common Questions

### Q: Why use epoll instead of threads?

**Threads approach**:
- Create one thread per connection
- Each thread blocks waiting for data
- 10,000 connections = 10,000 threads (uses lots of memory!)

**Epoll approach**:
- One thread handles all connections
- Only processes connections when they're ready
- 10,000 connections = minimal overhead

Epoll scales much better for high-concurrency scenarios.

### Q: Why doesn't this use async/await?

This project uses direct system calls to show the underlying mechanics. Rust's async/await is built on top of concepts like epoll (on Linux) or kqueue (on macOS).

When you use `async fn` and `.await`, the compiler and runtime (like Tokio) handle all this epoll complexity for you.

### Q: Why doesn't it parse HTTP requests?

This is a demonstration of the I/O layer, not an HTTP parser. A real server would:
1. Read the request incrementally
2. Parse the method, path, headers
3. Route to appropriate handler
4. Generate dynamic responses

This server just shows the connection handling part.

### Q: Why close the connection after each response?

This uses `Connection: close` for simplicity. Real servers use `Connection: keep-alive` to reuse connections for multiple requests (HTTP keep-alive or HTTP/2), which is more efficient.

### Q: Can I use this in production?

**No!** This is educational code. Production servers should:
- Use battle-tested frameworks (Tokio, Actix, etc.)
- Parse HTTP properly
- Handle errors gracefully
- Support HTTPS/TLS
- Have logging, metrics, and monitoring
- Support multiple threads/cores
- Handle backpressure and resource limits

### Q: Why only Linux?

Epoll is Linux-specific. Other operating systems have equivalents:
- **macOS/BSD**: kqueue
- **Windows**: IOCP (I/O Completion Ports)
- **Cross-platform**: Libraries like mio abstract these differences

### Q: What's the difference between this and Tokio?

**This project**:
- Direct epoll system calls
- Manual state management
- Single-threaded
- ~280 lines of code
- Educational

**Tokio**:
- Abstracts over epoll/kqueue/IOCP
- Automatic state management with async/await
- Multi-threaded work-stealing scheduler
- Timers, channels, sync primitives
- Production-ready
- Millions of lines (with dependencies)

Think of this as "learning how a combustion engine works" and Tokio as "driving a modern car."

### Q: Why use libc instead of pure Rust?

Rust's standard library provides networking, but it uses blocking I/O. For non-blocking epoll, we need to call Linux system calls directly, which requires FFI through the `libc` crate.

Higher-level Rust crates like `mio` wrap these calls safely, but we're going low-level here for learning.

---

## Next Steps

### To Learn More

1. **Read the Linux man pages**:
   ```bash
   man epoll
   man epoll_create
   man epoll_ctl
   man epoll_wait
   ```

2. **Study Rust async**:
   - Look at the `mio` crate (safe epoll wrapper)
   - Read Tokio's documentation
   - Learn about async/await in Rust

3. **Build on this**:
   - Add HTTP request parsing
   - Support multiple routes
   - Add logging
   - Implement keep-alive connections
   - Add TLS support

### Related Projects

- **mio**: Low-level cross-platform I/O (epoll/kqueue/IOCP wrapper)
- **Tokio**: Full async runtime
- **Actix-web**: High-performance web framework
- **Hyper**: HTTP library built on Tokio

---

## Summary

This project demonstrates:
- ✅ Low-level socket programming
- ✅ Non-blocking I/O with epoll
- ✅ Edge-triggered event notification
- ✅ Connection lifecycle management
- ✅ Basic HTTP response

It's a learning tool that shows how servers work at the system call level, giving you insight into what frameworks like Tokio do behind the scenes.

Happy learning! 🚀
