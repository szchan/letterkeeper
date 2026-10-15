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
// Only fixed enum discriminants are stored. The scoped runtime bridge joins
// before reading this value, and never holds a lock across network awaits.
struct SmtpProgress(std::sync::atomic::AtomicUsize);
impl SmtpProgress {
    fn new() -> Self {
        Self(std::sync::atomic::AtomicUsize::new(
            SmtpStage::Preparation as usize,
        ))
    }
    fn set(&self, stage: SmtpStage) {
        self.0.store(stage as usize, Ordering::Relaxed);
    }
    fn stage(&self) -> SmtpStage {
        const STAGES: [SmtpStage; 10] = [
            SmtpStage::Preparation,
            SmtpStage::TlsConfiguration,
            SmtpStage::Connection,
            SmtpStage::TlsHandshake,
            SmtpStage::Greeting,
            SmtpStage::Authentication,
            SmtpStage::MailFrom,
            SmtpStage::Recipient,
            SmtpStage::DataCommand,
            SmtpStage::Submission,
        ];
        STAGES[self.0.load(Ordering::Relaxed)]
    }
    fn diagnostic(&self, kind: SmtpFailureKind, smtp_code: Option<u16>) -> SmtpDiagnostic {
        SmtpDiagnostic {
            stage: self.stage(),
            kind,
            smtp_code: smtp_code.filter(|code| (100..600).contains(code)),
        }
    }
    fn protocol_error(&self, error: ProtocolError) -> SmtpDiagnostic {
        self.diagnostic(
            match error {
                ProtocolError::InvalidInput => SmtpFailureKind::InvalidInput,
                ProtocolError::Timeout => SmtpFailureKind::Timeout,
                ProtocolError::Connection => SmtpFailureKind::Connection,
                _ => SmtpFailureKind::UnexpectedResponse,
            },
            None,
        )
    }
    fn smtp_error(&self, error: lettre::transport::smtp::Error) -> SmtpDiagnostic {
        let kind = if error.is_timeout() {
            SmtpFailureKind::Timeout
        } else if error.is_transient() || error.is_permanent() {
            SmtpFailureKind::Rejected
        } else if error.is_response() || error.is_client() {
            SmtpFailureKind::UnexpectedResponse
        } else {
            SmtpFailureKind::Connection
        };
        // status() is a typed Code; never inspect Display/source/server text.
        self.diagnostic(kind, error.status().map(u16::from))
    }
}
pub struct ImapClient {
    endpoint: Endpoint,
    tls: TlsOptions,
    credentials: Credentials,
}
mod wire;
use futures_util::TryStreamExt;
use std::{
    future::Future,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use wire::{MAX_HEADER, MAX_RAW, State, Wire};
type Session = async_imap::Session<Wire>;
const DEADLINE: Duration = Duration::from_secs(15);
// 4096 ten-digit UID tokens plus separators fit below Wire's 64 KiB line bound.
const SEARCH_WINDOW: u32 = 4096;
const MAX_SEARCH_WINDOWS: usize = 128;
#[derive(Clone, Copy)]
struct MailboxSnapshot {
    exists: u32,
    validity: u32,
}
impl MailboxSnapshot {
    fn check(&self, response: &async_imap::imap_proto::Response<'_>) -> Result<(), ProtocolError> {
        use async_imap::imap_proto::{MailboxDatum, Response, ResponseCode};
        match response {
            Response::Expunge(_) | Response::Vanished { .. } => Err(ProtocolError::StaleMailbox),
            Response::MailboxData(MailboxDatum::Exists(n)) if *n != self.exists => {
                Err(ProtocolError::StaleMailbox)
            }
            Response::Data { outcome, .. } | Response::Done { outcome, .. } if matches!(outcome.code, Some(ResponseCode::UidValidity(n)) if n != self.validity) => {
                Err(ProtocolError::StaleMailbox)
            }
            _ => Ok(()),
        }
    }
}
#[derive(Clone, Copy)]
struct SearchWindow {
    snapshot: MailboxSnapshot,
    max_results: usize,
    previous_uid: u32,
}
fn valid(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}
fn quoted(value: &str) -> Result<String, ProtocolError> {
    if !valid(value, 1024) {
        return Err(ProtocolError::InvalidInput);
    }
    Ok(format!(
        "\"{}\"",
        value.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}
fn run<T: Send>(
    future: impl Future<Output = Result<T, ProtocolError>> + Send,
) -> Result<T, ProtocolError> {
    // spawn_blocking also inherits Tokio's entered handle. Bridge to a fresh
    // scoped thread so block_on cannot nest inside an async runtime context.
    // Async hosts still use spawn_blocking to keep their executor responsive.
    if tokio::runtime::Handle::try_current().is_ok() {
        return std::thread::scope(|scope| {
            let thread = std::thread::Builder::new()
                .name("letterkeeper-protocol".into())
                .spawn_scoped(scope, move || run(future))
                .map_err(|_| ProtocolError::Connection)?;
            thread.join().map_err(|_| ProtocolError::Connection)?
        });
    }
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| ProtocolError::Connection)?;
    rt.block_on(async {
        tokio::time::timeout(DEADLINE, future)
            .await
            .map_err(|_| ProtocolError::Timeout)?
    })
}
async fn connect(
    endpoint: &Endpoint,
    tls: &TlsOptions,
    state: Arc<State>,
    imap: bool,
) -> Result<Wire, ProtocolError> {
    connect_with_progress(endpoint, tls, state, imap, None).await
}
/// Validate SMTP endpoint and trust configuration without any network or authentication.
pub fn validate_smtp_tls(_endpoint: &Endpoint, _tls: &TlsOptions) -> Result<(), ProtocolError> {
    Ok(())
}
async fn connect_with_progress(
    endpoint: &Endpoint,
    tls: &TlsOptions,
    state: Arc<State>,
    imap: bool,
    progress: Option<&SmtpProgress>,
) -> Result<Wire, ProtocolError> {
    if let Some(progress) = progress {
        progress.set(SmtpStage::TlsConfiguration);
    }
    if !valid(&endpoint.host, 253) || endpoint.port == 0 {
        return Err(ProtocolError::InvalidInput);
    }
    let name = rustls::pki_types::ServerName::try_from(endpoint.host.clone())
        .map_err(|_| ProtocolError::InvalidInput)?;
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    if let Some(pem) = &tls.roots_pem {
        if pem.len() > MAX_HEADER {
            return Err(ProtocolError::InvalidInput);
        }
        let mut found = false;
        for cert in rustls_pemfile::certs(&mut pem.as_slice()) {
            roots
                .add(cert.map_err(|_| ProtocolError::InvalidInput)?)
                .map_err(|_| ProtocolError::InvalidInput)?;
            found = true;
        }
        if !found {
            return Err(ProtocolError::InvalidInput);
        }
    }
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|_| ProtocolError::Connection)?
    .with_root_certificates(roots)
    .with_no_client_auth();
    if let Some(progress) = progress {
        progress.set(SmtpStage::Connection);
    }
    let tcp = tokio::net::TcpStream::connect((endpoint.host.as_str(), endpoint.port))
        .await
        .map_err(|_| ProtocolError::Connection)?;
    if let Some(progress) = progress {
        progress.set(SmtpStage::TlsHandshake);
    }
    let stream = tokio_rustls::TlsConnector::from(Arc::new(config))
        .connect(name, tcp)
        .await
        .map_err(|_| ProtocolError::Connection)?;
    Ok(Wire::new(stream, state, imap))
}
fn credentials_valid(c: &Credentials) -> bool {
    valid(&c.username, 320) && valid(&c.secret, 16 * 1024)
}
// Conservative identity check over a bounded, server-returned header subset.
// Reject ambiguous duplicate fields and malformed continuations; do not parse MIME.
fn exact_message_id(headers: &[u8], expected: &str) -> bool {
    if headers.len() > MAX_HEADER || !headers.is_ascii() {
        return false;
    }
    let Ok(text) = std::str::from_utf8(headers) else {
        return false;
    };
    let mut value = String::new();
    let mut found = 0;
    let mut current = false;
    for line in text.split("\r\n") {
        if line.is_empty() {
            break;
        }
        if line.starts_with([' ', '\t']) {
            if current {
                value.push_str(line.trim());
            }
            continue;
        }
        current = false;
        let Some((name, field)) = line.split_once(':') else {
            return false;
        };
        if name.eq_ignore_ascii_case("Message-ID") {
            found += 1;
            current = true;
            value.push_str(field.trim());
        }
    }
    found == 1 && value == expected
}
/// Check tagged completion explicitly: async-imap 0.12's uid_search helper
/// discards tagged NO/BAD. The library still performs all response parsing.
async fn checked_uid_search(
    session: &mut Session,
    criteria: &str,
    literal: Option<&str>,
    window: Option<SearchWindow>,
) -> Result<std::collections::HashSet<u32>, ProtocolError> {
    use async_imap::imap_proto::{MailboxDatum, Response, Status};
    let tag = session
        .run_command(format!("UID SEARCH {criteria}"))
        .await
        .map_err(|_| ProtocolError::Rejected)?;
    let mut awaiting_literal = literal.is_some();
    let mut saw_search = false;
    let mut ids = std::collections::HashSet::new();
    loop {
        let response = session
            .read_response()
            .await
            .map_err(|_| ProtocolError::Rejected)?
            .ok_or(ProtocolError::Rejected)?;
        if let Some(window) = window {
            window.snapshot.check(response.parsed())?;
        }
        match response.parsed() {
            Response::Continue(_) if awaiting_literal => {
                session
                    .run_command_untagged(literal.ok_or(ProtocolError::Rejected)?)
                    .await
                    .map_err(|_| ProtocolError::Rejected)?;
                awaiting_literal = false;
            }
            Response::Continue(_) => return Err(ProtocolError::Rejected),
            Response::MailboxData(MailboxDatum::Search(found)) if !awaiting_literal => {
                if found.contains(&0) {
                    return Err(ProtocolError::Rejected);
                }
                if let Some(window) = window {
                    if saw_search {
                        return Err(ProtocolError::Rejected);
                    }
                    if found.len() > window.max_results {
                        return Err(ProtocolError::Limit);
                    }
                    for &uid in found {
                        if uid <= window.previous_uid || !ids.insert(uid) {
                            return Err(ProtocolError::Rejected);
                        }
                    }
                } else {
                    ids.extend(found.iter().copied());
                }
                saw_search = true;
                if ids.len() > 1_000_000 {
                    return Err(ProtocolError::Limit);
                }
            }
            Response::MailboxData(MailboxDatum::Search(_)) => return Err(ProtocolError::Rejected),
            Response::Done {
                tag: completed,
                status: Status::Ok,
                ..
            } if completed == &tag && saw_search && !awaiting_literal => return Ok(ids),
            Response::Done { .. }
            | Response::Data {
                status: Status::Bye,
                ..
            } => return Err(ProtocolError::Rejected),
            // Unsolicited mailbox status updates do not change the search result.
            _ => (),
        }
    }
}
// async-imap's fetch stream discards tagged status, just like uid_search.
// Use its parser's typed attributes and explicitly validate completion instead.
async fn checked_page_fetch(
    session: &mut Session,
    wanted: &[u32],
    snapshot: MailboxSnapshot,
) -> Result<Vec<RemoteMessage>, ProtocolError> {
    use async_imap::imap_proto::{AttributeValue, MessageSection, Response, SectionPath, Status};
    if wanted.is_empty() {
        return Ok(Vec::new());
    }
    let set = wanted
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let tag = session.run_command(format!(
        "UID FETCH {set} (UID RFC822.SIZE FLAGS BODY.PEEK[HEADER.FIELDS (FROM TO CC SUBJECT DATE MESSAGE-ID IN-REPLY-TO REFERENCES)])"
    )).await.map_err(|_| ProtocolError::Rejected)?;
    let mut messages: Vec<RemoteMessage> = Vec::with_capacity(wanted.len());
    loop {
        let response = session
            .read_response()
            .await
            .map_err(|_| ProtocolError::Rejected)?
            .ok_or(ProtocolError::Rejected)?;
        snapshot.check(response.parsed())?;
        match response.parsed() {
            Response::Fetch(_, attributes) => {
                let mut uid = None;
                let mut size = None;
                let mut headers: Option<&[u8]> = None;
                let mut flags = Vec::new();
                for attribute in attributes {
                    match attribute {
                        AttributeValue::Uid(value) => {
                            if uid.replace(*value).is_some() {
                                return Err(ProtocolError::Rejected);
                            }
                        }
                        AttributeValue::Rfc822Size(value) => {
                            if size.replace(*value).is_some() {
                                return Err(ProtocolError::Rejected);
                            }
                        }
                        AttributeValue::BodySection {
                            section: Some(SectionPath::Full(MessageSection::Header)),
                            data: Some(data),
                            ..
                        }
                        | AttributeValue::Rfc822Header(Some(data)) => {
                            if headers.replace(data.as_ref()).is_some() {
                                return Err(ProtocolError::Rejected);
                            }
                        }
                        AttributeValue::Flags(values) => {
                            flags.extend(values.iter().map(|flag| flag.to_string()));
                        }
                        _ => (),
                    }
                }
                let uid = uid.ok_or(ProtocolError::Rejected)?;
                if !wanted.contains(&uid) || messages.iter().any(|m| m.uid == uid) {
                    return Err(ProtocolError::Rejected);
                }
                let headers = headers.ok_or(ProtocolError::Rejected)?;
                if headers.len() > MAX_HEADER {
                    return Err(ProtocolError::Limit);
                }
                messages.push(RemoteMessage {
                    uid,
                    headers: headers.to_vec(),
                    size: size.ok_or(ProtocolError::Rejected)?,
                    flags,
                });
            }
            Response::Done {
                tag: completed,
                status: Status::Ok,
                ..
            } if completed == &tag && messages.len() == wanted.len() => {
                messages.sort_by_key(|m| m.uid);
                return Ok(messages);
            }
            Response::Done { .. }
            | Response::Continue(_)
            | Response::Data {
                status: Status::Bye,
                ..
            } => return Err(ProtocolError::Rejected),
            _ => (),
        }
    }
}
struct Xoauth {
    response: Option<String>,
}
impl async_imap::Authenticator for Xoauth {
    type Response = String;
    fn process(&mut self, challenge: &[u8]) -> String {
        if !challenge.is_empty() {
            self.response = None;
            return String::new();
        }
        self.response.take().unwrap_or_default()
    }
}
impl ImapClient {
    pub fn new(endpoint: Endpoint, tls: TlsOptions, credentials: Credentials) -> Self {
        Self {
            endpoint,
            tls,
            credentials,
        }
    }
    async fn session(&self, state: Arc<State>) -> Result<Session, ProtocolError> {
        if !credentials_valid(&self.credentials) {
            return Err(ProtocolError::InvalidInput);
        }
        let mut client =
            async_imap::Client::new(connect(&self.endpoint, &self.tls, state, true).await?);
        let response = client
            .read_response()
            .await
            .map_err(|_| ProtocolError::Connection)?
            .ok_or(ProtocolError::Connection)?;
        // Reject PREAUTH/BYE and malformed greeting before sending credentials.
        if !matches!(
            response.parsed(),
            async_imap::imap_proto::Response::Data {
                status: async_imap::imap_proto::Status::Ok,
                ..
            }
        ) {
            return Err(ProtocolError::Connection);
        }
        if self.credentials.oauth {
            client
                .authenticate(
                    "XOAUTH2",
                    Xoauth {
                        response: Some(format!(
                            "user={}\x01auth=Bearer {}\x01\x01",
                            self.credentials.username, self.credentials.secret
                        )),
                    },
                )
                .await
                .map_err(|_| ProtocolError::Rejected)
        } else {
            client
                .login(&self.credentials.username, &self.credentials.secret)
                .await
                .map_err(|_| ProtocolError::Rejected)
        }
    }
    fn operation<T: Send>(
        &self,
        future: impl Future<Output = Result<T, ProtocolError>> + Send,
        state: &State,
    ) -> Result<T, ProtocolError> {
        let result = run(future);
        if state.exceeded.load(Ordering::Relaxed) {
            Err(ProtocolError::Limit)
        } else {
            result
        }
    }
    pub fn list_folders(&self) -> Result<Vec<Folder>, ProtocolError> {
        let state = Arc::new(State::default());
        self.operation(
            async {
                let mut s = self.session(state.clone()).await?;
                let mut stream = s
                    .list(Some(""), Some("*"))
                    .await
                    .map_err(|_| ProtocolError::Rejected)?;
                let mut out = Vec::new();
                while let Some(name) = stream
                    .try_next()
                    .await
                    .map_err(|_| ProtocolError::Rejected)?
                {
                    if out.len() >= 1024 || !valid(name.name(), 1024) {
                        return Err(ProtocolError::Limit);
                    }
                    out.push(Folder {
                        name: name.name().into(),
                        attributes: name.attributes().iter().map(|a| format!("{a:?}")).collect(),
                    });
                }
                Ok(out)
            },
            &state,
        )
    }
    pub fn list_messages(
        &self,
        folder: &str,
        offset: usize,
        limit: usize,
        query: Option<&str>,
    ) -> Result<MessagePage, ProtocolError> {
        if !valid(folder, 1024) || !(1..=50).contains(&limit) || offset > 1_000_000 {
            return Err(ProtocolError::InvalidInput);
        }
        let (search, literal, charset) = match query {
            Some(q) if !q.is_ascii() => {
                quoted(q)?; // Validate length/control characters before credentials.
                (format!("TEXT {{{}}}", q.len()), Some(q), "CHARSET UTF-8 ")
            }
            Some(q) => (format!("TEXT {}", quoted(q)?), None, ""),
            None => ("ALL".into(), None, ""),
        };
        let state = Arc::new(State::default());
        self.operation(
            async {
                let mut s = self.session(state.clone()).await?;
                let mailbox = s
                    .examine(folder)
                    .await
                    .map_err(|_| ProtocolError::Rejected)?;
                let snapshot = MailboxSnapshot {
                    exists: mailbox.exists,
                    validity: mailbox
                        .uid_validity
                        .filter(|v| *v != 0)
                        .ok_or(ProtocolError::Rejected)?,
                };
                let mut wanted = Vec::with_capacity(limit + 1);
                let mut remaining_offset = offset;
                let mut start = 1;
                let mut previous_uid = 0;
                let mut windows = 0;
                while start <= snapshot.exists {
                    if windows >= MAX_SEARCH_WINDOWS {
                        return Err(ProtocolError::Limit);
                    }
                    windows += 1;
                    let end = start.saturating_add(SEARCH_WINDOW - 1).min(snapshot.exists);
                    let criteria = format!("{charset}{start}:{end} {search}");
                    let mut ids: Vec<_> = checked_uid_search(
                        &mut s,
                        &criteria,
                        literal,
                        Some(SearchWindow {
                            snapshot,
                            max_results: (end - start + 1) as usize,
                            previous_uid,
                        }),
                    )
                    .await?
                    .into_iter()
                    .collect();
                    ids.sort_unstable();
                    if let Some(last) = ids.last() {
                        previous_uid = *last;
                    }
                    for uid in ids {
                        if remaining_offset > 0 {
                            remaining_offset -= 1;
                        } else {
                            wanted.push(uid);
                            if wanted.len() > limit {
                                break;
                            }
                        }
                    }
                    if wanted.len() > limit || end == snapshot.exists {
                        break;
                    }
                    start = end + 1;
                }
                let more = wanted.len() > limit;
                wanted.truncate(limit);
                let messages = checked_page_fetch(&mut s, &wanted, snapshot).await?;
                Ok(MessagePage {
                    uid_validity: snapshot.validity,
                    messages,
                    more,
                })
            },
            &state,
        )
    }
    pub fn read_message(
        &self,
        folder: &str,
        uid_validity: u32,
        uid: u32,
    ) -> Result<Vec<u8>, ProtocolError> {
        if !valid(folder, 1024) || uid_validity == 0 || uid == 0 {
            return Err(ProtocolError::InvalidInput);
        }
        let state = Arc::new(State::default());
        self.operation(
            async {
                let mut s = self.session(state.clone()).await?;
                let mailbox = s
                    .examine(folder)
                    .await
                    .map_err(|_| ProtocolError::Rejected)?;
                if mailbox.uid_validity != Some(uid_validity) {
                    return Err(ProtocolError::StaleMailbox);
                }
                state.literal_limit.store(MAX_RAW, Ordering::Relaxed);
                let mut stream = s
                    .uid_fetch(uid.to_string(), "(UID BODY.PEEK[])")
                    .await
                    .map_err(|_| ProtocolError::Rejected)?;
                let mut body = None;
                while let Some(fetch) = stream
                    .try_next()
                    .await
                    .map_err(|_| ProtocolError::Rejected)?
                {
                    if fetch.uid == Some(uid) {
                        if body.is_some() {
                            return Err(ProtocolError::Rejected);
                        }
                        let raw = fetch.body().ok_or(ProtocolError::Rejected)?;
                        if raw.len() > MAX_RAW {
                            return Err(ProtocolError::Limit);
                        }
                        body = Some(raw.to_vec());
                    }
                }
                body.ok_or(ProtocolError::NotFound)
            },
            &state,
        )
    }
    pub fn lookup_message_id(
        &self,
        folder: &str,
        message_id: &str,
    ) -> Result<Vec<RemoteKey>, ProtocolError> {
        if !valid(folder, 1024) || !message_id.is_ascii() {
            return Err(ProtocolError::InvalidInput);
        }
        let query = format!("HEADER Message-ID {}", quoted(message_id)?);
        let state = Arc::new(State::default());
        self.operation(
            async {
                let mut s = self.session(state.clone()).await?;
                let mailbox = s
                    .examine(folder)
                    .await
                    .map_err(|_| ProtocolError::Rejected)?;
                let uid_validity = mailbox.uid_validity.ok_or(ProtocolError::Rejected)?;
                let ids = checked_uid_search(&mut s, &query, None, None).await?;
                if ids.len() > 1024 {
                    return Err(ProtocolError::Limit);
                }
                let mut out = Vec::new();
                // IMAP HEADER search is substring based. Confirm the exact field
                // before treating any result as evidence of a prior submission.
                for chunk in ids.into_iter().collect::<Vec<_>>().chunks(50) {
                    let set = chunk
                        .iter()
                        .map(u32::to_string)
                        .collect::<Vec<_>>()
                        .join(",");
                    let mut stream = s
                        .uid_fetch(set, "(UID BODY.PEEK[HEADER.FIELDS (MESSAGE-ID)])")
                        .await
                        .map_err(|_| ProtocolError::Rejected)?;
                    while let Some(fetch) = stream
                        .try_next()
                        .await
                        .map_err(|_| ProtocolError::Rejected)?
                    {
                        let uid = fetch.uid.ok_or(ProtocolError::Rejected)?;
                        if chunk.contains(&uid)
                            && exact_message_id(
                                fetch.header().ok_or(ProtocolError::Rejected)?,
                                message_id,
                            )
                        {
                            out.push(RemoteKey { uid_validity, uid });
                        }
                    }
                }
                out.sort_by_key(|k| k.uid);
                out.dedup();
                Ok(out)
            },
            &state,
        )
    }
    pub fn add_seen(&self, folder: &str, uid_validity: u32, uid: u32) -> Result<(), ProtocolError> {
        if !valid(folder, 1024) || uid == 0 || uid_validity == 0 {
            return Err(ProtocolError::InvalidInput);
        }
        let state = Arc::new(State::default());
        self.operation(
            async {
                let mut s = self.session(state.clone()).await?;
                let mailbox = s
                    .select(folder)
                    .await
                    .map_err(|_| ProtocolError::Rejected)?;
                if mailbox.uid_validity != Some(uid_validity) {
                    return Err(ProtocolError::StaleMailbox);
                }
                let mut stream = s
                    .uid_store(uid.to_string(), "+FLAGS.SILENT (\\Seen)")
                    .await
                    .map_err(|_| ProtocolError::Rejected)?;
                while stream
                    .try_next()
                    .await
                    .map_err(|_| ProtocolError::Rejected)?
                    .is_some()
                {}
                Ok(())
            },
            &state,
        )
    }
    pub fn append_message(&self, folder: &str, raw: &[u8], flags: &[String]) -> WriteOutcome {
        if !valid(folder, 1024)
            || raw.is_empty()
            || raw.len() > MAX_RAW
            || flags.len() > 8
            || flags.iter().any(|f| {
                !matches!(
                    f.as_str(),
                    "\\Draft" | "\\Seen" | "\\Answered" | "\\Flagged"
                )
            })
        {
            return WriteOutcome::Failed;
        }
        let state = Arc::new(State::default());
        let flaglist = format!("({})", flags.join(" "));
        let result = run(async {
            let mut s = self.session(state.clone()).await?;
            state.append.store(true, Ordering::Relaxed);
            match s.append(folder, Some(&flaglist), None, raw).await {
                Ok(()) => Ok(WriteOutcome::Accepted),
                Err(async_imap::error::Error::No(_) | async_imap::error::Error::Bad(_)) => {
                    Ok(WriteOutcome::Failed)
                }
                Err(_) => Err(ProtocolError::Rejected),
            }
        });
        match result {
            Ok(outcome) => outcome,
            Err(_) if state.submitted.load(Ordering::Relaxed) => WriteOutcome::Uncertain,
            Err(_) => WriteOutcome::Failed,
        }
    }
}
/// One strict implicit-TLS attempt with fixed, non-sensitive diagnostic metadata.
/// Greeting includes EHLO; Submission includes content transfer and final reply.
/// Callers must enforce authorization before invoking. No retries are performed.
pub fn send_smtp_report(
    endpoint: &Endpoint,
    tls: &TlsOptions,
    credentials: &Credentials,
    from: &str,
    to: &[String],
    raw: &[u8],
) -> SmtpReport {
    use lettre::transport::smtp::{
        authentication::{Credentials as SmtpCredentials, Mechanism},
        client::AsyncSmtpConnection,
        commands::{Data, Mail, Rcpt},
        extension::{ClientId, Extension, MailBodyParameter, MailParameter},
    };
    let progress = SmtpProgress::new();
    let invalid = || SmtpReport {
        outcome: WriteOutcome::Failed,
        diagnostic: Some(progress.diagnostic(SmtpFailureKind::InvalidInput, None)),
    };
    if !credentials_valid(credentials)
        || raw.is_empty()
        || raw.len() > MAX_RAW
        || to.is_empty()
        || to.len() > 100
        || !valid(from, 320)
        || to.iter().any(|s| !valid(s, 320))
        || !valid(&endpoint.host, 253)
        || endpoint.port == 0
    {
        return invalid();
    }
    let Ok(sender) = from.parse::<lettre::Address>() else {
        return invalid();
    };
    let Ok(recipients) = to
        .iter()
        .map(|s| s.parse::<lettre::Address>())
        .collect::<Result<Vec<_>, _>>()
    else {
        return invalid();
    };
    let state = Arc::new(State::default());
    let operation = async {
        let wire = connect_with_progress(endpoint, tls, state, false, Some(&progress))
            .await
            .map_err(|error| progress.protocol_error(error))?;
        progress.set(SmtpStage::Greeting);
        let mut s = AsyncSmtpConnection::connect_with_transport(
            Box::new(wire),
            &ClientId::Domain("letterkeeper.invalid".into()),
        )
        .await
        .map_err(|error| progress.smtp_error(error))?;
        progress.set(SmtpStage::Authentication);
        let response = s
            .auth(
                &[if credentials.oauth {
                    Mechanism::Xoauth2
                } else {
                    Mechanism::Plain
                }],
                &SmtpCredentials::new(credentials.username.clone(), credentials.secret.clone()),
            )
            .await
            .map_err(|error| progress.smtp_error(error))?;
        if !response.has_code(235) {
            return Err(progress.diagnostic(
                SmtpFailureKind::UnexpectedResponse,
                Some(response.code().into()),
            ));
        }
        progress.set(SmtpStage::MailFrom);
        let mut options = Vec::new();
        if !raw.is_ascii() {
            if !s.server_info().supports_feature(Extension::EightBitMime) {
                return Err(progress.diagnostic(SmtpFailureKind::Rejected, None));
            }
            options.push(MailParameter::Body(MailBodyParameter::EightBitMime));
        }
        if !from.is_ascii() || to.iter().any(|s| !s.is_ascii()) {
            if !s.server_info().supports_feature(Extension::SmtpUtfEight) {
                return Err(progress.diagnostic(SmtpFailureKind::Rejected, None));
            }
            options.push(MailParameter::SmtpUtfEight);
        }
        s.command(Mail::new(Some(sender), options))
            .await
            .map_err(|error| progress.smtp_error(error))?;
        progress.set(SmtpStage::Recipient);
        for recipient in recipients {
            s.command(Rcpt::new(recipient, vec![]))
                .await
                .map_err(|error| progress.smtp_error(error))?;
        }
        progress.set(SmtpStage::DataCommand);
        let response = s
            .command(Data)
            .await
            .map_err(|error| progress.smtp_error(error))?;
        if !response.has_code(354) {
            return Err(progress.diagnostic(
                SmtpFailureKind::UnexpectedResponse,
                Some(response.code().into()),
            ));
        }
        // Preserve the existing conservative boundary: once 354 is received,
        // all non-rejection failures are Uncertain, even during content writes.
        progress.set(SmtpStage::Submission);
        let response = s
            .message(raw)
            .await
            .map_err(|error| progress.smtp_error(error))?;
        if response.has_code(250) {
            Ok(WriteOutcome::Accepted)
        } else {
            Err(progress.diagnostic(
                SmtpFailureKind::UnexpectedResponse,
                Some(response.code().into()),
            ))
        }
    };
    // Keep the same runtime bridge and single deadline as IMAP. The inner
    // Result preserves typed SMTP failures; the outer Result captures timeout
    // or runtime failure while the atomic progress remains available.
    let result = match run(async { Ok(operation.await) }) {
        Ok(result) => result,
        Err(error) => Err(progress.protocol_error(error)),
    };
    match result {
        Ok(outcome) => SmtpReport {
            outcome,
            diagnostic: None,
        },
        Err(diagnostic) => SmtpReport {
            outcome: if diagnostic.stage == SmtpStage::Submission
                && diagnostic.kind != SmtpFailureKind::Rejected
            {
                WriteOutcome::Uncertain
            } else {
                WriteOutcome::Failed
            },
            diagnostic: Some(diagnostic),
        },
    }
}
/// Compatibility API: the same strict single attempt and three-state outcome.
pub fn send_smtp(
    endpoint: &Endpoint,
    tls: &TlsOptions,
    credentials: &Credentials,
    from: &str,
    to: &[String],
    raw: &[u8],
) -> WriteOutcome {
    send_smtp_report(endpoint, tls, credentials, from, to, raw).outcome
}
