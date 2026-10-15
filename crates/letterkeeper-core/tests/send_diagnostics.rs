// SPDX-License-Identifier: GPL-3.0-only
use letterkeeper_core::*;
use rusqlite::Connection;
use std::{os::unix::fs::PermissionsExt, path::Path};

fn policy(account: &str, read: bool, send: bool) -> AccessPolicy {
    AccessPolicy::new(
        vec![account.into()],
        Permissions {
            read,
            draft: true,
            send,
        },
    )
}
fn open(path: &Path) -> Store {
    Store::open(path)
        .unwrap()
        .with_policy(policy("demo", true, true))
}
fn input() -> DraftInput {
    DraftInput {
        account: "demo".into(),
        from: "sender@example.invalid".into(),
        to: vec!["recipient@example.invalid".into()],
        cc: vec![],
        bcc: vec![],
        subject: "synthetic".into(),
        body: "synthetic body".into(),
        attachments: vec![],
        in_reply_to: None,
        references: vec![],
    }
}
fn claim(store: &mut Store) -> Submission {
    let draft = store.create_draft(input()).unwrap();
    store.approve(draft.id, &draft.digest, 100).unwrap();
    store.claim_send(draft.id, 1).unwrap()
}
fn diagnostic(code: Option<u16>) -> SendDiagnostic {
    SendDiagnostic {
        stage: SendStage::Submission,
        kind: SendFailureKind::Timeout,
        smtp_code: code,
    }
}
fn database() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = dir.path().join("synthetic.sqlite");
    (dir, path)
}

#[test]
fn failed_and_uncertain_diagnostics_survive_reopen_without_retry() {
    for (outcome, state) in [
        (SendOutcome::Failed, SubmissionState::Failed),
        (SendOutcome::Uncertain, SubmissionState::Uncertain),
    ] {
        let (_dir, path) = database();
        let mut store = open(&path);
        let sub = claim(&mut store);
        let detail = diagnostic(Some(451));
        let result = store
            .finish_submission_with_diagnostic(sub.id, outcome, Some(detail))
            .unwrap();
        assert_eq!(result.state, state);
        assert_eq!(result.digest, sub.digest);
        drop(store);
        let mut store = open(&path);
        assert_eq!(
            store.get_submission_diagnostic(sub.id).unwrap(),
            Some(detail)
        );
        assert_eq!(store.get_submission(sub.id).unwrap(), result);
        assert!(store.claim_send(sub.draft_id, 2).is_err());
        assert_eq!(
            store.finish_submission_with_diagnostic(
                sub.id,
                SendOutcome::Failed,
                Some(diagnostic(None))
            ),
            Err(Error::InvalidState)
        );
        assert_eq!(
            store.get_submission_diagnostic(sub.id).unwrap(),
            Some(detail)
        );
        assert_eq!(store.list_submissions(0, 50).unwrap().len(), 1);
    }
}

#[test]
fn accepted_and_legacy_finish_do_not_fabricate_diagnostics() {
    let mut store = Store::memory()
        .unwrap()
        .with_policy(policy("demo", true, true));
    let sub = claim(&mut store);
    assert_eq!(
        store.finish_submission_with_diagnostic(
            sub.id,
            SendOutcome::Accepted,
            Some(diagnostic(None))
        ),
        Err(Error::InvalidInput)
    );
    assert_eq!(
        store.get_submission(sub.id).unwrap().state,
        SubmissionState::Submitting
    );
    assert_eq!(store.get_submission_diagnostic(sub.id).unwrap(), None);
    store
        .finish_submission_with_diagnostic(sub.id, SendOutcome::Accepted, None)
        .unwrap();
    assert_eq!(store.get_submission_diagnostic(sub.id).unwrap(), None);
    for outcome in [
        SendOutcome::Failed,
        SendOutcome::Uncertain,
        SendOutcome::Accepted,
    ] {
        let sub = claim(&mut store);
        store.finish_submission(sub.id, outcome).unwrap();
        assert_eq!(store.get_submission_diagnostic(sub.id).unwrap(), None);
    }
}

#[test]
fn diagnostic_code_limits_and_closed_serialization() {
    let mut store = Store::memory()
        .unwrap()
        .with_policy(policy("demo", true, true));
    for code in [0, 99, 600, u16::MAX] {
        let sub = claim(&mut store);
        assert_eq!(
            store.finish_submission_with_diagnostic(
                sub.id,
                SendOutcome::Failed,
                Some(diagnostic(Some(code)))
            ),
            Err(Error::InvalidInput)
        );
        assert_eq!(
            store.get_submission(sub.id).unwrap().state,
            SubmissionState::Submitting
        );
        assert_eq!(store.get_submission_diagnostic(sub.id).unwrap(), None);
        let json = format!(r#"{{"stage":"submission","kind":"timeout","smtp_code":{code}}}"#);
        assert!(serde_json::from_str::<SendDiagnostic>(&json).is_err());
    }
    for code in [None, Some(100), Some(599)] {
        let sub = claim(&mut store);
        let detail = diagnostic(code);
        store
            .finish_submission_with_diagnostic(sub.id, SendOutcome::Failed, Some(detail))
            .unwrap();
        assert_eq!(
            store.get_submission_diagnostic(sub.id).unwrap(),
            Some(detail)
        );
        assert_eq!(
            serde_json::from_str::<SendDiagnostic>(&serde_json::to_string(&detail).unwrap())
                .unwrap(),
            detail
        );
    }
    for json in [
        r#"{"stage":"unknown","kind":"timeout","smtp_code":null}"#,
        r#"{"stage":"submission","kind":"server text","smtp_code":null}"#,
        r#"{"stage":"submission","kind":"timeout","smtp_code":null,"server":"private"}"#,
    ] {
        assert!(serde_json::from_str::<SendDiagnostic>(json).is_err());
    }
}

#[test]
fn diagnostic_reads_and_writes_enforce_account_permissions_without_existence_leak() {
    let (_dir, path) = database();
    let mut store = open(&path);
    let sub = claim(&mut store);
    for (account, read, send) in [("other", true, true), ("demo", false, false)] {
        let mut denied = Store::open(&path)
            .unwrap()
            .with_policy(policy(account, read, send));
        assert_eq!(denied.get_submission_diagnostic(sub.id), Err(Error::Denied));
        assert_eq!(
            denied.get_submission_diagnostic(i64::MAX),
            Err(Error::Denied)
        );
        assert_eq!(
            denied.finish_submission_with_diagnostic(
                sub.id,
                SendOutcome::Failed,
                Some(diagnostic(None))
            ),
            Err(Error::Denied)
        );
    }
    store
        .finish_submission_with_diagnostic(sub.id, SendOutcome::Failed, Some(diagnostic(None)))
        .unwrap();
    let reader = Store::open(&path)
        .unwrap()
        .with_policy(policy("demo", true, false));
    assert_eq!(
        reader.get_submission_diagnostic(sub.id).unwrap(),
        Some(diagnostic(None))
    );
    assert_eq!(
        reader.get_submission_diagnostic(i64::MAX),
        Err(Error::Denied)
    );
    let no_read = Store::open(&path)
        .unwrap()
        .with_policy(policy("demo", false, true));
    assert_eq!(
        no_read.get_submission_diagnostic(sub.id),
        Err(Error::Denied)
    );
}

#[test]
fn diagnostic_and_state_commit_or_rollback_together() {
    let (_dir, path) = database();
    let mut store = open(&path);
    let sub = claim(&mut store);
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TRIGGER diagnostic_write_failure BEFORE INSERT ON submission_diagnostics BEGIN SELECT RAISE(ABORT, 'synthetic failure'); END;").unwrap();
    assert_eq!(
        store.finish_submission_with_diagnostic(
            sub.id,
            SendOutcome::Uncertain,
            Some(diagnostic(None))
        ),
        Err(Error::Storage)
    );
    drop(store);
    let mut store = open(&path);
    assert_eq!(store.get_submission(sub.id).unwrap(), sub);
    assert_eq!(store.get_submission_diagnostic(sub.id).unwrap(), None);
    conn.execute_batch("DROP TRIGGER diagnostic_write_failure; CREATE TRIGGER state_write_failure BEFORE UPDATE OF state ON submissions BEGIN SELECT RAISE(ABORT, 'synthetic failure'); END;").unwrap();
    assert_eq!(
        store.finish_submission_with_diagnostic(
            sub.id,
            SendOutcome::Failed,
            Some(diagnostic(None))
        ),
        Err(Error::Storage)
    );
    assert_eq!(store.get_submission(sub.id).unwrap(), sub);
    assert_eq!(store.get_submission_diagnostic(sub.id).unwrap(), None);
    conn.execute_batch("DROP TRIGGER state_write_failure;")
        .unwrap();
    store
        .finish_submission_with_diagnostic(sub.id, SendOutcome::Uncertain, Some(diagnostic(None)))
        .unwrap();
}

#[test]
fn legacy_schema_migration_preserves_snapshots_and_recovery_has_no_diagnostic() {
    for version in [0, 1, 2] {
        let (_dir, path) = database();
        let mut store = open(&path);
        let sub = if version == 0 {
            None
        } else {
            Some(claim(&mut store))
        };
        drop(store);
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("DROP TABLE IF EXISTS submission_diagnostics;")
            .unwrap();
        if version < 2 {
            conn.execute_batch("DROP TABLE submission_wire; DROP TABLE sync_records;")
                .unwrap();
        }
        if version == 0 {
            conn.execute_batch("DROP TABLE approvals; DROP TABLE submissions; DROP TABLE drafts;")
                .unwrap();
        }
        conn.pragma_update(None, "user_version", version).unwrap();
        drop(conn);
        let mut store = open(&path);
        let current: i64 = Connection::open(&path)
            .unwrap()
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(current, 3);
        if let Some(sub) = sub {
            assert_eq!(store.get_submission(sub.id).unwrap(), sub);
            assert_eq!(store.get_submission_diagnostic(sub.id).unwrap(), None);
            assert_eq!(
                store.recover_interrupted(sub.id).unwrap().state,
                SubmissionState::Uncertain
            );
            assert_eq!(store.get_submission_diagnostic(sub.id).unwrap(), None);
            assert!(store.claim_send(sub.draft_id, 2).is_err());
        } else {
            claim(&mut store);
        }
    }
}

#[test]
fn sqlite_rejects_untyped_diagnostics_and_foreign_submission_ids() {
    let (_dir, path) = database();
    let mut store = open(&path);
    let sub = claim(&mut store);
    let conn = Connection::open(&path).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    for (stage, kind, code, id) in [
        ("unknown", "timeout", Some(451), sub.id),
        ("submission", "unknown", Some(451), sub.id),
        ("submission", "timeout", Some(99), sub.id),
        ("submission", "timeout", Some(600), sub.id),
        ("submission", "timeout", None, i64::MAX),
    ] {
        assert!(conn.execute("INSERT INTO submission_diagnostics(submission_id,stage,kind,smtp_code) VALUES(?1,?2,?3,?4)", rusqlite::params![id,stage,kind,code]).is_err());
    }
    store
        .finish_submission_with_diagnostic(sub.id, SendOutcome::Failed, Some(diagnostic(None)))
        .unwrap();
    assert!(conn.execute("INSERT INTO submission_diagnostics(submission_id,stage,kind,smtp_code) VALUES(?1,'submission','timeout',NULL)", [sub.id]).is_err());
}
