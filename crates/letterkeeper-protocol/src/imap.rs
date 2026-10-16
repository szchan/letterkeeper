// SPDX-License-Identifier: GPL-3.0-only
use crate::runtime::run;
use crate::tls::connect;
use crate::validation::{credentials_valid, quoted, valid};
use crate::wire::{MAX_HEADER, MAX_RAW, State, Wire};
use crate::{
    Credentials, Endpoint, Folder, MessagePage, ProtocolError, RemoteKey, RemoteMessage,
    TlsOptions, WriteOutcome,
};
use futures_util::TryStreamExt;
use std::{
    future::Future,
    sync::{Arc, atomic::Ordering},
};
pub struct ImapClient {
    endpoint: Endpoint,
    tls: TlsOptions,
    credentials: Credentials,
}
type Session = async_imap::Session<Wire>;
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
