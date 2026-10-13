// SPDX-License-Identifier: GPL-3.0-only
use rmcp::{ServiceExt, model::CallToolRequestParams};
use serde_json::{Value, json};
use std::{
    net::{TcpListener, UdpSocket},
    process::Stdio,
    time::Duration,
};

#[tokio::test]
async fn actual_stdio_client_reads_safe_mail_and_cannot_approve() {
    let dir = tempfile::tempdir().unwrap();
    let http = TcpListener::bind("127.0.0.1:0").unwrap();
    http.set_nonblocking(true).unwrap();
    let dns = UdpSocket::bind("127.0.0.1:0").unwrap();
    dns.set_nonblocking(true).unwrap();
    let eml = dir.path().join("synthetic.eml");
    std::fs::write(&eml,format!("From: sender@example.test\r\nSubject: 中文安全测试\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<p>你好</p><img src=\"http://{}/track\"><a href=\"http://tracker.example.test\">click</a><script>alert(1)</script>",http.local_addr().unwrap())).unwrap();
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_letterkeeper"))
        .arg("--state-dir")
        .arg(dir.path().join("private"))
        .args(["mcp", "--mail"])
        .arg(&eml)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let input = child.stdin.take().unwrap();
    let output = child.stdout.take().unwrap();
    let client = tokio::time::timeout(Duration::from_secs(10), ().serve((output, input)))
        .await
        .unwrap()
        .unwrap();
    let list = client.list_all_tools().await.unwrap();
    assert_eq!(list.len(), 14);
    for name in [
        "list_accounts",
        "list_folders",
        "search_messages",
        "reply_draft",
        "sync_draft",
        "sync_sent",
        "sync_status",
        "sent_evidence",
    ] {
        assert!(list.iter().any(|tool| tool.name == name));
    }
    assert!(!list.iter().any(|t| t.name.contains("approve")));
    let read = client
        .call_tool(
            CallToolRequestParams::new("read_message")
                .with_arguments(json!({"id":"mail-1"}).as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    let text = read.content[0].as_text().unwrap().text.as_str();
    assert!(text.contains("你好"));
    for forbidden in ["http://", "https://", "<script", "<img", "alert(1)"] {
        assert!(!text.contains(forbidden));
    }
    let data: Value = serde_json::from_str(text).unwrap();
    assert_eq!(data["untrusted"], true);
    let forged = client
        .call_tool(
            CallToolRequestParams::new("send_draft").with_arguments(
                json!({"id":"1","confirm":true})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(forged.is_error, Some(true));
    let draft = client
        .call_tool(
            CallToolRequestParams::new("create_draft").with_arguments(
                json!({"to":["recipient@example.test"],"subject":"Re test","body":"合成回复"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_ne!(draft.is_error, Some(true));
    let draft: Value = serde_json::from_str(&draft.content[0].as_text().unwrap().text).unwrap();
    let rejected = client
        .call_tool(
            CallToolRequestParams::new("send_draft").with_arguments(
                json!({"id":draft["id"].to_string()})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(rejected.is_error, Some(true));
    assert_eq!(
        http.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(
        dns.recv_from(&mut [0; 512]).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    client.cancel().await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), child.wait())
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn oversized_unterminated_stdio_frame_closes_session_before_json_decoding() {
    use tokio::io::AsyncWriteExt;
    let dir = tempfile::tempdir().unwrap();
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_letterkeeper"))
        .arg("--state-dir")
        .arg(dir.path().join("private"))
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    // Keep stdin open: an unbounded read_until would keep waiting for a newline.
    let mut input = child.stdin.take().unwrap();
    input.write_all(&vec![b' '; 128 * 1024 + 1]).await.unwrap();
    let status = tokio::time::timeout(Duration::from_secs(2), child.wait())
        .await
        .expect("oversized input must terminate the MCP session")
        .unwrap();
    assert!(!status.success());
}
