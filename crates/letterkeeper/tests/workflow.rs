// SPDX-License-Identifier: GPL-3.0-only
use letterkeeper::app::{ACCOUNT, AppHost, SENDER};
use letterkeeper_adapters::workspace::{HumanWorkspace, WorkflowError};
use letterkeeper_core::{DraftInput, SubmissionState};
use std::path::Path;

fn input(body: &str) -> DraftInput {
    DraftInput {
        account: ACCOUNT.into(), from: SENDER.into(), to: vec!["friend@example.test".into()],
        cc: vec![], bcc: vec![], subject: "完整审阅".into(), body: body.into(),
        attachments: vec![], in_reply_to: None, references: vec![],
    }
}

#[test]
fn trusted_workflow_scopes_and_approval_transition() {
    let dir = tempfile::tempdir().unwrap();
    let host = AppHost::open(dir.path(), vec![], false, None).unwrap();
    assert_eq!(host.capabilities().account, ACCOUNT);
    assert_eq!(host.create(input("hello")).unwrap().revision, 1);
    let d = host.list_drafts(0, 50).unwrap();
    assert_eq!(d.len(), 1);
    assert_eq!(host.list_drafts(0, 51), Err(WorkflowError::InvalidInput));
    let id = d[0].id;
    let snapshot = host.review(id).unwrap();
    assert_eq!(snapshot.draft.input.body, "hello");
    assert!(snapshot.attachment_digests.is_empty());
    assert_eq!(host.approve(id, &snapshot.draft.digest, "bad"), Err(WorkflowError::InvalidInput));
    assert_eq!(host.send(id), Err(WorkflowError::Unavailable));
    let approval = host.approve(id, &snapshot.draft.digest, &snapshot.draft.digest).unwrap();
    assert_eq!(approval.revision, 1);
    assert_eq!(host.load_draft(id).unwrap().digest, approval.digest);
    let mut changed = input("changed");
    changed.subject = "changed".into();
    host.save(id, 1, changed).unwrap();
    assert_eq!(host.approve(id, &snapshot.draft.digest, &snapshot.draft.digest), Err(WorkflowError::Conflict));
    assert_eq!(host.list_submissions(0, 50).unwrap().len(), 0);
}

#[test]
fn read_only_and_attachment_path_denial_precede_io() {
    let dir = tempfile::tempdir().unwrap();
    let host = AppHost::open(dir.path(), vec![], true, None).unwrap();
    assert_eq!(host.create(input("x")), Err(WorkflowError::Denied));
    assert_eq!(host.add_attachment(1, 1, Path::new("/definitely/missing"), "text/plain"), Err(WorkflowError::Denied));
    assert_eq!(host.export_attachment("mail-1", 0, Path::new("/definitely/missing")), Err(WorkflowError::Denied));
    assert_eq!(host.list_drafts(0, 50).unwrap().len(), 0);
}

#[test]
fn offline_unavailable_operations_are_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let host = AppHost::open(dir.path(), vec![], false, None).unwrap();
    let d = host.create(input("x")).unwrap();
    assert_eq!(host.reply("mail-1", letterkeeper_adapters::workspace::ReplyMode::Reply, "x", &[]), Err(WorkflowError::Unsupported));
    assert_eq!(host.sync_draft(d.id), Err(WorkflowError::Unsupported));
    assert_eq!(host.submission_status(1), Err(WorkflowError::Unavailable));
    assert_eq!(host.sent_evidence(1), Err(WorkflowError::Unsupported));
    let _ = SubmissionState::Accepted;
}
