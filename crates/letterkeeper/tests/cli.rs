// SPDX-License-Identifier: GPL-3.0-only
use std::process::Command;

#[test]
fn help_describes_the_offline_prototype_and_entry_points() {
    let output = Command::new(env!("CARGO_BIN_EXE_letterkeeper")).arg("--help").output().unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    for command in ["read", "tui", "mcp", "draft", "send", "status"] { assert!(text.contains(command)); }
    assert!(text.contains("prototype"));
}

#[test]
fn cli_creates_persistent_draft_and_rejects_piped_approval() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("private");
    let file = dir.path().join("draft.json");
    std::fs::write(&file, r#"{"account":"local-demo","from":"sender@example.test","to":["recipient@example.test"],"cc":[],"bcc":[],"subject":"中文草稿","body":"合成测试正文","attachments":[]}"#).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_letterkeeper")).args(["--state-dir"]).arg(&state).args(["draft", "create"]).arg(&file).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let draft: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let id = draft["id"].to_string();
    let read = Command::new(env!("CARGO_BIN_EXE_letterkeeper")).arg("--state-dir").arg(&state).args(["draft", "show", &id]).output().unwrap();
    assert!(read.status.success());
    assert!(String::from_utf8(read.stdout).unwrap().contains("中文草稿"));
    let approval = Command::new(env!("CARGO_BIN_EXE_letterkeeper")).arg("--state-dir").arg(&state).args(["draft", "approve", &id]).output().unwrap();
    assert!(!approval.status.success());
    assert!(String::from_utf8_lossy(&approval.stderr).contains("interactive_terminal_required"));
}
