# AGENTS.md
Guidance for autonomous coding agents working in this repository.

## Scope
- Applies to the whole repository rooted at this file.
- Project: Rust binary crate (`http-rust`) implementing an `epoll`-driven HTTP server.
- Runtime model: one process, one thread, non-blocking I/O via `libc` syscalls.

## Rule Sources and Precedence
- Primary source: this `AGENTS.md`.
- Additional rule files checked in this repository:
  - `.cursor/rules/`: not present
  - `.cursorrules`: not present
  - `.github/copilot-instructions.md`: not present
- If any of those files are added later, treat them as additional constraints and follow the strictest applicable rule.

## Repository Layout
- `src/main.rs`: server bootstrap, listener setup, route registration.
- `src/router/mod.rs`: router state, route registration, request dispatch.
- `src/router/event_loop.rs`: epoll-driven connection/event processing.
- `src/router/request_parsing.rs`: HTTP request parsing and validation.
- `src/router/route_matching.rs`: path/query matching helpers.
- `src/router/session.rs`: session creation/refresh/expiry logic.
- `src/conn.rs`: connection buffers and request framing (`Content-Length`, chunked).
- `src/https.rs`: protocol types (`Request`, `Response`, `StatusCode`, headers).
- `src/utils/helpers.rs`: syscall wrappers (`socket`, `bind`, `listen`, `accept4`, `recv`, `send`, `epoll_ctl`, `epoll_wait`).
- `src/utils/logger.rs`: lightweight logging macros/utilities.
- `src/handlers/mod.rs`: route handlers and route registration helpers.
- `README.md`: project requirements and constraints.
- `AUDIT.md`: audit checklist and expectations.
- `REMAINING.md`: open work and known gaps.

## Build, Lint, Run, and Test Commands
Run all commands from repository root.

### Build / Check
- Debug build: `cargo build`
- Release build: `cargo build --release`
- Type-check only: `cargo check`
- Type-check all targets: `cargo check --all-targets`

### Run
- Run server: `cargo run`
- Run optimized server: `cargo run --release`

### Formatting / Lint
- Check formatting: `cargo fmt --all -- --check`
- Apply formatting: `cargo fmt --all`
- Strict clippy: `cargo clippy --all-targets --all-features -- -D warnings`
- Faster clippy loop: `cargo clippy --all-targets -- -D warnings`

### Tests
- Run all tests: `cargo test`
- Run with output: `cargo test -- --nocapture`
- Run serially (useful for networking/ordering-sensitive tests): `cargo test -- --test-threads=1`
- List tests without running: `cargo test -- --list`

### Running a Single Test (important)
- Unit test by exact name:
  - `cargo test decode_chunked_body_accepts_empty_trailers -- --exact --nocapture`
- Another exact unit test example:
  - `cargo test decode_chunked_body_waits_for_final_crlf -- --exact --nocapture`
- Unit tests by substring:
  - `cargo test decode_chunked_body -- --nocapture`
- Limit to one crate/package (useful in workspaces):
  - `cargo test -p http-rust decode_chunked_body -- --nocapture`
- If integration tests are added later (`tests/*.rs`), run one target:
  - `cargo test --test http_flow`
- If integration tests are added later, run one exact test:
  - `cargo test --test http_flow handles_get_health -- --exact --nocapture`

## Recommended Validation Before Finalizing Changes
- `cargo fmt --all`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test -- --nocapture`
- If networking behavior changed, verify manually with `curl` against both ports (`8080` and `9090`).

## Code Style Guidelines
Follow existing patterns in `src/**/*.rs` and idiomatic Rust.

### Imports
- Order import groups as: `std`, third-party crates, then `crate::...`.
- Keep imports explicit/minimal; remove unused imports instead of silencing warnings.
- Prefer one import block per module unless split blocks materially improve readability.

### Formatting
- Treat `rustfmt` output as the source of truth.
- Use trailing commas in multi-line literals/calls/match arms.
- Prefer multi-line layout for long chains or function calls.
- Do not preserve manual alignment that `rustfmt` will rewrite.

### Types and Data Modeling
- Prefer domain enums/structs (`HttpMethod`, `StatusCode`, `Request`, `Response`) over stringly-typed APIs.
- Use enums for protocol/state machines (connection state, framing mode, outcomes).
- Prefer named struct fields (`local_port`, `in_buf`, `out_buf`) over tuple structs.
- Borrow in hot paths when possible; allocate/clones only when required for correctness.

### Naming Conventions
- `snake_case` for functions, variables, modules.
- `PascalCase` for structs, enums, traits, and type aliases.
- Use verb-first names for side-effecting operations (`listen_and_serve`, `handle_connections`, `drop_conn`).
- Use protocol-accurate terms (`header_end`, `content_length`, `session_id`).

### Error Handling
- Prefer `Result<T, io::Error>` at syscall and I/O boundaries.
- Avoid `unwrap()`/`expect()` in long-running server/runtime code paths.
- Convert recoverable parse/route failures into HTTP responses (`400`, `404`, `405`, etc.).
- Add actionable context to low-level errors (`last_err("...")`, include fd/port when relevant).
- On connection-level failure, cleanly remove fd from epoll and close the socket.

### Unsafe Code
- Keep `unsafe` blocks small, local, and limited to syscall boundaries.
- Add concise safety comments when invariants are non-obvious.
- Prefer safe wrapper functions in `src/utils/helpers.rs` so higher layers stay safe.
- Do not introduce new unsafe blocks when a safe equivalent is practical.

### Event Loop and Networking
- Keep sockets non-blocking.
- Route reads and writes through epoll readiness handling.
- Preserve the single-thread event-loop architecture; do not add request worker threads.
- Correctly handle partial reads/writes and retry on readiness.
- Maintain timeout/session cleanup behavior as part of the event loop lifecycle.

### HTTP Behavior
- Preserve request-line/version validation behavior (support `HTTP/1.0` and `HTTP/1.1`).
- Normalize header names to lowercase in storage/lookup logic.
- Keep status-code behavior aligned with route and method matching semantics.
- Keep response headers/body consistent (`Content-Length`, `Content-Type`, `Connection`).
- Keep chunked decoding logic strict about CRLF and malformed chunk size errors.

### Route and Handler Design
- Keep handlers focused on HTTP/business logic, not epoll/socket management.
- Keep handler signatures aligned with current abstractions (`Fn(&Request, &Data) -> Response`).
- Keep route registration explicit and readable in startup/handler modules.
- Prefer small helper functions over monolithic handlers for non-trivial behavior.

### Logging and Testing
- Use normal logs for startup/info and error logs for failures.
- Include enough context in logs (fd, port, request path) without flooding hot loops.
- Place unit tests near implementation (`#[cfg(test)]`) for parser/protocol logic.
- Add integration tests under `tests/` for end-to-end server flows when introduced.
- Assert protocol details in tests (status line, headers, body bytes), not only broad success/failure.

## Change Management for Agents
- Make focused, minimal patches.
- Avoid unnecessary dependencies, especially async runtimes or full server frameworks.
- Respect project constraints from `README.md` (no `tokio`/`nix`-style server replacement).
- If behavior changes, update tests and this file when command/style guidance changes.

## Quick Start for New Agents
- Read `README.md` and `AUDIT.md` first.
- Review `src/router/mod.rs`, `src/router/event_loop.rs`, and `src/conn.rs` before protocol-loop edits.
- Run `cargo check`, then `cargo fmt --all -- --check`, then clippy/tests.
- Prioritize correctness and robustness in server-loop behavior over adding broad new features.
