// SPDX-License-Identifier: GPL-3.0-only
//! Shared trusted-human workflow helpers; never exposed as MCP tools.
use letterkeeper_adapters::HostError;
use letterkeeper_adapters::workspace::{
    ApprovalInfo, DraftSummary, ReviewSnapshot, SubmissionInfo, SyncInfo, WorkflowError,
    WorkflowResult,
};
use letterkeeper_core::{
    Draft, DraftListEntry, Error, Submission, SubmissionListEntry, SubmissionState, SyncRecord,
};
use sha2::{Digest, Sha256};

pub fn core_error(error: Error) -> WorkflowError {
    match error {
        Error::Denied => WorkflowError::Denied,
        Error::Conflict | Error::InvalidState => WorkflowError::Conflict,
        Error::ApprovalRequired => WorkflowError::ApprovalRequired,
        Error::InvalidInput => WorkflowError::InvalidInput,
        Error::NotFound | Error::UnsafePath | Error::SchemaVersion | Error::Storage => {
            WorkflowError::Unavailable
        }
    }
}
pub fn host_error(error: HostError) -> WorkflowError {
    match error.0.as_str() {
        "account or operation is outside the configured scope"
        | "draft_identity_outside_account"
        | "read_scope_denied"
        | "draft_scope_denied"
        | "send_scope_denied"
        | "folder_scope_denied" => WorkflowError::Denied,
        "draft revision or approved content changed"
        | "submission state prevents this operation"
        | "smtp_acceptance_required" => WorkflowError::Conflict,
        "a current, unexpired human approval is required" => WorkflowError::ApprovalRequired,
        "input violates field or size limits"
        | "invalid_reply_mode"
        | "invalid_forward_recipients"
        | "invalid_id"
        | "invalid_message_id" => WorkflowError::InvalidInput,
        "sent_copy_is_server_managed_use_sent_evidence" => WorkflowError::Unsupported,
        _ => WorkflowError::Unavailable,
    }
}
pub fn draft_summary(d: DraftListEntry) -> DraftSummary {
    DraftSummary {
        id: d.id,
        revision: d.revision,
        subject: d.subject,
        digest: d.digest,
    }
}
pub fn submission_entry_info(s: SubmissionListEntry) -> SubmissionInfo {
    SubmissionInfo {
        id: s.id,
        draft_id: s.draft_id,
        state: s.state,
        state_recorded: true,
    }
}
pub fn submission_info(s: Submission) -> SubmissionInfo {
    SubmissionInfo {
        id: s.id,
        draft_id: s.draft_id,
        state: s.state,
        state_recorded: true,
    }
}
pub fn sync_info(s: SyncRecord) -> SyncInfo {
    SyncInfo {
        id: s.id,
        state: s.state,
    }
}
pub fn review_snapshot(draft: Draft) -> ReviewSnapshot {
    let attachment_digests = draft
        .input
        .attachments
        .iter()
        .map(|a| format!("{:x}", Sha256::digest(&a.bytes)))
        .collect();
    ReviewSnapshot {
        draft,
        attachment_digests,
    }
}
pub fn approve(
    store: &mut letterkeeper_core::Store,
    id: i64,
    expected_digest: &str,
    typed_digest: &str,
) -> WorkflowResult<ApprovalInfo> {
    let draft = store.get_draft(id).map_err(core_error)?;
    if draft.digest != expected_digest {
        return Err(WorkflowError::Conflict);
    }
    if typed_digest != expected_digest {
        return Err(WorkflowError::InvalidInput);
    }
    // Check the exact MIME serializer's bound before granting approval, so a
    // draft that cannot be sent does not consume a human confirmation.
    let probe = Submission {
        id: 0,
        draft_id: id,
        revision: draft.revision,
        digest: draft.digest.clone(),
        message_id: "<lk-00000000000000000000000000000000@letterkeeper.invalid>".into(),
        input: draft.input.clone(),
        state: SubmissionState::Submitting,
    };
    let wire = crate::transport::prepare(&probe)
        .map_err(|_| WorkflowError::InvalidInput)?
        .formatted();
    if wire.len() > 2 * 1024 * 1024 {
        return Err(WorkflowError::InvalidInput);
    }
    let now = crate::app::now().map_err(|_| WorkflowError::Unavailable)?;
    let expires_at = now.checked_add(300).ok_or(WorkflowError::Unavailable)?;
    store
        .approve(id, expected_digest, expires_at)
        .map_err(core_error)?;
    Ok(ApprovalInfo {
        draft_id: id,
        revision: draft.revision,
        digest: draft.digest,
        expires_at,
    })
}
