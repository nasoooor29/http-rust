# Remaining Work (Prioritized)

Based on the current codebase (`src/main.rs`, `src/router.rs`, `src/conn.rs`, `src/https.rs`), the core epoll loop, non-blocking I/O, basic routing, GET/POST/DELETE handlers, simple uploads, and in-memory cookie sessions are in place.

The items below are still needed to satisfy the README/AUDIT requirements, ordered by priority and split so two people can work in parallel.

## P0 - Mandatory for project compliance

1. **Config file system (Owner A)**
   - Parse a server config file instead of hardcoding ports/routes in `main.rs`.
   - Support: host, multiple ports per server, `server_name`, route blocks, body limit, error pages.
   - Add validation with clear startup errors (invalid syntax, invalid route options, duplicate/conflicting listen declarations).

2. **Config-driven virtual hosts and routing (Owner A)**
   - Implement host+port server selection (`Host` header + default server fallback).
   - Keep first server on same host:port as default when `server_name` does not match.
   - Allow multiple servers with shared ports while isolating misconfigured server blocks.

3. **Route features required by audit (Owner A)**
   - Per-route allowed methods with correct `405` behavior.
   - Redirect rules.
   - Route root/file mapping and default index file for directories.
   - Directory listing enable/disable behavior.

4. **Body size limits and strict request handling (Owner B)**
   - Enforce `client_max_body_size` during body read (both content-length and chunked paths).
   - Return `413` when exceeded.
   - Add malformed-request handling cases to ensure bad requests do not break the event loop.

5. **Custom/default error pages (Owner B)**
   - Serve configured custom error pages.
   - Ensure default pages exist for at least: `400`, `403`, `404`, `405`, `413`, `500`.

6. **CGI support (at least one) (Owner B)**
   - Execute CGI by extension using `fork/exec`.
   - Pass body until EOF and required env vars (including `PATH_INFO`).
   - Ensure chunked and unchunked request bodies work for CGI endpoints.

## P1 - Strongly recommended before audit

7. **Browser/static website behavior hardening (Owner A)**
   - Serve full static assets correctly (HTML/CSS/JS/images).
   - Correct handling for wrong URL, redirects, and directory access cases.
   - Verify request/response headers expected by modern browsers.

8. **HTTP correctness and connection behavior (Owner B)**
   - Improve parser edge-case coverage (duplicate headers, invalid framing, unsupported transfer coding, invalid request lines).
   - Re-check status code mapping consistency across all failure paths.
   - Revisit keep-alive behavior vs `Connection: close` policy for browser compatibility.

9. **Tests coverage expansion (Both, split by area)**
   - Unit tests: parser, chunked decoding, routing/method matching, config validation.
   - Integration tests: multi-port/multi-server, file upload/download integrity, error pages, redirects, CGI.
   - Add regression tests for previously fixed bugs.

## P2 - Audit readiness and reliability proof

10. **Stress and stability validation (Owner B)**
    - Run `siege -b [IP]:[PORT]` and document availability >= 99.5%.
    - Check hanging connections under load and timeout cleanup behavior.

11. **Memory/resource checks (Owner B)**
    - Run leak/fd checks (e.g., valgrind or sanitizer + fd monitoring).
    - Confirm no fd leaks across connection churn and CGI execution.

12. **Audit documentation package (Owner A)**
    - Prepare runnable config examples for all required scenarios.
    - Add a short test playbook with curl/browser commands used during audit.
    - Document architecture choices: single epoll model, one read/write per readiness cycle, failure cleanup path.

---

## Suggested Two-Person Split

- **Person A (Config/Serving track):** Items 1, 2, 3, 7, 12
- **Person B (Protocol/CGI/Quality track):** Items 4, 5, 6, 8, 10, 11
- **Shared checkpoints:** Item 9 after P0 merges, then final joint audit dry run.
