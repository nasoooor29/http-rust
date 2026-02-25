# AGENTS.md
Guidance for autonomous coding agents working in this repository.

## Scope
- Applies to the whole repository rooted at this file.
- Project type: Rust binary crate (`http-rust`) using `libc` for low-level networking.
- Runtime model: single-process, single-thread, `epoll`-driven HTTP server.

## Rule Sources and Precedence
- Primary source: this `AGENTS.md`.
- Additional rule files checked:
  - `.cursor/rules/` -> not present
  - `.cursorrules` -> not present
  - `.github/copilot-instructions.md` -> not present
- If any of these files appear later, treat them as extra constraints and follow the strictest applicable rule.

## Repository Layout
- `src/main.rs`: bootstrap, server setup, route registration on `8080` and `9090`.
- `src/router.rs`: epoll event loop, connection lifecycle, request parsing, route dispatch.
- `src/https.rs`: HTTP types (`Request`, `Response`, `StatusCode`, `HttpMethod`, headers).
- `src/helpers.rs`: syscall wrappers (`socket`, `bind`, `listen`, `accept4`, `recv`, `send`, `epoll_ctl`).
- `README.md`: project requirements.
- `AUDIT.md`: audit checklist.
- `REMAINING.md`: known gap list.

## Build, Lint, Run, and Test Commands
Run from repository root.

### Build / Check
- Debug build: `cargo build`
- Release build: `cargo build --release`
- Type-check only: `cargo check`
- Type-check all targets: `cargo check --all-targets`

### Run
- Run server: `cargo run`
- Run optimized server: `cargo run --release`

### Format / Lint
- Check formatting: `cargo fmt --all -- --check`
- Apply formatting: `cargo fmt --all`
- Strict clippy: `cargo clippy --all-targets --all-features -- -D warnings`
- Faster clippy loop: `cargo clippy --all-targets -- -D warnings`

### Tests
- Run all tests: `cargo test`
- Run with output: `cargo test -- --nocapture`
- Run serially (useful for networking tests): `cargo test -- --test-threads=1`

### Running a Single Test (important)
- Unit test by exact name:
  - `cargo test parse_request_rejects_invalid_version -- --exact --nocapture`
- Test(s) by name substring:
  - `cargo test parse_request -- --nocapture`
- One integration target (`tests/http_flow.rs`):
  - `cargo test --test http_flow`
- One integration test by exact name:
  - `cargo test --test http_flow handles_get_health -- --exact --nocapture`
- List tests without running:
  - `cargo test -- --list`

Note: if `tests/` is absent, keep these single-test command patterns as canonical examples.

## Recommended Validation Before Finalizing Changes
- `cargo fmt --all`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test -- --nocapture`
- If networking behavior changed, manually verify with `curl` against both `8080` and `9090`.

## Code Style Guidelines
Follow existing patterns in `src/*.rs` and standard Rust conventions.

### Imports
- Group imports by origin: `std`, external crates, then `crate::...`.
- Keep imports explicit and minimal.
- Prefer one logical import block unless separation improves readability.

### Formatting
- Treat `rustfmt` output as source of truth.
- Prefer multi-line formatting for long function calls and match arms.
- Keep trailing commas in multi-line literals/calls.
- Avoid manual alignment that `rustfmt` will rewrite.

### Types and Data Modeling
- Prefer domain types (`StatusCode`, `HttpMethod`, `Request`, `Response`) over primitives in APIs.
- Use `enum` for protocol states and finite options.
- Prefer named-`struct` fields (`local_port`, `in_buf`, `out_buf`) over tuple structs.
- Borrow in hot paths where practical; allocate only when needed.

### Naming
- `snake_case` for functions, variables, modules.
- `PascalCase` for structs, enums, type aliases.
- Use verb-first names for side effects (`handle_connections`, `drop_conn`, `create_epoll`).
- Use protocol-specific names when helpful (`header_end`, `status`, `local_port`).

### Error Handling
- Use `Result<T, io::Error>` at syscall and I/O boundaries.
- Avoid `unwrap()`/`expect()` in long-running server paths.
- Convert recoverable parsing/routing failures into HTTP error responses.
- Add context to errors/logging (`last_err("...")`, `eprintln!`).
- On per-connection failures, remove fd from epoll and close the socket cleanly.

### Unsafe Code
- Keep `unsafe` blocks small and localized.
- Add concise safety comments when invariants are not obvious.
- Prefer safe wrappers in `src/helpers.rs` so higher layers remain safe.
- Do not expand unsafe usage without a concrete syscall/API need.

### Event Loop and Networking
- Keep all sockets non-blocking.
- Route all reads/writes through epoll readiness handling.
- Keep one event-loop model; do not introduce request-handling threads.
- Correctly handle partial reads/writes via buffering and retry-on-readiness.

### HTTP Behavior
- Preserve current version checks (`HTTP/1.0` and `HTTP/1.1`).
- Normalize header names to lowercase internally.
- Return accurate status codes (`404`, `405`, etc.) for route/method outcomes.
- Keep response headers consistent with body (`Content-Length`, `Content-Type`, `Connection`).

### Route and Handler Design
- Keep handlers transport-agnostic where possible: `fn(&Request) -> Response`.
- Keep socket/epoll concerns in router/helpers, not route handlers.
- Keep route registration explicit in `main.rs` unless config-driven routing is introduced.

### Logging and Testing
- Use `println!` for startup/info and `eprintln!` for errors.
- Include fd/port/context in error logs when available.
- Avoid noisy per-event logging in hot loops.
- Add unit tests close to parser/HTTP logic and integration tests under `tests/` for end-to-end flows.
- Prefer assertions over status line, headers, and body bytes.

## Change Management for Agents
- Make focused, minimal patches.
- Avoid unnecessary dependencies (especially async runtimes/server frameworks).
- Respect project constraints in `README.md` (no tokio/nix-based server implementation).
- If behavior changes, update tests and this file when command/style guidance changes.

## Quick Start for New Agents
- Read `README.md` and `AUDIT.md` first.
- Review `src/router.rs` before changing protocol or connection flow.
- Run `cargo check`, then `cargo fmt --all -- --check`, then clippy/tests.
- Prioritize correctness and robustness over feature breadth in server-loop changes.
