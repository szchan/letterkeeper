// SPDX-License-Identifier: GPL-3.0-only
use letterkeeper_core::*;
fn policy() -> AccessPolicy {
    AccessPolicy::new(
        vec!["demo".into()],
        Permissions {
            read: true,
            draft: true,
            send: true,
        },
    )
}
fn store() -> Store {
    Store::memory().unwrap().with_policy(policy())
}
fn input() -> DraftInput {
    DraftInput {
        account: "demo".into(),
        from: "sender@example.invalid".into(),
        to: vec!["reader@example.invalid".into()],
        cc: vec![],
        bcc: vec![],
        subject: "合成邮件".into(),
        body: "纯文本\n正文".into(),
        attachments: vec![],
    }
}
#[test]
fn creation_revision_and_cas() {
    let mut s = store();
    let d = s.create_draft(input()).unwrap();
    assert_eq!(d.revision, 1);
    assert_eq!(s.get_draft(d.id).unwrap(), d);
    let mut next = input();
    next.subject = "新主题".into();
    let updated = s.update_draft(d.id, 1, next).unwrap();
    assert_eq!(updated.revision, 2);
    assert_ne!(updated.digest, d.digest);
    assert!(s.update_draft(d.id, 1, input()).is_err());
}
#[test]
fn approval_is_single_use_and_content_bound() {
    let mut s = store();
    let d = s.create_draft(input()).unwrap();
    assert!(s.claim_send(d.id, 10).is_err());
    assert!(s.approve(d.id, "wrong", 50).is_err());
    s.approve(d.id, &d.digest, 50).unwrap();
    let sent = s.claim_send(d.id, 10).unwrap();
    assert_eq!(sent.input, d.input);
    assert_eq!(sent.state, SubmissionState::Submitting);
    assert!(s.claim_send(d.id, 11).is_err());
    assert!(s.approve(d.id, &d.digest, 100).is_err());
    assert!(s.update_draft(d.id, d.revision, input()).is_err());
    s.finish_submission(sent.id, SendOutcome::Accepted).unwrap();
    assert!(s.claim_send(d.id, 12).is_err());
}
#[test]
fn bcc_and_attachment_change_invalidate_approval() {
    let mut s = store();
    let d = s.create_draft(input()).unwrap();
    s.approve(d.id, &d.digest, 50).unwrap();
    let mut next = input();
    next.bcc.push("hidden@example.invalid".into());
    let d2 = s.update_draft(d.id, 1, next.clone()).unwrap();
    assert_ne!(d.digest, d2.digest);
    assert!(s.claim_send(d.id, 10).is_err());
    s.approve(d.id, &d2.digest, 50).unwrap();
    next.attachments.push(Attachment {
        name: "note.txt".into(),
        content_type: "text/plain".into(),
        bytes: b"one".to_vec(),
    });
    let d3 = s.update_draft(d.id, 2, next.clone()).unwrap();
    assert_ne!(d2.digest, d3.digest);
    s.approve(d.id, &d3.digest, 50).unwrap();
    next.attachments[0].bytes = b"two".to_vec();
    let d4 = s.update_draft(d.id, 3, next).unwrap();
    assert_ne!(d3.digest, d4.digest);
    assert!(s.claim_send(d.id, 10).is_err());
}
#[test]
fn expiry_is_exclusive() {
    let mut s = store();
    let d = s.create_draft(input()).unwrap();
    s.approve(d.id, &d.digest, 10).unwrap();
    assert!(s.claim_send(d.id, 10).is_err());
    s.approve(d.id, &d.digest, 11).unwrap();
    assert!(s.claim_send(d.id, 10).is_ok());
}
#[test]
fn uncertain_blocks_retry_and_explicit_recovery_only() {
    let mut s = store();
    let d = s.create_draft(input()).unwrap();
    s.approve(d.id, &d.digest, 50).unwrap();
    let sent = s.claim_send(d.id, 10).unwrap();
    let uncertain = s.recover_interrupted(sent.id).unwrap();
    assert_eq!(uncertain.state, SubmissionState::Uncertain);
    assert!(s.claim_send(d.id, 11).is_err());
    assert!(s.approve(d.id, &d.digest, 100).is_err());
    assert!(s.finish_submission(sent.id, SendOutcome::Failed).is_err());
}
#[test]
fn definitive_failure_requires_fresh_approval() {
    let mut s = store();
    let d = s.create_draft(input()).unwrap();
    s.approve(d.id, &d.digest, 50).unwrap();
    let first = s.claim_send(d.id, 10).unwrap();
    s.finish_submission(first.id, SendOutcome::Failed).unwrap();
    assert!(s.claim_send(d.id, 11).is_err());
    s.approve(d.id, &d.digest, 50).unwrap();
    let second = s.claim_send(d.id, 11).unwrap();
    assert_ne!(first.message_id, second.message_id);
}
#[test]
fn default_deny_account_and_operation_scope() {
    let mut s = Store::memory().unwrap();
    assert!(s.create_draft(input()).is_err());
    let mut s = store();
    let mut bad = input();
    bad.account = "other".into();
    assert!(s.create_draft(bad).is_err());
    let mut s = Store::memory().unwrap().with_policy(AccessPolicy::new(
        vec!["demo".into()],
        Permissions {
            read: true,
            draft: true,
            send: false,
        },
    ));
    let d = s.create_draft(input()).unwrap();
    assert!(s.approve(d.id, &d.digest, 50).is_err());
}
#[test]
fn rejects_header_injection_paths_controls_and_size() {
    let mut s = store();
    for field in 0..4 {
        let mut bad = input();
        match field {
            0 => bad.subject = "secret\r\nBcc: attacker@example.invalid".into(),
            1 => bad.from = "bad\naddress@example.invalid".into(),
            2 => bad.body = "escape\x1b[31m".into(),
            _ => bad.attachments.push(Attachment {
                name: "../secret".into(),
                content_type: "text/plain".into(),
                bytes: vec![],
            }),
        };
        let err = s.create_draft(bad).unwrap_err().to_string();
        assert!(!err.contains("secret"));
    }
    let mut huge = input();
    huge.body = "a".repeat(1024 * 1024 + 1);
    assert!(s.create_draft(huge).is_err());
}
#[test]
fn persistence_does_not_recover_other_workers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let mut s = Store::open(&path).unwrap().with_policy(policy());
    let d = s.create_draft(input()).unwrap();
    s.approve(d.id, &d.digest, 50).unwrap();
    let submission = s.claim_send(d.id, 10).unwrap();
    drop(s);
    let s = Store::open(&path).unwrap().with_policy(policy());
    assert_eq!(s.get_draft(d.id).unwrap(), d);
    assert_eq!(s.get_submission(submission.id).unwrap(), submission);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
#[test]
fn concurrent_connections_claim_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let mut s = Store::open(&path).unwrap().with_policy(policy());
    let d = s.create_draft(input()).unwrap();
    s.approve(d.id, &d.digest, 50).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut s = Store::open(path).unwrap().with_policy(policy());
                barrier.wait();
                s.claim_send(d.id, 10).is_ok()
            })
        })
        .collect();
    assert_eq!(
        handles
            .into_iter()
            .filter_map(|h| h.join().ok().filter(|x| *x))
            .count(),
        1
    );
}
