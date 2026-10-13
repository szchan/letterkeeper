// SPDX-License-Identifier: GPL-3.0-only
use letterkeeper_core::{AccessPolicy, DraftInput, Permissions, Store, SendOutcome};

fn input(account: &str) -> DraftInput {
    DraftInput { account: account.into(), from: "sender@example.test".into(),
        to: vec!["recipient@example.test".into()], cc: vec![], bcc: vec![],
        subject: account.into(), body: "x".into(), attachments: vec![],
        in_reply_to: None, references: vec![] }
}

#[test]
fn account_scoped_pages_cover_drafts_and_submissions() {
    let policy = || AccessPolicy::new(vec!["a".into(), "b".into()], Permissions { read:true, draft:true, send:true });
    let mut store = Store::memory().unwrap().with_policy(policy());
    let a = store.create_draft(input("a")).unwrap();
    let b = store.create_draft(input("b")).unwrap();
    store.approve(a.id, &a.digest, 100).unwrap();
    let sub = store.claim_send(a.id, 1).unwrap();
    store.finish_submission(sub.id, SendOutcome::Accepted).unwrap();
    let only_a = Store::memory().unwrap().with_policy(AccessPolicy::new(vec!["a".into()], Permissions { read:true, draft:false, send:false }));
    assert!(only_a.list_drafts(0, 50).unwrap().is_empty());
    assert_eq!(store.list_drafts(0, 1).unwrap()[0].id, a.id);
    assert_eq!(store.list_drafts(1, 1).unwrap()[0].id, b.id);
    assert_eq!(store.list_submissions(0, 50).unwrap()[0].id, sub.id);
    assert!(store.list_drafts(0, 51).is_err());
}
