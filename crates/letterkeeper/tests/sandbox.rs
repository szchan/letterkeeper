// SPDX-License-Identifier: GPL-3.0-only
use letterkeeper::sandbox::Sandbox;
use std::path::PathBuf;

fn sandbox() -> Sandbox {
    Sandbox {
        executable: PathBuf::from(env!("CARGO_BIN_EXE_letterkeeper")),
        launcher: PathBuf::from("/usr/bin/bwrap"),
    }
}

#[test]
fn isolated_worker_reads_chinese_without_active_content() {
    let raw = "From: sender@example.test\r\nSubject: 中文原型\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<p>你好，世界</p><script>alert(1)</script><img src=\"http://tracker.example.test/pixel\">";
    let doc = sandbox().parse(raw.as_bytes()).expect("isolated parsing succeeds");
    assert_eq!(doc.subject, "中文原型");
    assert!(doc.text.contains("你好，世界"));
    assert!(!doc.text.contains("alert(1)"));
    assert!(!doc.text.contains("<img"));
}

#[test]
fn missing_sandbox_refuses_processing_instead_of_falling_back() {
    let mut sandbox = sandbox();
    sandbox.launcher = PathBuf::from("/nonexistent-letterkeeper-sandbox");
    assert!(sandbox.parse(b"Subject: test\r\n\r\nhello").is_err());
}

#[test]
fn oversized_raw_message_is_rejected_before_launch() {
    let mut sandbox = sandbox();
    sandbox.launcher = PathBuf::from("/nonexistent-letterkeeper-sandbox");
    assert_eq!(sandbox.parse(&vec![b'a'; 2 * 1024 * 1024 + 1]).unwrap_err(), "message_too_large");
}

#[test]
fn internal_worker_cannot_be_used_as_an_unconfined_reader() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_letterkeeper"))
        .arg("__worker").output().unwrap();
    assert!(!output.status.success());
}
