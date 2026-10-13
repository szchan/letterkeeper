# ADR 0002 fragment: bounded TLS protocol transport

2026-10-13. This fragment covers only protocol library choices; integration and account decisions belong to the root implementation.

Use async-imap 0.12 (MIT/Apache-2.0, actively used by Delta Chat) with its runtime-tokio feature, Tokio 1 (MIT), rustls 0.23/tokio-rustls 0.26 (MIT/Apache-2.0) with ring, and webpki-roots (MPL-2.0). TLS validates both trust chain and endpoint name before sending credentials. An optional PEM root is an additional trust anchor, never a certificate-verification bypass. Implicit TLS only in this increment; STARTTLS deferred rather than silently falling back.

Reuse lettre 0.11 (MIT) for SMTP grammar, response parsing and authentication, keeping message construction separate from immutable-byte submission. No connection pool/retry. A bounded TLS stream must precede library parsing, enforce literal lengths before buffering, and cap each session. Operation deadlines cover DNS/TCP/TLS/authentication and commands. Fixed diagnostic enums discard upstream strings which may contain credentials or mail data.

Tests use rcgen (MIT/Apache-2.0) ephemeral TLS keys, synthetic loopback services only. No certificate private keys are checked into the repository.

Upstream reviewed: https://docs.rs/async-imap/latest/async_imap/ (0.12.0), https://docs.rs/async-imap/latest/async_imap/struct.Session.html, https://docs.rs/lettre/latest/lettre/transport/smtp/struct.AsyncSmtpTransport.html (0.11.23). Dependency source inspection supplements these docs for buffer and retry behavior.
