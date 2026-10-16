// SPDX-License-Identifier: GPL-3.0-only
use crate::{DraftInput, Error, Result};
use sha2::{Digest, Sha256};
pub(crate) fn check_page(offset: usize, limit: usize) -> Result<()> {
    if offset > 100_000 || !(1..=50).contains(&limit) {
        Err(Error::InvalidInput)
    } else {
        Ok(())
    }
}
pub(crate) fn validate_folder(folder: &str) -> Result<()> {
    if folder.is_empty() || !header(folder, 512) {
        Err(Error::InvalidInput)
    } else {
        Ok(())
    }
}
pub(crate) fn content_digest(revision: i64, input: &DraftInput) -> Result<String> {
    let bytes = serde_json::to_vec(&("letterkeeper-draft-v1", revision, input))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
fn dangerous(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200e}' | '\u{200f}' | '\u{061c}')
}
fn header(value: &str, max: usize) -> bool {
    value.len() <= max && !value.chars().any(dangerous)
}
fn address(value: &str) -> bool {
    if !header(value, 254) || !value.is_ascii() {
        return false;
    }
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && local.len() <= 64
        && !domain.is_empty()
        && !domain.contains('@')
        && local
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".!#$%&'*+-/=?^_`{|}~".contains(&c))
        && domain
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".-".contains(&c))
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !local.contains("..")
        && !domain.starts_with(['.', '-'])
        && !domain.ends_with(['.', '-'])
        && !domain.contains("..")
}
pub(crate) fn validate(input: &DraftInput) -> Result<()> {
    let recipients = input
        .to
        .len()
        .checked_add(input.cc.len())
        .and_then(|n| n.checked_add(input.bcc.len()))
        .ok_or(Error::InvalidInput)?;
    if input.account.is_empty()
        || !header(&input.account, 128)
        || !address(&input.from)
        || recipients == 0
        || recipients > 100
        || !input
            .to
            .iter()
            .chain(&input.cc)
            .chain(&input.bcc)
            .all(|s| address(s))
        || !header(&input.subject, 998)
        || input.body.len() > 1024 * 1024
        || input
            .body
            .chars()
            .any(|c| dangerous(c) && c != '\n' && c != '\t')
        || input.attachments.len() > 16
        || input
            .in_reply_to
            .as_deref()
            .is_some_and(|id| !message_id(id))
        || input.references.len() > 32
        || !input.references.iter().all(|id| message_id(id))
    {
        return Err(Error::InvalidInput);
    }
    let mut total = 0usize;
    for a in &input.attachments {
        if a.name.is_empty()
            || !header(&a.name, 255)
            || matches!(a.name.as_str(), "." | "..")
            || a.name.contains(['/', '\\'])
            || !header(&a.content_type, 127)
            || a.content_type.split('/').count() != 2
            || a.content_type.split('/').any(str::is_empty)
            || !a
                .content_type
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"!#$&^_.+-/".contains(&c))
        {
            return Err(Error::InvalidInput);
        }
        total = total
            .checked_add(a.bytes.len())
            .ok_or(Error::InvalidInput)?;
        if total > 10 * 1024 * 1024 {
            return Err(Error::InvalidInput);
        }
    }
    Ok(())
}
fn message_id(value: &str) -> bool {
    if !value.is_ascii() || value.len() > 256 {
        return false;
    }
    let Some(inner) = value.strip_prefix('<').and_then(|v| v.strip_suffix('>')) else {
        return false;
    };
    let Some((left, right)) = inner.split_once('@') else {
        return false;
    };
    let atom = |s: &str| {
        !s.is_empty()
            && s.split('.').all(|part| {
                !part.is_empty()
                    && part
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-/=?^_`{|}~".contains(&b))
            })
    };
    atom(left)
        && (atom(right)
            || right
                .strip_prefix('[')
                .and_then(|v| v.strip_suffix(']'))
                .is_some_and(|v| v.bytes().all(|b| matches!(b,33..=90|94..=126))))
}
