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

Limitations: implicit TLS only; real providers and STARTTLS unverified. Blocking APIs create a fresh scoped thread when Tokio has an entered handle; asynchronous callers use spawn_blocking to keep the executor responsive. SEARCH scans bounded server response then returns a page (no cache/snapshot across concurrent mailbox changes). Credentials are in-memory and redacted, not a credential store. This crate transports raw bytes; it does not parse/render mail bodies, authorize sends or automatically reconcile uncertainty.

## Recovery/refusal regression RED

Expanded TLS tests revealed two behavioral failures: UID SEARCH HEADER alone returned all three substring candidates rather than the one exact Message-ID; APPEND tagged NO after literal submission was conservatively Uncertain although the server explicitly refused. `cargo test -p letterkeeper-protocol --offline`: 14 passed, 2 failed, including a real 15 s authentication deadline. New controls cover explicit Seen, exact 2 MiB body acceptance, header declaration overflow, SMTP CA/hostname failure and compile-time TRACE suppression even with a capturing Trace logger.

Recovery/refusal GREEN: 16/16 tests passed (16.14 s). Recovery now fetches Message-ID headers with PEEK and confirms a single exact field, rejecting extra text or duplicates. APPEND explicit tagged NO/BAD is Failed; network failure after literal write remains Uncertain. SMTP explicit final refusal is also Failed. A Trace-level capturing logger observed no credential/message text; the static TRACE ceiling was checked.

## Reusable mailbox fixture

`Server::start_shared()` serves sequential independent TLS sessions until `finish()` or drop, with an ephemeral certificate and persistent INBOX/Drafts/Sent synthetic state. APPEND stores exact bytes; SEARCH/FETCH retrieve appended items across sessions. `MESSAGE` includes Reply-To, thread identifiers, a remote HTML image/script and an inert synthetic attachment for application worker/adapter tests. Stop polling is bounded (100 ms accept polling, 3 s idle I/O); no detached fixture survives a failed test.

`cargo test -p letterkeeper-protocol --test protocol shared_mailbox_preserves_append_across_tls_sessions --offline`: passed (1.29 s). Clippy all targets with `-D warnings`: passed. This fixture is deliberately a limited transcript service, not a complete reference server; full Dovecot/provider semantics remain unverified.

Final crate checks after fixture cleanup: `cargo test --offline --locked -p letterkeeper-protocol` passed 17/17 (16.18 s); `cargo fmt --all -- --check` passed; `cargo clippy --offline --locked -p letterkeeper-protocol --all-targets -- -D warnings` passed. The worktree's root workspace member and generated lock change are intentionally left uncommitted for root integration.

## MCP runtime bridge regression RED

Application integration revealed that Tokio `spawn_blocking` threads still have an entered runtime handle. The former `Handle::try_current` rejection prevented legitimate MCP remote calls. A real TLS regression compares ordinary `std::thread` and `tokio::task::spawn_blocking`; the ordinary thread succeeds and spawn_blocking returns InvalidInput. This is an actual behavioral regression, not a test transport failure.

MCP bridge GREEN: scoped fresh-thread execution supports Tokio's entered handle without nested-runtime panic; real TLS IMAP succeeds from both ordinary threads and `spawn_blocking`. `cargo test --offline -p letterkeeper-protocol`: 18/18 passed (16.17 s). Clippy all targets passed. Async hosts still use spawn_blocking because the public API is blocking. `Credentials::drop` now zeroizes username and secret using zeroize 1 (MIT OR Apache-2.0); credentials remain redacted and non-serializable.

Direct calls from an entered Tokio runtime were also verified against the controlled TLS service (rather than an arbitrary loopback port); they succeed through the scoped thread bridge. Such calls still block their caller, so application async paths retain spawn_blocking.
