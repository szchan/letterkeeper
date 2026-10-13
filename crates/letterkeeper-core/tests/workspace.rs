// SPDX-License-Identifier: GPL-3.0-only
use letterkeeper_core::{
    AccessPolicy, Attachment, DraftInput, Error, Permissions, SendOutcome, Store,
};
use std::os::unix::fs::PermissionsExt;

fn input(account: &str) -> DraftInput {
    DraftInput {
        account: account.into(),
        from: "sender@example.test".into(),
        to: vec!["recipient@example.test".into()],
        cc: vec![],
        bcc: vec![],
        subject: account.into(),
        body: "x".into(),
        attachments: vec![],
        in_reply_to: None,
        references: vec![],
    }
}

#[test]
fn account_scoped_pages_cover_drafts_and_submissions() {
    let policy = || {
        AccessPolicy::new(
            vec!["a".into(), "b".into()],
            Permissions {
                read: true,
                draft: true,
                send: true,
            },
        )
    };
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = dir.path().join("state.sqlite");
    let mut store = Store::open(&path).unwrap().with_policy(policy());
    let a = store.create_draft(input("a")).unwrap();
    let b = store.create_draft(input("b")).unwrap();
    store.approve(a.id, &a.digest, 100).unwrap();
    let sub = store.claim_send(a.id, 1).unwrap();
    store
        .finish_submission(sub.id, SendOutcome::Accepted)
        .unwrap();
    let only_a = Store::open(&path).unwrap().with_policy(AccessPolicy::new(
        vec!["a".into()],
        Permissions {
            read: true,
            draft: false,
            send: false,
        },
    ));
    assert_eq!(
        only_a
            .list_drafts(0, 50)
            .unwrap()
            .iter()
            .map(|d| d.id)
            .collect::<Vec<_>>(),
        vec![a.id]
    );
    assert_eq!(
        only_a
            .list_submissions(0, 50)
            .unwrap()
            .iter()
            .map(|s| s.id)
            .collect::<Vec<_>>(),
        vec![sub.id]
    );
    let only_b = Store::open(&path).unwrap().with_policy(AccessPolicy::new(
        vec!["b".into()],
        Permissions {
            read: true,
            draft: false,
            send: false,
        },
    ));
    assert_eq!(
        only_b
            .list_drafts(0, 50)
            .unwrap()
            .iter()
            .map(|d| d.id)
            .collect::<Vec<_>>(),
        vec![b.id]
    );
    assert!(only_b.list_submissions(0, 50).unwrap().is_empty());
    assert_eq!(store.list_drafts(0, 1).unwrap()[0].id, b.id);
    assert_eq!(store.list_drafts(1, 1).unwrap()[0].id, a.id);
    assert_eq!(store.list_submissions(0, 50).unwrap()[0].id, sub.id);
    assert!(store.list_drafts(0, 51).is_err());
    assert_eq!(store.list_drafts(0, 0), Err(Error::InvalidInput));
    assert_eq!(store.list_submissions(100_001, 1), Err(Error::InvalidInput));
    let no_read = Store::open(&path).unwrap().with_policy(AccessPolicy::new(
        vec!["a".into()],
        Permissions {
            read: false,
            draft: false,
            send: false,
        },
    ));
    assert_eq!(no_read.list_drafts(0, 50), Err(Error::Denied));
    assert_eq!(no_read.list_submissions(0, 50), Err(Error::Denied));
}

#[test]
fn list_pages_are_bounded_and_do_not_return_attachment_bytes() {
    let mut store = Store::memory().unwrap().with_policy(AccessPolicy::new(
        vec!["a".into()],
        Permissions {
            read: true,
            draft: true,
            send: true,
        },
    ));
    let mut large = input("a");
    large.attachments.push(Attachment {
        name: "large.bin".into(),
        content_type: "application/octet-stream".into(),
        bytes: vec![7; 512 * 1024],
    });
    let first = store.create_draft(large).unwrap();
    for _ in 0..55 {
        store.create_draft(input("a")).unwrap();
    }
    let page = store.list_drafts(0, 50).unwrap();
    assert_eq!(page.len(), 50);
    assert_eq!(store.list_drafts(50, 50).unwrap().len(), 6);
    let entry = store.list_drafts(55, 1).unwrap().pop().unwrap();
    assert_eq!(entry.id, first.id);
    assert_eq!(entry.subject, "a");
    assert_eq!(entry.digest, first.digest);
}

#[test]
fn two_stores_conflict_on_stale_digest_and_one_claim_consumes_approval() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = dir.path().join("state.sqlite");
    let policy = || {
        AccessPolicy::new(
            vec!["a".into()],
            Permissions {
                read: true,
                draft: true,
                send: true,
            },
        )
    };
    let mut first = Store::open(&path).unwrap().with_policy(policy());
    let mut second = Store::open(&path).unwrap().with_policy(policy());
    let initial = first.create_draft(input("a")).unwrap();
    let mut changed = initial.input.clone();
    changed.body = "new".into();
    second
        .update_draft(initial.id, initial.revision, changed)
        .unwrap();
    assert_eq!(
        first.approve(initial.id, &initial.digest, 100),
        Err(Error::Conflict)
    );
    let current = first.get_draft(initial.id).unwrap();
    first.approve(current.id, &current.digest, 100).unwrap();
    assert!(second.claim_send(current.id, 1).is_ok());
    assert_eq!(first.claim_send(current.id, 1), Err(Error::InvalidState));
}
