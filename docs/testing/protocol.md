# Protocol verification record

2026-10-13. Synthetic loopback TLS only; no real accounts or external mail traffic.

## TDD RED

`cargo test -p letterkeeper-protocol --offline` initially could not bind loopback under the execution sandbox (EPERM). Re-run with local-socket permission using the compilable stub: five behavioral assertions fail (folders/read, pagination, input rejection, APPEND accepted, SMTP accepted). Three negative-only controls pass; they alone do not prove implemented transport behavior.

The local TLS support creates rcgen certificates and keys at runtime. `tests/support.rs` exposes `Server`, `Scenario`, `Server::credentials`, and observed synthetic transcript for integration tests through `#[path]`.

Implementation and GREEN results to follow.

## Initial GREEN

`cargo test -p letterkeeper-protocol --offline`: 8 tests passed (0.48 s), with synthetic loopback permission. `cargo clippy -p letterkeeper-protocol --all-targets --offline -- -D warnings`: passed. `cargo fmt -p letterkeeper-protocol`: applied.

Valid TLS folder/read and metadata/search operations, strict CA/hostname failures before credentials, EXAMINE+PEEK without STORE, UIDVALIDITY rejection, giant declared literal rejection before payload, fixed OAuth error diagnostics, APPEND acceptance/refusal/disconnect and SMTP acceptance/refusal/disconnect/Bcc envelope are verified. Each write uses one connection and one attempt; Accepted means server acceptance, not delivery.

Bounds: 15 s whole operation including connection/TLS/authentication; 8 MiB session response; 2 MiB raw message; 64 KiB header literal/IMAP line; 4 KiB SMTP line; 50 metadata entries/page; 1,024 folders and recovery matches. async-imap emits credential-bearing TRACE logs, so `log` compile-time max_level_debug and release_max_level_debug remove TRACE in every profile. Do not enable lettre tracing or log max_level_trace.

Dependencies: async-imap 0.12.0, tokio-rustls 0.26.6, rustls 0.23.45, lettre 0.11.23, rcgen 0.14.10; licenses and choice rationale in ADR 0002. Maintained upstream APIs and source were reviewed; no vendored protocol or cryptography implementation.

Limitations: implicit TLS only; real providers and STARTTLS unverified. Blocking APIs reject calls from an active Tokio runtime; use spawn_blocking. SEARCH scans bounded server response then returns a page (no cache/snapshot across concurrent mailbox changes). Credentials are in-memory and redacted, not a credential store. This crate transports raw bytes; it does not parse/render mail bodies, authorize sends or automatically reconcile uncertainty.
