// SPDX-License-Identifier: GPL-3.0-only
use crate::{ContentError, Limits, SafeDocument};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplyMetadata {
    pub from: Vec<String>,
    pub reply_to: Vec<String>,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub message_id: Option<String>,
    pub references: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InspectedMail {
    pub document: SafeDocument,
    pub reply: ReplyMetadata,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ComposeKind {
    Reply,
    ReplyAll,
    Forward,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComposedDraft {
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub body: String,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecodedAttachment {
    pub name: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
}
pub const MAX_ATTACHMENT_BYTES: usize = 512 * 1024;

use crate::{is_attachment, parse_message, sanitize_text};
use mail_parser::{Address, Encoding, HeaderName, MessageParser, MimeHeaders, PartType};
use std::collections::HashSet;

fn byte_prefix(value: &str, max: usize) -> &str {
    let mut end = value.len().min(max);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}
fn header_text(value: &str) -> String {
    sanitize_text(value).replace(['\n', '\t'], " ")
}
fn mailbox(value: &str) -> bool {
    if value.len() > 254 || !value.is_ascii() {
        return false;
    }
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && local.len() <= 64
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !local.contains("..")
        && local
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".!#$%&'*+-/=?^_`{|}~".contains(&b))
        && !domain.is_empty()
        && domain.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}
fn addresses(value: Option<&Address<'_>>) -> Result<Vec<String>, ContentError> {
    let mut result = Vec::new();
    if let Some(value) = value {
        for addr in value.iter() {
            let addr = addr
                .address
                .as_deref()
                .ok_or(ContentError::InvalidRecipient)?;
            if !mailbox(addr) || result.len() >= 64 {
                return Err(ContentError::InvalidRecipient);
            }
            result.push(addr.to_owned());
        }
    }
    Ok(result)
}
fn valid_id(value: &str) -> bool {
    value.len() <= 256
        && value.starts_with('<')
        && value.ends_with('>')
        && mailbox(&value[1..value.len() - 1])
}
fn bound_references(ids: &mut Vec<String>) {
    if ids.len() > 32 {
        ids.drain(1..ids.len() - 31);
    }
}
// A deliberately conservative msg-id grammar. Folded whitespace is permitted
// between ids, never inside them; malformed values fail instead of being repaired.
fn raw_ids(value: &str) -> Result<Vec<String>, ContentError> {
    if value.len() > 16 * 1024 {
        return Err(ContentError::InvalidMetadata);
    }
    let value = value.trim_end_matches(['\r', '\n']);
    let mut unfolded = String::new();
    let mut lines = value.split('\n');
    if let Some(first) = lines.next() {
        unfolded.push_str(first.trim_end_matches('\r'));
    }
    for line in lines {
        if !line.starts_with([' ', '\t']) {
            return Err(ContentError::InvalidMetadata);
        }
        unfolded.push(' ');
        unfolded.push_str(line.trim_end_matches('\r').trim_start());
    }
    if unfolded.chars().any(|c| c.is_control() && c != '\t') {
        return Err(ContentError::InvalidMetadata);
    }
    let mut result = Vec::new();
    for id in unfolded.split_whitespace() {
        if !valid_id(id) {
            return Err(ContentError::InvalidMetadata);
        }
        if !result.iter().any(|v| v == id) {
            result.push(id.to_owned());
        }
    }
    Ok(result)
}
/// Inspect raw MIME only within the application's isolated worker. Metadata is
/// untrusted data and does not authorize sending, links, or attachment handling.
pub fn inspect_message(raw: &[u8], limits: Limits) -> Result<InspectedMail, ContentError> {
    let document = parse_message(raw, limits)?;
    let message = MessageParser::default()
        .parse(raw)
        .ok_or(ContentError::InvalidMessage)?;
    for name in [
        HeaderName::From,
        HeaderName::ReplyTo,
        HeaderName::To,
        HeaderName::Cc,
        HeaderName::Subject,
        HeaderName::MessageId,
        HeaderName::References,
    ] {
        if message.header_values(name).count() > 1 {
            return Err(ContentError::InvalidMetadata);
        }
    }
    let mut ids = message
        .header_raw(HeaderName::MessageId)
        .map(raw_ids)
        .transpose()?
        .unwrap_or_default();
    if ids.len() > 1 {
        return Err(ContentError::InvalidMetadata);
    }
    let mut references = message
        .header_raw(HeaderName::References)
        .map(raw_ids)
        .transpose()?
        .unwrap_or_default();
    bound_references(&mut references);
    let reply = ReplyMetadata {
        from: addresses(message.from())?,
        reply_to: addresses(message.reply_to())?,
        to: addresses(message.to())?,
        cc: addresses(message.cc())?,
        subject: byte_prefix(&header_text(message.subject().unwrap_or("")), 990).to_owned(),
        message_id: ids.pop(),
        references,
    };
    Ok(InspectedMail { document, reply })
}
/// Compose a plain-text draft only; no attachment inclusion or external action.
/// Revalidate metadata because callers may deserialize or modify InspectedMail.
pub fn compose(
    mail: &InspectedMail,
    identity: &str,
    kind: ComposeKind,
    body: &str,
) -> Result<ComposedDraft, ContentError> {
    if !mailbox(identity) {
        return Err(ContentError::InvalidRecipient);
    }
    let meta = &mail.reply;
    if meta.references.len() > 32
        || meta.references.iter().any(|id| !valid_id(id))
        || meta.message_id.as_ref().is_some_and(|id| !valid_id(id))
        || meta.subject.len() > 990
        || header_text(&meta.subject) != meta.subject
    {
        return Err(ContentError::InvalidMetadata);
    }
    for list in [&meta.from, &meta.reply_to, &meta.to, &meta.cc] {
        if list.len() > 64 || list.iter().any(|addr| !mailbox(addr)) {
            return Err(ContentError::InvalidRecipient);
        }
    }
    if body.len() > 1024 * 1024 || mail.document.text.len() > 1024 * 1024 {
        return Err(ContentError::DraftTooLarge);
    }
    let forward = kind == ComposeKind::Forward;
    let mut to = Vec::new();
    let mut cc = Vec::new();
    let mut seen = HashSet::from([identity.to_ascii_lowercase()]);
    if !forward {
        let targets = if meta.reply_to.is_empty() {
            &meta.from
        } else {
            &meta.reply_to
        };
        for addr in targets {
            if seen.insert(addr.to_ascii_lowercase()) {
                to.push(addr.clone());
            }
        }
        if kind == ComposeKind::ReplyAll {
            for addr in &meta.to {
                if seen.insert(addr.to_ascii_lowercase()) {
                    to.push(addr.clone());
                }
            }
            for addr in &meta.cc {
                if seen.insert(addr.to_ascii_lowercase()) {
                    cc.push(addr.clone());
                }
            }
        }
        if to.is_empty() && cc.is_empty() {
            return Err(ContentError::InvalidRecipient);
        }
    }
    if to.len() + cc.len() > 100 {
        return Err(ContentError::InvalidRecipient);
    }
    let prefix = if forward { "Fwd:" } else { "Re:" };
    let subject = if meta
        .subject
        .get(..prefix.len())
        .is_some_and(|p| p.eq_ignore_ascii_case(prefix))
    {
        meta.subject.clone()
    } else {
        format!("{prefix} {}", meta.subject)
    };
    let mut references = if forward {
        Vec::new()
    } else {
        meta.references.clone()
    };
    let in_reply_to = if forward {
        None
    } else {
        meta.message_id.clone()
    };
    if let Some(id) = &in_reply_to {
        references.retain(|v| v != id);
        references.push(id.clone());
        bound_references(&mut references);
    }
    let sender = header_text(&mail.document.from);
    let mut result = format!(
        "{}\n\nOriginal message (untrusted) from {}:\n",
        sanitize_text(body),
        sender.chars().take(512).collect::<String>()
    );
    if forward {
        result.push_str(&format!("Subject: {}\n", meta.subject));
    }
    for line in sanitize_text(&mail.document.text).lines() {
        result.push_str("> ");
        result.push_str(line);
        result.push('\n');
    }
    if mail.document.truncated {
        result.push_str("[Original message display was truncated]\n");
    }
    if result.len() > 1024 * 1024 {
        return Err(ContentError::DraftTooLarge);
    }
    Ok(ComposedDraft {
        to,
        cc,
        subject,
        body: result,
        in_reply_to,
        references,
    })
}
fn filename(value: &str) -> String {
    let safe = header_text(value);
    let name = safe.rsplit(['/', '\\']).next().unwrap_or("").trim();
    let name = byte_prefix(name, 255).to_owned();
    if name.is_empty() || name == "." || name == ".." {
        "unnamed".to_owned()
    } else {
        name
    }
}
fn media_type(part: &mail_parser::MessagePart<'_>) -> String {
    let Some(media) = part.content_type() else {
        return "application/octet-stream".to_owned();
    };
    let token = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$&^_.+-".contains(&b))
    };
    let subtype = media.c_subtype.as_deref().unwrap_or("octet-stream");
    if media.c_type.len() + subtype.len() + 1 > 127 || !token(&media.c_type) || !token(subtype) {
        "application/octet-stream".to_owned()
    } else {
        format!("{}/{subtype}", media.c_type).to_ascii_lowercase()
    }
}
/// Extract one explicitly selected top-level part ID, matching SafeDocument.
/// No archives are expanded, files written, nested messages traversed, or viewers
/// started. Transport decoding preserves original bytes (no charset conversion).
pub fn extract_attachment(
    raw: &[u8],
    id: usize,
    limits: Limits,
) -> Result<DecodedAttachment, ContentError> {
    let document = parse_message(raw, limits)?;
    if !document.attachments.iter().any(|part| part.id == id) {
        return Err(ContentError::AttachmentNotFound);
    }
    let message = MessageParser::default()
        .parse(raw)
        .ok_or(ContentError::InvalidMessage)?;
    let part = message
        .parts
        .get(id)
        .filter(|part| is_attachment(part))
        .ok_or(ContentError::AttachmentNotFound)?;
    if part
        .headers
        .iter()
        .filter(|header| header.name == HeaderName::ContentTransferEncoding)
        .count()
        > 1
        || part.content_transfer_encoding().is_some_and(|encoding| {
            !["7bit", "8bit", "binary", "base64", "quoted-printable"]
                .iter()
                .any(|known| encoding.eq_ignore_ascii_case(known))
        })
    {
        return Err(ContentError::AttachmentDecodeFailed);
    }
    if part.is_encoding_problem {
        return Err(ContentError::AttachmentDecodeFailed);
    }
    if matches!(part.body, PartType::Message(_) | PartType::Multipart(_)) {
        return Err(ContentError::AttachmentDecodeFailed);
    }
    let encoded = raw
        .get(part.offset_body as usize..part.offset_end as usize)
        .ok_or(ContentError::AttachmentDecodeFailed)?;
    let bytes = match part.encoding {
        Encoding::Base64 => mail_parser::decoders::base64::base64_decode(encoded)
            .ok_or(ContentError::AttachmentDecodeFailed)?,
        Encoding::QuotedPrintable => {
            mail_parser::decoders::quoted_printable::quoted_printable_decode(encoded)
                .ok_or(ContentError::AttachmentDecodeFailed)?
        }
        Encoding::None => encoded.to_vec(),
    };
    if bytes.len() > MAX_ATTACHMENT_BYTES {
        return Err(ContentError::AttachmentTooLarge);
    }
    Ok(DecodedAttachment {
        name: filename(part.attachment_name().unwrap_or("unnamed")),
        media_type: media_type(part),
        bytes,
    })
}
