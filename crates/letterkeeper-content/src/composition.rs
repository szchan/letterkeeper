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
pub fn inspect_message(_: &[u8], _: Limits) -> Result<InspectedMail, ContentError> {
    Err(ContentError::InvalidMessage)
}
pub fn compose(
    _: &InspectedMail,
    _: &str,
    _: ComposeKind,
    _: &str,
) -> Result<ComposedDraft, ContentError> {
    Err(ContentError::InvalidMessage)
}
pub fn extract_attachment(
    _: &[u8],
    _: usize,
    _: Limits,
) -> Result<DecodedAttachment, ContentError> {
    Err(ContentError::InvalidMessage)
}
