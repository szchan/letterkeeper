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
