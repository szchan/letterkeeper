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
    let tcp = tokio::net::TcpStream::connect((endpoint.host.as_str(), endpoint.port))
        .await
        .map_err(|_| ProtocolError::Connection)?;
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
        let search = match query {
            Some(q) => format!("TEXT {}", quoted(q)?),
            None => "ALL".into(),
        };
        let state = Arc::new(State::default());
        self.operation(async{
   let mut s=self.session(state.clone()).await?;let mailbox=s.examine(folder).await.map_err(|_|ProtocolError::Rejected)?;let validity=mailbox.uid_validity.ok_or(ProtocolError::Rejected)?;
   let mut ids:Vec<u32>=s.uid_search(search).await.map_err(|_|ProtocolError::Rejected)?.into_iter().collect();ids.sort_unstable();let more=ids.len()>offset.saturating_add(limit);let wanted:Vec<u32>=ids.into_iter().skip(offset).take(limit).collect();let mut messages=Vec::new();
   if !wanted.is_empty(){let set=wanted.iter().map(u32::to_string).collect::<Vec<_>>().join(",");let mut stream=s.uid_fetch(set,"(UID RFC822.SIZE FLAGS BODY.PEEK[HEADER.FIELDS (FROM TO CC SUBJECT DATE MESSAGE-ID IN-REPLY-TO REFERENCES)])").await.map_err(|_|ProtocolError::Rejected)?;
    while let Some(fetch)=stream.try_next().await.map_err(|_|ProtocolError::Rejected)?{let uid=fetch.uid.ok_or(ProtocolError::Rejected)?;if !wanted.contains(&uid){continue;}
if messages.len()>=limit{return Err(ProtocolError::Limit);}let headers=fetch.header().ok_or(ProtocolError::Rejected)?;if headers.len()>MAX_HEADER{return Err(ProtocolError::Limit);}messages.push(RemoteMessage{uid,headers:headers.to_vec(),size:fetch.size.ok_or(ProtocolError::Rejected)?,flags:fetch.flags().map(|flag|match flag { async_imap::types::Flag::Custom(s)=>s.into_owned(),async_imap::types::Flag::MayCreate=>"\\*".into(),other=>format!("\\{other:?}") }).collect()});}
   }messages.sort_by_key(|m|m.uid);Ok(MessagePage{uid_validity:validity,messages,more})
  },&state)
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
        if !valid(folder, 1024) {
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
                let ids = s
                    .uid_search(query)
                    .await
                    .map_err(|_| ProtocolError::Rejected)?;
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
/// One strict implicit-TLS attempt. Callers must enforce authorization before invoking.
pub fn send_smtp(
    endpoint: &Endpoint,
    tls: &TlsOptions,
    credentials: &Credentials,
    from: &str,
    to: &[String],
    raw: &[u8],
) -> WriteOutcome {
    use lettre::transport::smtp::{
        authentication::{Credentials as SmtpCredentials, Mechanism},
        client::AsyncSmtpConnection,
        commands::{Data, Mail, Rcpt},
        extension::{ClientId, Extension, MailBodyParameter, MailParameter},
    };
    if !credentials_valid(credentials)
        || raw.is_empty()
        || raw.len() > MAX_RAW
        || to.is_empty()
        || to.len() > 100
        || !valid(from, 320)
        || to.iter().any(|s| !valid(s, 320))
    {
        return WriteOutcome::Failed;
    }
    let Ok(sender) = from.parse::<lettre::Address>() else {
        return WriteOutcome::Failed;
    };
    let Ok(recipients) = to
        .iter()
        .map(|s| s.parse::<lettre::Address>())
        .collect::<Result<Vec<_>, _>>()
    else {
        return WriteOutcome::Failed;
    };
    let state = Arc::new(State::default());
    let submitting = std::sync::atomic::AtomicBool::new(false);
    let result = run(async {
        let wire = connect(endpoint, tls, state.clone(), false).await?;
        let mut s = AsyncSmtpConnection::connect_with_transport(
            Box::new(wire),
            &ClientId::Domain("letterkeeper.invalid".into()),
        )
        .await
        .map_err(|_| ProtocolError::Connection)?;
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
            .map_err(|_| ProtocolError::Rejected)?;
        if !response.has_code(235) {
            return Err(ProtocolError::Rejected);
        }
        let mut options = Vec::new();
        if !raw.is_ascii() {
            if !s.server_info().supports_feature(Extension::EightBitMime) {
                return Err(ProtocolError::Rejected);
            }
            options.push(MailParameter::Body(MailBodyParameter::EightBitMime));
        }
        if !from.is_ascii() || to.iter().any(|s| !s.is_ascii()) {
            if !s.server_info().supports_feature(Extension::SmtpUtfEight) {
                return Err(ProtocolError::Rejected);
            }
            options.push(MailParameter::SmtpUtfEight);
        }
        s.command(Mail::new(Some(sender), options))
            .await
            .map_err(|_| ProtocolError::Rejected)?;
        for recipient in recipients {
            s.command(Rcpt::new(recipient, vec![]))
                .await
                .map_err(|_| ProtocolError::Rejected)?;
        }
        let response = s.command(Data).await.map_err(|_| ProtocolError::Rejected)?;
        if !response.has_code(354) {
            return Err(ProtocolError::Rejected);
        }
        submitting.store(true, Ordering::Relaxed);
        match s.message(raw).await {
            Ok(response) if response.has_code(250) => Ok(WriteOutcome::Accepted),
            Ok(_) => Ok(WriteOutcome::Uncertain),
            Err(error) if error.is_permanent() || error.is_transient() => Ok(WriteOutcome::Failed),
            Err(_) => Ok(WriteOutcome::Uncertain),
        }
    });
    match result {
        Ok(outcome) => outcome,
        Err(_) if submitting.load(Ordering::Relaxed) => WriteOutcome::Uncertain,
        Err(_) => WriteOutcome::Failed,
    }
}
