Based on the current code in `src/main.rs`, `src/router.rs`, `src/https.rs`, and `src/helpers.rs`, here’s what is still remaining from `README.md` + `AUDIT.md`, ordered by priority.

**P0 (must do first, project-blocking)**

- NASER:
  - [ ] Implement real request-body handling (currently only headers are parsed; `body` is always empty), including `Content-Length` and full read state machine in `src/router.rs:174` and `src/router.rs:368`.
    - [ ] Implement file uploads and retrieval validation (upload then GET back uncorrupted).
  - [ ] Add chunked request support (`Transfer-Encoding: chunked`) and ensure CGI works with both chunked/unchunked bodies (explicit audit point).
  - [ ] Remove crash paths: `unwrap()` in server loop (`src/main.rs:31`) and listener setup unwraps (`src/router.rs:51`, `src/router.rs:55`) violate “server never crashes”.

- EB:
  - [x] Implement `POST` and `DELETE` behavior end-to-end (routing, filesystem effects where needed, correct status codes), not just enum parsing in `src/https.rs:4`.
  - [x] Add per-connection/request timeout handling (currently `epoll_wait` is blocking with `-1` in `src/router.rs:240` and no client timeout policy).
  - [ ] Implement cookies + session system (explicit mandatory requirement and audit checkpoint).

**P1 (core compliance)**

- [ ] Build full configuration-file parser and runtime model (host, ports, server_name/default server, routes, methods, redirects, root, index/default file, autoindex, CGI mapping, body size limit, custom error page paths) per README config section.
- [ ] Support virtual hosts by `Host` header and default-server fallback on same `host:port`.
- [ ] Enforce client max body size and return `413`.
- [ ] Implement custom/default error pages for at least `400, 403, 404, 405, 413, 500` (you have generic HTML error responses, but not config-driven custom pages).
- [ ] Add directory handling: default index file, optional directory listing on/off, forbidden cases (`403`) where applicable.
- [ ] Add redirection route support (`3xx` + `Location`).
- [ ] Add configuration conflict handling from audit:
  - [ ] same port configured multiple times should be detected/handled clearly,
  - [ ] invalid one server config should not bring down all valid servers.
- [ ] Ensure request/response header correctness for real browser static-site serving (currently very minimal).

**P2 (audit readiness / robustness)**

- [ ] Add exhaustive test suite (methods, bad requests, config errors, redirects, directory behavior, error pages, CGI, chunked/unchunked).
- [ ] Add stress testing workflow (`siege`) and prove availability >= 99.5%.
- [ ] Add memory leak and hanging-connection checks with reproducible scripts/commands.
- [ ] Prepare audit explanations for epoll flow and “one multiplexing loop” guarantees with code walk-through.

**P3 (bonus)**

- [ ] Implement at least one additional CGI beyond the first.
- [ ] Optional second implementation in another language.

If you want, I can turn this into a concrete execution plan (e.g., 2-week milestone checklist with file-level tasks and test cases).
