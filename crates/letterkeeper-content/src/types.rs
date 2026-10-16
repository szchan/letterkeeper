// SPDX-License-Identifier: GPL-3.0-only
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
    #[error("invalid reply metadata")]
    InvalidMetadata,
    #[error("invalid recipient")]
    InvalidRecipient,
    #[error("attachment not found")]
    AttachmentNotFound,
    #[error("decoded attachment exceeds limit")]
    AttachmentTooLarge,
    #[error("attachment decoding failed")]
    AttachmentDecodeFailed,
    #[error("draft exceeds limit")]
    DraftTooLarge,
}
