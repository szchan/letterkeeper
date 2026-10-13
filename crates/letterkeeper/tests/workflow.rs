// SPDX-License-Identifier: GPL-3.0-only
use letterkeeper::app::{ACCOUNT, AppHost, SENDER};
use letterkeeper_adapters::workspace::{HumanWorkspace, WorkflowError};
use letterkeeper_core::{Attachment, DraftInput, Submission, SubmissionState};
use sha2::{Digest, Sha256};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn private_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn input(body: &str) -> DraftInput {
    DraftInput {
        account: ACCOUNT.into(),
        from: SENDER.into(),
        to: vec!["friend@example.test".into()],
        cc: vec![],
        bcc: vec![],
        subject: "完整审阅".into(),
        body: body.into(),
        attachments: vec![],
        in_reply_to: None,
        references: vec![],
    }
}

#[test]
fn trusted_workflow_scopes_and_approval_transition() {
    let dir = private_dir();
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
    assert_eq!(
        host.approve(id, &snapshot.draft.digest, "bad"),
        Err(WorkflowError::InvalidInput)
    );
    assert_eq!(host.send(id), Err(WorkflowError::Unavailable));
    let approval = host
        .approve(id, &snapshot.draft.digest, &snapshot.draft.digest)
        .unwrap();
    assert_eq!(approval.revision, 1);
    let now = letterkeeper::app::now().unwrap();
    assert!(approval.expires_at >= now + 299 && approval.expires_at <= now + 300);
    assert_eq!(host.load_draft(id).unwrap().digest, approval.digest);
    let mut changed = input("changed");
    changed.subject = "changed".into();
    host.save(id, 1, changed).unwrap();
    assert_eq!(
        host.approve(id, &snapshot.draft.digest, &snapshot.draft.digest),
        Err(WorkflowError::Conflict)
    );
    assert_eq!(host.list_submissions(0, 50).unwrap().len(), 0);
}

#[test]
fn read_only_and_attachment_path_denial_precede_io() {
    let dir = private_dir();
    let host = AppHost::open(dir.path(), vec![], true, None).unwrap();
    assert_eq!(host.create(input("x")), Err(WorkflowError::Denied));
    assert_eq!(host.save(1, 1, input("x")), Err(WorkflowError::Denied));
    assert_eq!(host.approve(1, "x", "x"), Err(WorkflowError::Denied));
    assert_eq!(host.send(1), Err(WorkflowError::Denied));
    assert_eq!(
        host.add_attachment(1, 1, Path::new("/definitely/missing"), "text/plain"),
        Err(WorkflowError::Denied)
    );
    assert_eq!(
        host.export_attachment("mail-1", 0, Path::new("/definitely/missing")),
        Err(WorkflowError::Denied)
    );
    assert_eq!(host.remove_attachment(1, 1, 0), Err(WorkflowError::Denied));
    assert_eq!(host.list_drafts(0, 50).unwrap().len(), 0);
}

#[test]
fn read_only_can_review_existing_draft_but_cannot_approve_it() {
    let dir = private_dir();
    let writer = AppHost::open(dir.path(), vec![], false, None).unwrap();
    let draft = writer.create(input("full body")).unwrap();
    let reader = AppHost::open(dir.path(), vec![], true, None).unwrap();
    assert_eq!(
        reader.review(draft.id).unwrap().draft.input.body,
        "full body"
    );
    assert_eq!(
        reader.approve(draft.id, &draft.digest, &draft.digest),
        Err(WorkflowError::Denied)
    );
}

#[test]
fn configured_transport_still_requires_an_unexpired_approval() {
    let dir = private_dir();
    let endpoint = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1);
    let host = AppHost::open(dir.path(), vec![], false, Some(endpoint)).unwrap();
    let draft = host.create(input("body")).unwrap();
    assert_eq!(host.send(draft.id), Err(WorkflowError::ApprovalRequired));
    assert!(host.list_submissions(0, 50).unwrap().is_empty());
}

#[test]
fn offline_unavailable_operations_are_explicit() {
    let dir = private_dir();
    let host = AppHost::open(dir.path(), vec![], false, None).unwrap();
    let d = host.create(input("x")).unwrap();
    assert_eq!(
        host.reply(
            "mail-1",
            letterkeeper_adapters::workspace::ReplyMode::Reply,
            "x",
            &[]
        ),
        Err(WorkflowError::Unsupported)
    );
    assert_eq!(host.sync_draft(d.id), Err(WorkflowError::Unsupported));
    assert_eq!(host.submission_status(1), Err(WorkflowError::Unavailable));
    assert_eq!(host.sent_evidence(1), Err(WorkflowError::Unsupported));
}

#[test]
fn attachment_bytes_are_reviewed_and_mutation_invalidates_revision() {
    let dir = private_dir();
    let host = AppHost::open(dir.path(), vec![], false, None).unwrap();
    let d = host.create(input("body")).unwrap();
    let path = dir.path().join("sample.txt");
    std::fs::write(&path, b"exact bytes\0").unwrap();
    let added = host
        .add_attachment(d.id, d.revision, &path, "text/plain")
        .unwrap();
    let review = host.review(d.id).unwrap();
    assert_eq!(
        review.attachment_digests,
        vec![format!("{:x}", Sha256::digest(b"exact bytes\0"))]
    );
    assert_eq!(
        host.remove_attachment(d.id, d.revision, 0),
        Err(WorkflowError::Conflict)
    );
    assert_eq!(
        host.approve(d.id, &d.digest, &d.digest),
        Err(WorkflowError::Conflict)
    );
    let removed = host.remove_attachment(d.id, added.revision, 0).unwrap();
    assert!(removed.input.attachments.is_empty());
}

#[test]
fn identity_and_attachment_failures_leave_revision_unchanged() {
    let dir = private_dir();
    let host = AppHost::open(dir.path(), vec![], false, None).unwrap();
    let mut foreign = input("x");
    foreign.account = "other".into();
    assert_eq!(host.create(foreign.clone()), Err(WorkflowError::Denied));
    let d = host.create(input("x")).unwrap();
    assert_eq!(
        host.save(d.id, d.revision, foreign),
        Err(WorkflowError::Denied)
    );
    assert_eq!(
        host.remove_attachment(d.id, d.revision, 0),
        Err(WorkflowError::InvalidInput)
    );
    assert_eq!(
        host.add_attachment(d.id, d.revision, Path::new("/missing/path"), "text/plain"),
        Err(WorkflowError::InvalidInput)
    );
    let real = dir.path().join("real.txt");
    std::fs::write(&real, b"bytes").unwrap();
    let link = dir.path().join("link.txt");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    assert_eq!(
        host.add_attachment(d.id, d.revision, &link, "text/plain"),
        Err(WorkflowError::InvalidInput)
    );
    let oversized = dir.path().join("oversized.bin");
    std::fs::write(&oversized, vec![0; 512 * 1024 + 1]).unwrap();
    assert_eq!(
        host.add_attachment(d.id, d.revision, &oversized, "application/octet-stream"),
        Err(WorkflowError::InvalidInput)
    );
    assert_eq!(host.load_draft(d.id).unwrap().revision, d.revision);
}

#[test]
fn approval_rejects_oversized_mime_before_grant() {
    let dir = private_dir();
    let host = AppHost::open(dir.path(), vec![], false, None).unwrap();
    let mut data = input(&"x".repeat(1024 * 1024));
    data.attachments = (0..2)
        .map(|i| Attachment {
            name: format!("{i}.bin"),
            content_type: "application/octet-stream".into(),
            bytes: vec![0u8; 512 * 1024],
        })
        .collect();
    let d = host.create(data).unwrap();
    assert_eq!(host.review(d.id).unwrap().draft.input.attachments.len(), 2);
    assert_eq!(
        host.approve(d.id, &d.digest, &d.digest),
        Err(WorkflowError::InvalidInput)
    );
}

#[test]
fn approval_uses_actual_message_id_length_at_wire_boundary() {
    let dir = private_dir();
    let host = AppHost::open(dir.path(), vec![], false, None).unwrap();
    let mut data = input("");
    data.attachments = (0..2)
        .map(|i| Attachment {
            name: format!("{i}.bin"),
            content_type: "application/octet-stream".into(),
            bytes: vec![0u8; 512 * 1024],
        })
        .collect();
    let mut probe = Submission {
        id: 0,
        draft_id: 0,
        revision: 1,
        digest: String::new(),
        message_id: "<lk-00000000000000000000000000000000@letterkeeper.invalid>".into(),
        input: data.clone(),
        state: SubmissionState::Submitting,
    };
    let wire_len = |probe: &mut Submission, n: usize| {
        probe.input.body = "x".repeat(n);
        letterkeeper::transport::prepare(probe)
            .unwrap()
            .formatted()
            .len()
    };
    let limit = 2 * 1024 * 1024;
    assert!(wire_len(&mut probe, 0) < limit);
    assert!(wire_len(&mut probe, 1024 * 1024) > limit);
    let (mut lo, mut hi) = (0, 1024 * 1024);
    while lo + 1 < hi {
        let mid = (lo + hi) / 2;
        if wire_len(&mut probe, mid) > limit {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    let actual = wire_len(&mut probe, hi);
    probe.message_id = "<review@letterkeeper.invalid>".into();
    let old_probe = wire_len(&mut probe, hi);
    assert!(
        actual > limit && old_probe <= limit,
        "actual={actual} old={old_probe}"
    );
    data.body = "x".repeat(hi);
    let d = host.create(data).unwrap();
    assert_eq!(
        host.approve(d.id, &d.digest, &d.digest),
        Err(WorkflowError::InvalidInput)
    );
}
