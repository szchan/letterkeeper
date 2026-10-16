// SPDX-License-Identifier: GPL-3.0-only
use crate::html::render_html;
use crate::{AttachmentInfo, ContentError, Limits, SafeDocument};
use mail_parser::{Message, MessageParser, MessagePart, MimeHeaders, PartType};

/// Remove terminal controls and bidirectional formatting controls. This returns
/// plain text: consumers must not interpret it as HTML, Markdown or commands.
pub fn sanitize_text(input: &str) -> String {
    input.replace("\r\n", "\n").chars().filter(|c| {
        (!c.is_control() || matches!(c, '\n' | '\t'))
            && !matches!(*c as u32, 0x061c | 0x200e..=0x200f | 0x202a..=0x202e | 0x2066..=0x206f | 0xfeff)
    }).collect()
}

pub(crate) struct Budget {
    pub(crate) remaining: usize,
    pub(crate) truncated: bool,
}
impl Budget {
    pub(crate) fn take(&mut self, input: &str, cap: usize) -> String {
        let safe = sanitize_text(input);
        let count = safe.chars().count();
        let limit = self.remaining.min(cap);
        self.truncated |= count > limit;
        let result: String = safe.chars().take(limit).collect();
        self.remaining -= result.chars().count();
        result
    }
}

/// Parse only in the application's isolated worker. Raw length is checked before
/// MIME parsing; parser allocation/CPU are bounded by the worker, not this API.
/// The output character budget is shared by all display fields (warnings contain
/// only fixed application text). No parser here has a resource fetching API.
pub fn parse_message(raw: &[u8], limits: Limits) -> Result<SafeDocument, ContentError> {
    if [
        limits.max_raw_bytes,
        limits.max_output_chars,
        limits.max_mime_parts,
        limits.max_html_depth,
    ]
    .contains(&0)
    {
        return Err(ContentError::InvalidLimits);
    }
    if raw.len() > limits.max_raw_bytes {
        return Err(ContentError::RawTooLarge);
    }
    if raw.is_empty() {
        return Err(ContentError::InvalidMessage);
    }
    let message = MessageParser::default()
        .parse(raw)
        .ok_or(ContentError::InvalidMessage)?;
    let mut count = 0usize;
    let mut messages = vec![&message];
    while let Some(msg) = messages.pop() {
        count = count.saturating_add(msg.parts.len());
        if count > limits.max_mime_parts {
            return Err(ContentError::MimePartsExceeded);
        }
        for part in &msg.parts {
            if let PartType::Message(nested) = &part.body {
                messages.push(nested);
            }
        }
    }
    let mut budget = Budget {
        remaining: limits.max_output_chars,
        truncated: false,
    };
    let subject = budget.take(message.subject().unwrap_or(""), 512);
    let sender = message
        .from()
        .map(|a| {
            a.iter()
                .map(
                    |addr| match (addr.name.as_deref(), addr.address.as_deref()) {
                        (Some(name), Some(address)) => format!("{name} <{address}>"),
                        (None, Some(address)) => address.to_owned(),
                        (Some(name), None) => name.to_owned(),
                        _ => String::new(),
                    },
                )
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    let from = budget.take(&sender, 512);
    let mut selected = Vec::new();
    select_body(&message, 0, false, &mut selected);
    let mut text = String::new();
    let mut links = Vec::new();
    for part in selected {
        if !text.is_empty() {
            text.push_str(&budget.take("\n", usize::MAX));
        }
        match &part.body {
            PartType::Text(body) => text.push_str(&budget.take(body, usize::MAX)),
            PartType::Html(body) => render_html(
                body,
                limits.max_html_depth,
                &mut budget,
                &mut text,
                &mut links,
            )?,
            _ => {}
        }
    }
    let mut attachments = Vec::new();
    for (index, part) in message
        .parts
        .iter()
        .enumerate()
        .filter(|(_, p)| is_attachment(p))
    {
        let name = part
            .attachment_name()
            .unwrap_or("unnamed")
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("unnamed");
        let name = if name.is_empty() || name == "." || name == ".." {
            "unnamed"
        } else {
            name
        };
        let media = part
            .content_type()
            .map(|c| {
                format!(
                    "{}/{}",
                    c.c_type,
                    c.c_subtype.as_deref().unwrap_or("octet-stream")
                )
            })
            .unwrap_or_else(|| "application/octet-stream".to_owned());
        let safe_name = sanitize_text(name);
        let safe_media = sanitize_text(&media);
        let name: String = safe_name.chars().take(256).collect();
        let name = if name.is_empty() || name == "." || name == ".." {
            "unnamed".to_owned()
        } else {
            name
        };
        let media_type: String = safe_media.chars().take(128).collect();
        if safe_name.chars().count() > 256 || safe_media.chars().count() > 128 {
            budget.truncated = true;
        }
        let need = name.chars().count() + media_type.chars().count();
        if need > budget.remaining {
            budget.truncated = true;
            break;
        }
        budget.remaining -= need;
        attachments.push(AttachmentInfo {
            id: index,
            name,
            media_type,
            size: part.body.len(),
        });
    }
    let mut warnings = Vec::new();
    if budget.truncated {
        warnings.push("Display character limit reached; content was truncated.".to_owned());
    }
    if message.parts.iter().any(|p| p.is_encoding_problem) {
        warnings.push("MIME decoding reported malformed encoding.".to_owned());
    }
    Ok(SafeDocument {
        subject,
        from,
        text: text.trim().to_owned(),
        links,
        attachments,
        truncated: budget.truncated,
        warnings,
    })
}

pub(crate) fn is_attachment(part: &MessagePart<'_>) -> bool {
    part.attachment_name().is_some()
        || part
            .content_disposition()
            .is_some_and(|c| c.c_type.eq_ignore_ascii_case("attachment"))
        || matches!(
            part.body,
            PartType::Binary(_) | PartType::InlineBinary(_) | PartType::Message(_)
        )
}
fn has_plain(message: &Message<'_>, index: u32) -> bool {
    let Some(part) = message.parts.get(index as usize) else {
        return false;
    };
    if is_attachment(part) {
        return false;
    }
    match &part.body {
        PartType::Text(_) => part.content_type().is_none() || part.is_content_type("text", "plain"),
        PartType::Multipart(children) => children.iter().any(|i| has_plain(message, *i)),
        _ => false,
    }
}
fn has_display_body(message: &Message<'_>, index: u32) -> bool {
    let Some(part) = message.parts.get(index as usize) else {
        return false;
    };
    if is_attachment(part) {
        return false;
    }
    match &part.body {
        PartType::Text(_) => part.content_type().is_none() || part.is_content_type("text", "plain"),
        PartType::Html(_) => true,
        PartType::Multipart(children) => children.iter().any(|i| has_display_body(message, *i)),
        _ => false,
    }
}
fn select_body<'a, 'raw>(
    message: &'a Message<'raw>,
    index: u32,
    plain_only: bool,
    out: &mut Vec<&'a MessagePart<'raw>>,
) {
    let Some(part) = message.parts.get(index as usize) else {
        return;
    };
    if is_attachment(part) {
        return;
    }
    match &part.body {
        PartType::Text(_)
            if part.content_type().is_none() || part.is_content_type("text", "plain") =>
        {
            out.push(part)
        }
        PartType::Html(_) if !plain_only => out.push(part),
        PartType::Multipart(children) => {
            if part.is_content_type("multipart", "alternative") {
                if let Some(child) = children.iter().find(|i| has_plain(message, **i)) {
                    select_body(message, *child, true, out);
                } else if !plain_only
                    && let Some(child) = children.iter().find(|i| has_display_body(message, **i))
                {
                    select_body(message, *child, false, out);
                }
            } else {
                for child in children {
                    select_body(message, *child, plain_only, out);
                }
            }
        }
        _ => {}
    }
}
