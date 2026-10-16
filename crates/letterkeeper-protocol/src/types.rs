// SPDX-License-Identifier: GPL-3.0-only
use std::fmt;
use zeroize::Zeroize;
#[derive(Clone, Debug)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
}
#[derive(Clone, Debug, Default)]
pub struct TlsOptions {
    pub roots_pem: Option<Vec<u8>>,
}
#[derive(Clone)]
pub struct Credentials {
    pub username: String,
    pub secret: String,
    pub oauth: bool,
}
impl Drop for Credentials {
    fn drop(&mut self) {
        self.secret.zeroize();
        self.username.zeroize();
    }
}
impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Credentials([redacted])")
    }
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("invalid protocol input")]
    InvalidInput,
    #[error("protocol connection failed")]
    Connection,
    #[error("protocol operation failed")]
    Rejected,
    #[error("protocol response exceeds limit")]
    Limit,
    #[error("protocol operation timed out")]
    Timeout,
    #[error("mailbox identity changed")]
    StaleMailbox,
    #[error("message not found")]
    NotFound,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub name: String,
    pub attributes: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteMessage {
    pub uid: u32,
    pub headers: Vec<u8>,
    pub size: u32,
    pub flags: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessagePage {
    pub uid_validity: u32,
    pub messages: Vec<RemoteMessage>,
    pub more: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteKey {
    pub uid_validity: u32,
    pub uid: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOutcome {
    Accepted,
    Failed,
    Uncertain,
}
/// Bounded diagnostic metadata. Never contains server text or mail identities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmtpStage {
    Preparation,
    TlsConfiguration,
    Connection,
    TlsHandshake,
    Greeting,
    Authentication,
    MailFrom,
    Recipient,
    DataCommand,
    Submission,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmtpFailureKind {
    InvalidInput,
    Connection,
    Timeout,
    Rejected,
    UnexpectedResponse,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SmtpDiagnostic {
    pub stage: SmtpStage,
    pub kind: SmtpFailureKind,
    pub smtp_code: Option<u16>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SmtpReport {
    pub outcome: WriteOutcome,
    pub diagnostic: Option<SmtpDiagnostic>,
}
