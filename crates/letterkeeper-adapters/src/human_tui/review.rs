// SPDX-License-Identifier: GPL-3.0-only
use super::*;
/// Lossless terminal-safe spelling: backslashes and dangerous Unicode are escaped.
/// Newlines are retained for scrolling; all other controls have a reversible spelling.
pub fn review_escape(input: &str) -> String {
    let mut result = String::new();
    for c in input.chars() {
        if c == '\\' {
            result.push_str("\\\\");
        } else if dangerous(c) || (c.is_control() && c != '\n') {
            result.push_str(&format!("\\u{{{:04x}}}", c as u32));
        } else {
            result.push(c);
        }
    }
    result
}
pub(super) fn dangerous(c: char) -> bool {
    matches!(c, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}
pub fn review_text(review: &ReviewSnapshot) -> String {
    let draft = &review.draft;
    let i = &draft.input;
    let mut text = format!(
        "完整审阅 · 草稿 {} 版本 {}\n摘要: {}\nAccount: {}\nFrom: {}\nTo: {}\nCc: {}\nBcc: {}\nSubject: {}\nIn-Reply-To: {}\nReferences: {}\nAttachments: {}\n",
        draft.id,
        draft.revision,
        draft.digest,
        review_escape(&i.account),
        review_escape(&i.from),
        i.to.iter()
            .map(|s| review_escape(s))
            .collect::<Vec<_>>()
            .join(", "),
        i.cc.iter()
            .map(|s| review_escape(s))
            .collect::<Vec<_>>()
            .join(", "),
        i.bcc
            .iter()
            .map(|s| review_escape(s))
            .collect::<Vec<_>>()
            .join(", "),
        review_escape(&i.subject),
        i.in_reply_to
            .as_deref()
            .map(review_escape)
            .unwrap_or_default(),
        i.references
            .iter()
            .map(|s| review_escape(s))
            .collect::<Vec<_>>()
            .join(" "),
        i.attachments.len()
    );
    for (index, attachment) in i.attachments.iter().enumerate() {
        text.push_str(&format!(
            "{}: {} | {} | {} bytes | SHA-256 {}\n",
            index,
            review_escape(&attachment.name),
            review_escape(&attachment.content_type),
            attachment.bytes.len(),
            review
                .attachment_digests
                .get(index)
                .map(String::as_str)
                .unwrap_or("MISSING HASH — approval unavailable")
        ));
    }
    text.push_str("\nBody (exact; \\\\ = backslash, \\u{xxxx} = escaped control):\n");
    text.push_str(&review_escape(&i.body));
    text
}
