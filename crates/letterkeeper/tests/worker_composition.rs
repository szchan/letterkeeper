// SPDX-License-Identifier: GPL-3.0-only
use letterkeeper::files::{export_attachment, import_attachment};
use letterkeeper::sandbox::Sandbox;
use std::path::PathBuf;
fn worker() -> Sandbox {
    Sandbox {
        executable: PathBuf::from(env!("CARGO_BIN_EXE_letterkeeper")),
        launcher: "/usr/bin/bwrap".into(),
    }
}
fn mail() -> Vec<u8> {
    b"From: sender@example.test\r\nReply-To: replies@example.test\r\nTo: reader@example.test\r\nSubject: synthetic\r\nMessage-ID: <original@example.test>\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=x\r\n\r\n--x\r\nContent-Type: text/html\r\n\r\n<p>safe body</p><img src=http://tracker.example.test/track>\r\n--x\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=note.bin\r\nContent-Transfer-Encoding: base64\r\n\r\nAAECAw==\r\n--x--\r\n".to_vec()
}
#[test]
fn isolated_worker_returns_thread_metadata_and_exact_attachment_bytes() {
    let inspected = worker().inspect(&mail()).unwrap();
    assert_eq!(
        inspected.reply.message_id.as_deref(),
        Some("<original@example.test>")
    );
    assert_eq!(inspected.reply.reply_to, vec!["replies@example.test"]);
    assert!(!inspected.document.text.contains("<img"));
    let id = inspected.document.attachments[0].id;
    let attachment = worker().attachment(&mail(), id).unwrap();
    assert_eq!(attachment.bytes, [0, 1, 2, 3]);
    assert!(worker().attachment(&mail(), 999).is_err());
}
#[test]
fn new_worker_entries_fail_closed_without_isolation() {
    for entry in ["__inspect", "__attachment"] {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_letterkeeper"))
            .args([entry, "1"])
            .output()
            .unwrap();
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("worker_requires_isolation"));
    }
}
#[test]
fn attachment_files_are_explicit_private_and_never_overwrite_or_follow_links() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("合成.bin");
    std::fs::write(&input, [0, 1, 2, 3]).unwrap();
    let item = import_attachment(&input, "application/octet-stream").unwrap();
    assert_eq!(item.bytes, [0, 1, 2, 3]);
    let link = dir.path().join("alias");
    symlink(&input, &link).unwrap();
    assert!(import_attachment(&link, "text/plain").is_err());
    export_attachment(dir.path(), "saved.bin", &item.bytes).unwrap();
    let saved = dir.path().join("saved.bin");
    assert_eq!(std::fs::read(&saved).unwrap(), item.bytes);
    assert_eq!(
        std::fs::metadata(&saved).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(export_attachment(dir.path(), "saved.bin", b"overwrite").is_err());
    assert!(export_attachment(dir.path(), "../escape", b"unsafe").is_err());
    assert!(export_attachment(dir.path(), "alias", b"unsafe").is_err());
    std::fs::write(&input, vec![0; 512 * 1024 + 1]).unwrap();
    assert!(import_attachment(&input, "application/octet-stream").is_err());
}
#[test]
fn smtp_builder_emits_approved_thread_headers_without_bcc() {
    let mut input: letterkeeper_core::DraftInput =
        serde_json::from_str(include_str!("../../../tests/fixtures/draft.json")).unwrap();
    input.in_reply_to = Some("<original@example.test>".into());
    input.references = vec![
        "<root@example.test>".into(),
        "<original@example.test>".into(),
    ];
    input.bcc.push("hidden@example.test".into());
    let sub = letterkeeper_core::Submission {
        id: 1,
        draft_id: 1,
        revision: 1,
        digest: "synthetic".into(),
        message_id: "<sent@example.test>".into(),
        input,
        state: letterkeeper_core::SubmissionState::Submitting,
    };
    let wire =
        String::from_utf8(letterkeeper::transport::prepare(&sub).unwrap().formatted()).unwrap();
    assert!(wire.contains("In-Reply-To: <original@example.test>"));
    assert!(wire.contains("References: <root@example.test> <original@example.test>"));
    assert!(!wire.contains("Bcc:"));
}
