// SPDX-License-Identifier: GPL-3.0-only
//! Bounded IMAP and SMTP operations over strict TLS.
mod imap;
mod runtime;
mod smtp;
mod tls;
mod types;
mod validation;
mod wire;

pub use imap::ImapClient;
pub use smtp::{send_smtp, send_smtp_report};
pub use tls::validate_smtp_tls;
pub use types::{
    Credentials, Endpoint, Folder, MessagePage, ProtocolError, RemoteKey, RemoteMessage,
    SmtpDiagnostic, SmtpFailureKind, SmtpReport, SmtpStage, TlsOptions, WriteOutcome,
};
