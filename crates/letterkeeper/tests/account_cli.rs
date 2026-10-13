// SPDX-License-Identifier: GPL-3.0-only
use serde_json::json;
use std::process::Command;
fn command(config: &std::path::Path, data: &std::path::Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_letterkeeper"));
    c.arg("--config-dir")
        .arg(config)
        .arg("--data-dir")
        .arg(data);
    c
}
#[test]
fn cli_imports_public_account_config_and_creates_scoped_local_draft() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config");
    let data = dir.path().join("data");
    let fixture = dir.path().join("account.json");
    std::fs::write(&fixture,serde_json::to_vec(&json!({"id":"controlled","email":"reader@example.test","username":"person@example.test","imap":{"host":"localhost","port":9993},"smtp":{"host":"localhost","port":9465},"allowed_folders":["INBOX","Drafts","Sent"],"permissions":{"read":true,"draft":true,"send":true},"drafts_folder":"Drafts","sent_folder":"Sent","sent_copy":"server_managed","authentication":{"type":"password"},"extra_ca_pem":null})).unwrap()).unwrap();
    let imported = command(&config, &data)
        .args(["account", "import"])
        .arg(&fixture)
        .output()
        .unwrap();
    assert!(
        imported.status.success(),
        "{}",
        String::from_utf8_lossy(&imported.stderr)
    );
    let listed = command(&config, &data)
        .args(["account", "list"])
        .output()
        .unwrap();
    assert!(listed.status.success());
    assert!(String::from_utf8_lossy(&listed.stdout).contains("controlled"));
    let draft = dir.path().join("draft.json");
    std::fs::write(&draft,r#"{"account":"controlled","from":"reader@example.test","to":["to@example.test"],"cc":[],"bcc":[],"subject":"synthetic","body":"test","attachments":[],"in_reply_to":"<original@example.test>","references":["<original@example.test>"]}"#).unwrap();
    let created = command(&config, &data)
        .args(["--account", "controlled", "draft", "create"])
        .arg(&draft)
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&created.stdout).unwrap();
    assert_eq!(v["account"], "controlled");
    assert_eq!(v["in_reply_to"], "<original@example.test>");
    let approve = command(&config, &data)
        .args(["--account", "controlled", "draft", "approve", "1"])
        .output()
        .unwrap();
    assert!(!approve.status.success());
    assert!(String::from_utf8_lossy(&approve.stderr).contains("interactive_terminal_required"));
    let password = command(&config, &data)
        .args(["account", "password", "controlled"])
        .output()
        .unwrap();
    assert!(!password.status.success());
    assert!(String::from_utf8_lossy(&password.stderr).contains("interactive_terminal_required"));
}
#[test]
fn cli_does_not_accept_secrets_in_account_configuration_or_arguments() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config");
    let data = dir.path().join("data");
    let fixture = dir.path().join("bad.json");
    std::fs::write(
        &fixture,
        r#"{"id":"controlled","password":"synthetic-canary"}"#,
    )
    .unwrap();
    let output = command(&config, &data)
        .args(["account", "import"])
        .arg(&fixture)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("synthetic-canary"));
    let output = command(&config, &data)
        .args([
            "account",
            "password",
            "controlled",
            "--password",
            "synthetic-canary",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let help = Command::new(env!("CARGO_BIN_EXE_letterkeeper"))
        .arg("--help")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&help.stdout);
    assert!(
        text.lines()
            .any(|line| line.trim_start().starts_with("account "))
    );
    assert!(
        text.lines()
            .any(|line| line.trim_start().starts_with("mail "))
    );
}
