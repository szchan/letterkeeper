// SPDX-License-Identifier: GPL-3.0-only
use std::process::{Command, Stdio};
#[test]
fn tui_help_exposes_read_only_and_explicit_synthetic_transport() {
    let result = Command::new(env!("CARGO_BIN_EXE_letterkeeper"))
        .args(["tui", "--help"])
        .output()
        .unwrap();
    assert!(result.status.success());
    let text = String::from_utf8(result.stdout).unwrap();
    assert!(text.contains("--read-only"));
    assert!(text.contains("--smtp-test"));
}
#[test]
fn noninteractive_tui_rejects_before_reading_mail_or_creating_storage() {
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().join("private");
    let result = Command::new(env!("CARGO_BIN_EXE_letterkeeper"))
        .arg("--state-dir")
        .arg(&state)
        .args(["tui", "--mail", "/does-not-exist/synthetic.eml"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(
        String::from_utf8(result.stderr)
            .unwrap()
            .contains("interactive_terminal_required")
    );
    assert!(!state.exists());
}
