// SPDX-License-Identifier: GPL-3.0-only
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    pub name: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftInput {
    pub account: String,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub body: String,
    pub attachments: Vec<Attachment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_reply_to: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draft {
    pub id: i64,
    pub revision: i64,
    pub digest: String,
    pub input: DraftInput,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DraftListEntry {
    pub id: i64,
    pub revision: i64,
    pub digest: String,
    pub subject: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubmissionState {
    Submitting,
    Accepted,
    Failed,
    Uncertain,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendOutcome {
    Accepted,
    Failed,
    Uncertain,
}
/// Fixed, non-sensitive point at which a send attempt failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SendStage {
    Preparation,
    TlsConfiguration,
    Credentials,
    Connection,
    TlsHandshake,
    Greeting,
    Authentication,
    MailFrom,
    Recipient,
    DataCommand,
    Submission,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SendFailureKind {
    InvalidInput,
    Connection,
    Timeout,
    Rejected,
    UnexpectedResponse,
    Unavailable,
}
/// No server-provided strings, addresses or credentials may be stored here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SendDiagnostic {
    pub stage: SendStage,
    pub kind: SendFailureKind,
    #[serde(deserialize_with = "deserialize_smtp_code")]
    pub smtp_code: Option<u16>,
}
fn deserialize_smtp_code<'de, D>(deserializer: D) -> std::result::Result<Option<u16>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let code = Option::<u16>::deserialize(deserializer)?;
    if code.is_some_and(|code| !(100..=599).contains(&code)) {
        return Err(serde::de::Error::custom("SMTP code outside allowed range"));
    }
    Ok(code)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Submission {
    pub id: i64,
    pub draft_id: i64,
    pub revision: i64,
    pub digest: String,
    pub message_id: String,
    pub input: DraftInput,
    pub state: SubmissionState,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmissionListEntry {
    pub id: i64,
    pub draft_id: i64,
    pub state: SubmissionState,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncState {
    Pending,
    Confirmed,
    Failed,
    Uncertain,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncKind {
    Draft,
    Sent,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncRecord {
    pub id: i64,
    pub kind: SyncKind,
    pub local_id: i64,
    pub revision: i64,
    pub account: String,
    pub folder: String,
    pub message_id: String,
    pub state: SyncState,
    pub digest: String,
    pub input: DraftInput,
}
