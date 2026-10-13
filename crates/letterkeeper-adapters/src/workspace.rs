// SPDX-License-Identifier: GPL-3.0-only
//! Trusted human workflow. This capability is never passed to the MCP adapter.
use letterkeeper_core::{Draft, DraftInput, SubmissionState, SyncState};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceCapabilities {
    pub account: String,
    pub from: String,
    pub read: bool,
    pub draft: bool,
    pub send: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftSummary {
    pub id: i64,
    pub revision: i64,
    pub subject: String,
    pub digest: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmissionInfo {
    pub id: i64,
    pub draft_id: i64,
    pub state: SubmissionState,
    pub state_recorded: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewSnapshot {
    pub draft: Draft,
    /// SHA-256 of each attachment's exact bytes, in attachment order.
    pub attachment_digests: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalInfo {
    pub draft_id: i64,
    pub revision: i64,
    pub digest: String,
    pub expires_at: i64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentEvidence {
    pub submission_id: i64,
    pub matching_messages: usize,
    // Observation only; no delivery-confirmed field is offered.
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncInfo {
    pub id: i64,
    pub state: SyncState,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyMode {
    Reply,
    ReplyAll,
    Forward,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WorkflowError {
    #[error("permission_denied")]
    Denied,
    #[error("revision_conflict")]
    Conflict,
    #[error("invalid_input")]
    InvalidInput,
    #[error("approval_required")]
    ApprovalRequired,
    #[error("operation_unavailable")]
    Unavailable,
    #[error("unsupported_operation")]
    Unsupported,
}
pub type WorkflowResult<T> = Result<T, WorkflowError>;
/// Application injects this trait into a verified interactive terminal only.
/// It shares storage and sending with Host but is not a model-facing API.
pub trait HumanWorkspace: Send + Sync {
    fn capabilities(&self) -> WorkspaceCapabilities;
    fn list_drafts(&self, _offset: usize, _limit: usize) -> WorkflowResult<Vec<DraftSummary>> {
        Err(WorkflowError::Unsupported)
    }
    fn load_draft(&self, _id: i64) -> WorkflowResult<Draft> {
        Err(WorkflowError::Unsupported)
    }
    fn create(&self, _input: DraftInput) -> WorkflowResult<Draft> {
        Err(WorkflowError::Unsupported)
    }
    fn save(&self, _id: i64, _expected_revision: i64, _input: DraftInput) -> WorkflowResult<Draft> {
        Err(WorkflowError::Unsupported)
    }
    fn reply(
        &self,
        _message_id: &str,
        _mode: ReplyMode,
        _body: &str,
        _to: &[String],
    ) -> WorkflowResult<Draft> {
        Err(WorkflowError::Unsupported)
    }
    fn add_attachment(
        &self,
        _id: i64,
        _expected_revision: i64,
        _path: &Path,
        _content_type: &str,
    ) -> WorkflowResult<Draft> {
        Err(WorkflowError::Unsupported)
    }
    fn remove_attachment(
        &self,
        _id: i64,
        _expected_revision: i64,
        _index: usize,
    ) -> WorkflowResult<Draft> {
        Err(WorkflowError::Unsupported)
    }
    fn export_attachment(
        &self,
        _message_id: &str,
        _part: usize,
        _directory: &Path,
    ) -> WorkflowResult<String> {
        Err(WorkflowError::Unsupported)
    }
    fn review(&self, _id: i64) -> WorkflowResult<ReviewSnapshot> {
        Err(WorkflowError::Unsupported)
    }
    /// Checks typed digest, reloads current revision/digest, grants exactly 300s.
    fn approve(
        &self,
        _id: i64,
        _expected_digest: &str,
        _typed_digest: &str,
    ) -> WorkflowResult<ApprovalInfo> {
        Err(WorkflowError::Unsupported)
    }
    fn send(&self, _id: i64) -> WorkflowResult<SubmissionInfo> {
        Err(WorkflowError::Unsupported)
    }
    fn list_submissions(
        &self,
        _offset: usize,
        _limit: usize,
    ) -> WorkflowResult<Vec<SubmissionInfo>> {
        Err(WorkflowError::Unsupported)
    }
    fn submission_status(&self, _id: i64) -> WorkflowResult<SubmissionInfo> {
        Err(WorkflowError::Unsupported)
    }
    fn sent_evidence(&self, _id: i64) -> WorkflowResult<SentEvidence> {
        Err(WorkflowError::Unsupported)
    }
    fn recover_submission(&self, _id: i64) -> WorkflowResult<SubmissionInfo> {
        Err(WorkflowError::Unsupported)
    }
    fn sync_draft(&self, _id: i64) -> WorkflowResult<SyncInfo> {
        Err(WorkflowError::Unsupported)
    }
    fn sync_sent(&self, _id: i64) -> WorkflowResult<SyncInfo> {
        Err(WorkflowError::Unsupported)
    }
    fn sync_status(&self, _id: i64) -> WorkflowResult<SyncInfo> {
        Err(WorkflowError::Unsupported)
    }
}
