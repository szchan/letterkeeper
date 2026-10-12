// SPDX-License-Identifier: GPL-3.0-only
//! Resource-free parsing. Production entry points must use the isolated worker.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SafeDocument {
    pub subject: String,
    pub from: String,
    pub text: String,
    pub links: Vec<SafeLink>,
    pub attachments: Vec<AttachmentInfo>,
    pub truncated: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SafeLink {
    pub id: usize,
    pub label: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttachmentInfo {
    pub id: usize,
    pub name: String,
    pub media_type: String,
    pub size: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Limits {
    pub max_raw_bytes: usize,
    pub max_output_chars: usize,
    pub max_mime_parts: usize,
    pub max_html_depth: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_raw_bytes: 2 * 1024 * 1024,
            max_output_chars: 64 * 1024,
            max_mime_parts: 128,
            max_html_depth: 64,
        }
    }
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ContentError {
    #[error("invalid message")]
    InvalidMessage,
    #[error("raw message exceeds limit")]
    RawTooLarge,
    #[error("MIME part count exceeds limit")]
    MimePartsExceeded,
    #[error("HTML nesting exceeds limit")]
    HtmlDepthExceeded,
    #[error("limits must be positive")]
    InvalidLimits,
}

use html5ever::{parse_document, tendril::TendrilSink};
use mail_parser::{Message, MessageParser, MessagePart, MimeHeaders, PartType};
use markup5ever_rcdom::{Handle, NodeData, RcDom};

/// Remove terminal controls and bidirectional formatting controls. This returns
/// plain text: consumers must not interpret it as HTML, Markdown or commands.
pub fn sanitize_text(input: &str) -> String {
    input.replace("\r\n", "\n").chars().filter(|c| {
        (!c.is_control() || matches!(c, '\n' | '\t'))
            && !matches!(*c as u32, 0x061c | 0x200e..=0x200f | 0x202a..=0x202e | 0x2066..=0x206f | 0xfeff)
    }).collect()
}

struct Budget {
    remaining: usize,
    truncated: bool,
}
impl Budget {
    fn take(&mut self, input: &str, cap: usize) -> String {
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

fn is_attachment(part: &MessagePart<'_>) -> bool {
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

fn blocked(tag: &str) -> bool {
    matches!(
        tag,
        "script"
            | "style"
            | "svg"
            | "math"
            | "iframe"
            | "object"
            | "embed"
            | "form"
            | "input"
            | "button"
            | "textarea"
            | "select"
            | "template"
            | "head"
            | "link"
            | "meta"
            | "base"
            | "noscript"
            | "audio"
            | "video"
            | "source"
            | "track"
    )
}
fn block(tag: &str) -> bool {
    matches!(
        tag,
        "p" | "div"
            | "section"
            | "article"
            | "header"
            | "footer"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "blockquote"
            | "pre"
            | "ul"
            | "ol"
            | "li"
            | "table"
            | "tr"
    )
}
fn safe_target(target: &str) -> bool {
    if target.chars().count() > 2048
        || target
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '\\' | '<' | '>' | '"' | '\''))
        || sanitize_text(target) != target
    {
        return false;
    }
    let lower = target.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") {
        let Some((_, rest)) = target.split_once("://") else {
            return false;
        };
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
        !authority.is_empty()
            && !authority.contains('@')
            && !authority.contains('%')
            && !authority.starts_with(':')
            && !authority.ends_with(':')
    } else {
        lower.starts_with("mailto:") && target.len() > 7 && !target[7..].contains(['?', '#', ':'])
    }
}
fn newline(text: &mut String, budget: &mut Budget) {
    if !text.is_empty() && !text.ends_with('\n') {
        text.push_str(&budget.take("\n", usize::MAX));
    }
}
enum Visit {
    Enter(Handle),
    Exit {
        tag: String,
        link: Option<(String, usize)>,
    },
}
fn render_html(
    html: &str,
    max_depth: usize,
    budget: &mut Budget,
    text: &mut String,
    links: &mut Vec<SafeLink>,
) -> Result<(), ContentError> {
    let dom = parse_document(RcDom::default(), Default::default()).one(html);
    // Check all nodes, including discarded subtrees, before emitting any HTML text.
    let mut depth_stack = vec![(dom.document.clone(), 0usize)];
    while let Some((node, depth)) = depth_stack.pop() {
        if depth > max_depth {
            return Err(ContentError::HtmlDepthExceeded);
        }
        for child in node.children.borrow().iter() {
            depth_stack.push((child.clone(), depth + 1));
        }
    }
    let mut stack = vec![Visit::Enter(dom.document.clone())];
    while let Some(visit) = stack.pop() {
        match visit {
            Visit::Enter(node) => {
                let mut exit = None;
                match &node.data {
                    NodeData::Text { contents } => {
                        let safe = sanitize_text(&contents.borrow());
                        let mut normalized = String::new();
                        let mut whitespace =
                            text.ends_with(|c: char| c.is_whitespace()) || text.is_empty();
                        for c in safe.chars() {
                            if c.is_whitespace() {
                                if !whitespace {
                                    normalized.push(' ');
                                }
                                whitespace = true;
                            } else {
                                normalized.push(c);
                                whitespace = false;
                            }
                        }
                        text.push_str(&budget.take(&normalized, usize::MAX));
                    }
                    NodeData::Element { name, attrs, .. } => {
                        let tag = name.local.as_ref();
                        if name.ns.as_ref() != "http://www.w3.org/1999/xhtml" || blocked(tag) {
                            continue;
                        }
                        if block(tag) {
                            newline(text, budget);
                        }
                        if tag == "br" || tag == "hr" {
                            newline(text, budget);
                        }
                        if tag == "li" {
                            text.push_str(&budget.take("- ", usize::MAX));
                        }
                        if matches!(tag, "td" | "th") && !text.is_empty() && !text.ends_with('\n') {
                            text.push_str(&budget.take(" | ", usize::MAX));
                        }
                        if tag == "img" {
                            let attributes = attrs.borrow();
                            let src = attributes
                                .iter()
                                .find(|a| a.name.local.as_ref() == "src")
                                .map(|a| a.value.to_ascii_lowercase());
                            let has_srcset =
                                attributes.iter().any(|a| a.name.local.as_ref() == "srcset");
                            let placeholder = match src.as_deref() {
                                Some(s) if s.starts_with("cid:") => "[CID image omitted]",
                                Some(s)
                                    if s.starts_with("http:")
                                        || s.starts_with("https:")
                                        || s.starts_with("//") =>
                                {
                                    "[remote image blocked]"
                                }
                                _ if has_srcset => "[remote image blocked]",
                                _ => "[image omitted]",
                            };
                            text.push_str(&budget.take(placeholder, usize::MAX));
                        }
                        let link = if tag == "a" {
                            attrs
                                .borrow()
                                .iter()
                                .find(|a| a.name.local.as_ref() == "href")
                                .map(|a| a.value.to_string())
                                .filter(|s| safe_target(s))
                                .map(|s| (s, text.len()))
                        } else {
                            None
                        };
                        exit = Some(Visit::Exit {
                            tag: tag.to_owned(),
                            link,
                        });
                    }
                    _ => {}
                }
                if let Some(exit) = exit {
                    stack.push(exit);
                }
                for child in node.children.borrow().iter().rev() {
                    stack.push(Visit::Enter(child.clone()));
                }
            }
            Visit::Exit { tag, link } => {
                if let Some((target, start)) = link {
                    let label: String = text[start..].trim().chars().take(256).collect();
                    let id = links.len() + 1;
                    let marker = format!(" [{id}]");
                    let need =
                        target.chars().count() + label.chars().count() + marker.chars().count();
                    if links.len() < 128 && budget.remaining >= need {
                        let target = budget.take(&target, usize::MAX);
                        let label = budget.take(&label, usize::MAX);
                        text.push_str(&budget.take(&marker, usize::MAX));
                        links.push(SafeLink { id, label, target });
                    } else {
                        budget.truncated = true;
                    }
                }
                if block(&tag) {
                    newline(text, budget);
                }
            }
        }
    }
    Ok(())
}
