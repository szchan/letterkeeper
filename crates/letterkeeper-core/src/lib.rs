// SPDX-License-Identifier: GPL-3.0-only
//! Shared draft, authorization and submission state.
use serde::{Deserialize, Serialize};
use std::path::Path;

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
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draft {
    pub id: i64,
    pub revision: i64,
    pub digest: String,
    pub input: DraftInput,
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
#[derive(Clone, Copy, Debug)]
pub struct Permissions {
    pub read: bool,
    pub draft: bool,
    pub send: bool,
}
pub struct AccessPolicy;
impl AccessPolicy {
    pub fn new(_accounts: Vec<String>, _permissions: Permissions) -> Self {
        Self
    }
}
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("core operation unavailable")]
    Unavailable,
}
pub type Result<T> = std::result::Result<T, Error>;
pub struct Store;
impl Store {
    pub fn memory() -> Result<Self> {
        Ok(Self)
    }
    pub fn open(_path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self)
    }
    pub fn with_policy(self, _policy: AccessPolicy) -> Self {
        self
    }
    pub fn create_draft(&mut self, _input: DraftInput) -> Result<Draft> {
        Err(Error::Unavailable)
    }
    pub fn get_draft(&self, _id: i64) -> Result<Draft> {
        Err(Error::Unavailable)
    }
    pub fn update_draft(
        &mut self,
        _id: i64,
        _expected_revision: i64,
        _input: DraftInput,
    ) -> Result<Draft> {
        Err(Error::Unavailable)
    }
    pub fn approve(&mut self, _id: i64, _expected_digest: &str, _expires_at: i64) -> Result<()> {
        Err(Error::Unavailable)
    }
    pub fn claim_send(&mut self, _id: i64, _now: i64) -> Result<Submission> {
        Err(Error::Unavailable)
    }
    pub fn finish_submission(&mut self, _id: i64, _outcome: SendOutcome) -> Result<Submission> {
        Err(Error::Unavailable)
    }
    pub fn get_submission(&self, _id: i64) -> Result<Submission> {
        Err(Error::Unavailable)
    }
    pub fn recover_interrupted(&mut self, _id: i64) -> Result<Submission> {
        Err(Error::Unavailable)
    }
}
