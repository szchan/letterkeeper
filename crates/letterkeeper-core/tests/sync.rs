// SPDX-License-Identifier: GPL-3.0-only
use letterkeeper_core::{
    AccessPolicy, DraftInput, Error, Permissions, SendOutcome, Store, SyncState,
};

fn input() -> DraftInput {
    serde_json::from_str(r#"{"account":"controlled","from":"sender@example.test","to":["reader@example.test"],"cc":[],"bcc":[],"subject":"Re: 中文","body":"合成回复","attachments":[]}"#).unwrap()
}
fn store() -> Store {
    Store::memory().unwrap().with_policy(policy(true))
}
fn policy(write: bool) -> AccessPolicy {
    AccessPolicy::new(
        vec!["controlled".into()],
        Permissions {
            read: true,
            draft: write,
            send: write,
        },
    )
}

#[test]
fn thread_headers_reject_injection_and_excessive_references() {
    let mut s = store();
    let mut d = input();
    d.in_reply_to = Some("<original@example.test>\r\nBcc: stolen@example.test".into());
    assert_eq!(s.create_draft(d).unwrap_err(), Error::InvalidInput);
    let mut d = input();
    d.references = vec!["<original@example.test>".into(); 33];
    assert_eq!(s.create_draft(d).unwrap_err(), Error::InvalidInput);
}

#[test]
fn changing_thread_headers_invalidates_approval() {
    let mut s = store();
    let d = s.create_draft(input()).unwrap();
    s.approve(d.id, &d.digest, 100).unwrap();
    let mut next = d.input.clone();
    next.in_reply_to = Some("<original@example.test>".into());
    next.references = vec!["<root@example.test>".into()];
    let next = s.update_draft(d.id, d.revision, next).unwrap();
    assert_ne!(next.digest, d.digest);
    assert_eq!(s.claim_send(d.id, 1).unwrap_err(), Error::ApprovalRequired);
}

#[test]
fn empty_thread_fields_preserve_legacy_snapshot_serialization() {
    let legacy = input();
    let v = serde_json::to_value(&legacy).unwrap();
    assert!(v.get("in_reply_to").is_none());
    assert!(v.get("references").is_none());
    let mut s = store();
    let d = s.create_draft(legacy).unwrap();
    let encoded = serde_json::to_vec(&("letterkeeper-draft-v1", 1i64, &d.input)).unwrap();
    use sha2::{Digest, Sha256};
    assert_eq!(d.digest, format!("{:x}", Sha256::digest(encoded)));
}

#[test]
fn draft_sync_is_single_claim_per_revision_and_content_bound() {
    let mut s = store();
    let d = s.create_draft(input()).unwrap();
    let first = s.claim_draft_sync(d.id, "Drafts").unwrap();
    assert_eq!(first.state, SyncState::Pending);
    assert_eq!(
        s.claim_draft_sync(d.id, "Drafts").unwrap_err(),
        Error::InvalidState
    );
    s.finish_sync(first.id, SendOutcome::Accepted).unwrap();
    assert_eq!(
        s.claim_draft_sync(d.id, "OtherDrafts").unwrap_err(),
        Error::InvalidState
    );
    let mut next = d.input.clone();
    next.body = "edited".into();
    let d = s.update_draft(d.id, 1, next).unwrap();
    let second = s.claim_draft_sync(d.id, "Drafts").unwrap();
    assert_eq!(second.revision, 2);
    assert_ne!(first.message_id, second.message_id);
    assert_eq!(s.get_sync(first.id).unwrap().state, SyncState::Confirmed);
}

#[test]
fn uncertain_append_cannot_be_retried_after_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("private/state.sqlite");
    let mut s = Store::open(&db).unwrap().with_policy(policy(true));
    let d = s.create_draft(input()).unwrap();
    let record = s.claim_draft_sync(d.id, "Drafts").unwrap();
    s.finish_sync(record.id, SendOutcome::Uncertain).unwrap();
    drop(s);
    let mut s = Store::open(&db).unwrap().with_policy(policy(true));
    assert_eq!(s.get_sync(record.id).unwrap().state, SyncState::Uncertain);
    assert_eq!(
        s.claim_draft_sync(d.id, "Drafts").unwrap_err(),
        Error::InvalidState
    );
    assert_eq!(
        s.finish_sync(record.id, SendOutcome::Accepted).unwrap_err(),
        Error::InvalidState
    );
}

#[test]
fn pending_claim_and_scope_survive_reopen_without_auto_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("private/state.sqlite");
    let mut s = Store::open(&db).unwrap().with_policy(policy(true));
    let d = s.create_draft(input()).unwrap();
    let record = s.claim_draft_sync(d.id, "Drafts").unwrap();
    drop(s);
    let mut readonly = Store::open(&db).unwrap().with_policy(policy(false));
    assert_eq!(
        readonly.get_sync(record.id).unwrap().state,
        SyncState::Pending
    );
    assert_eq!(
        readonly
            .finish_sync(record.id, SendOutcome::Accepted)
            .unwrap_err(),
        Error::Denied
    );
    assert_eq!(
        readonly.claim_draft_sync(d.id, "Drafts").unwrap_err(),
        Error::Denied
    );
    let denied = Store::open(&db).unwrap();
    assert_eq!(denied.get_sync(record.id).unwrap_err(), Error::Denied);
}

#[test]
fn sent_sync_requires_smtp_acceptance_and_uses_same_message_id() {
    let mut s = store();
    let d = s.create_draft(input()).unwrap();
    s.approve(d.id, &d.digest, 100).unwrap();
    let sub = s.claim_send(d.id, 1).unwrap();
    assert_eq!(
        s.claim_sent_sync(sub.id, "Sent").unwrap_err(),
        Error::InvalidState
    );
    s.finish_submission(sub.id, SendOutcome::Accepted).unwrap();
    let record = s.claim_sent_sync(sub.id, "Sent").unwrap();
    assert_eq!(record.message_id, sub.message_id);
    s.finish_sync(record.id, SendOutcome::Accepted).unwrap();
    assert_eq!(
        s.claim_sent_sync(sub.id, "Sent").unwrap_err(),
        Error::InvalidState
    );
}

#[test]
fn sync_folder_rejects_commands_and_concurrent_claim_has_one_winner() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("private/state.sqlite");
    let mut s = Store::open(&db).unwrap().with_policy(policy(true));
    let d = s.create_draft(input()).unwrap();
    assert_eq!(
        s.claim_draft_sync(d.id, "Drafts\r\nLOGOUT").unwrap_err(),
        Error::InvalidInput
    );
    drop(s);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let workers = (0..2)
        .map(|_| {
            let db = db.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut s = Store::open(db).unwrap().with_policy(policy(true));
                barrier.wait();
                s.claim_draft_sync(d.id, "Drafts")
            })
        })
        .collect::<Vec<_>>();
    let results = workers
        .into_iter()
        .map(|w| w.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Err(Error::InvalidState)))
            .count(),
        1
    );
}

#[test]
fn immutable_wire_is_durable_and_scoped_for_exact_sent_copy() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("private/state.sqlite");
    let mut s = Store::open(&db).unwrap().with_policy(policy(true));
    let d = s.create_draft(input()).unwrap();
    s.approve(d.id, &d.digest, 100).unwrap();
    let sub = s.claim_send(d.id, 1).unwrap();
    let wire = b"From: sender@example.test\r\n\r\nexact MIME boundary snapshot";
    s.record_submission_wire(sub.id, wire).unwrap();
    assert_eq!(
        s.record_submission_wire(sub.id, b"changed").unwrap_err(),
        Error::InvalidState
    );
    s.finish_submission(sub.id, SendOutcome::Accepted).unwrap();
    drop(s);
    let s = Store::open(&db).unwrap().with_policy(policy(false));
    assert_eq!(s.submission_wire(sub.id).unwrap(), wire);
    let denied = Store::open(&db).unwrap();
    assert_eq!(denied.submission_wire(sub.id).unwrap_err(), Error::Denied);
}

#[test]
fn legacy_schema_migrates_atomically_and_preserves_existing_approval() {
    use sha2::{Digest, Sha256};
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("private/state.sqlite");
    let s = Store::open(&db).unwrap();
    drop(s);
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch(
        "DROP TABLE sync_records; DROP TABLE submission_wire; PRAGMA user_version=1;",
    )
    .unwrap();
    let content = serde_json::to_string(&input()).unwrap();
    let digest = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&("letterkeeper-draft-v1", 1i64, &input())).unwrap())
    );
    conn.execute(
        "INSERT INTO drafts(id,revision,digest,content) VALUES(1,1,?1,?2)",
        rusqlite::params![digest, content],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO approvals(draft_id,revision,digest,expires_at) VALUES(1,1,?1,100)",
        [&digest],
    )
    .unwrap();
    drop(conn);
    let mut s = Store::open(&db).unwrap().with_policy(policy(true));
    assert_eq!(s.get_draft(1).unwrap().digest, digest);
    assert_eq!(s.claim_send(1, 1).unwrap().digest, digest);
    drop(s);
    let conn = rusqlite::Connection::open(db).unwrap();
    assert_eq!(
        conn.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        2
    );
}

#[test]
fn sync_keeps_claimed_snapshot_if_local_draft_is_edited() {
    let mut s = store();
    let d = s.create_draft(input()).unwrap();
    let record = s.claim_draft_sync(d.id, "Drafts").unwrap();
    let mut next = d.input.clone();
    next.body = "different revision".into();
    s.update_draft(d.id, 1, next).unwrap();
    assert_eq!(s.get_sync(record.id).unwrap().input, d.input);
    assert_eq!(s.get_sync(record.id).unwrap().digest, d.digest);
}

#[test]
fn message_ids_are_opaque_identifiers_not_limited_email_addresses() {
    let mut s = store();
    for msg_id in [
        format!("<{}@example.test>", "x".repeat(80)),
        "<synthetic@[127.0.0.1]>".into(),
        "<synthetic@opaque_id>".into(),
        "<synthetic@[opaque@literal]>".into(),
    ] {
        let mut d = input();
        d.in_reply_to = Some(msg_id.clone());
        d.references = vec![msg_id];
        assert!(s.create_draft(d).is_ok());
    }
}

#[test]
fn malformed_thread_ids_fail_closed() {
    let mut s = store();
    for msg_id in [
        "<x@@test>",
        "<.x@test>",
        "<x..y@test>",
        "<x@a..b>",
        "<x@[a\\b]>",
        "<x@[]]>",
        "<x@a>\n",
        "<x@a> <y@b>",
    ] {
        let mut d = input();
        d.references = vec![msg_id.into()];
        assert_eq!(s.create_draft(d).unwrap_err(), Error::InvalidInput);
    }
}
