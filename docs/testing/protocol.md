# Protocol verification record

2026-10-13. Synthetic loopback TLS only; no real accounts or external mail traffic.

## TDD RED

`cargo test -p letterkeeper-protocol --offline` initially could not bind loopback under the execution sandbox (EPERM). Re-run with local-socket permission using the compilable stub: five behavioral assertions fail (folders/read, pagination, input rejection, APPEND accepted, SMTP accepted). Three negative-only controls pass; they alone do not prove implemented transport behavior.

The local TLS support creates rcgen certificates and keys at runtime. `tests/support.rs` exposes `Server`, `Scenario`, `Server::credentials`, and observed synthetic transcript for integration tests through `#[path]`.

Implementation and GREEN results to follow.
