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
    let doc = sandbox()
        .parse(raw.as_bytes())
        .expect("isolated parsing succeeds");
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
    assert_eq!(
        sandbox.parse(&vec![b'a'; 2 * 1024 * 1024 + 1]).unwrap_err(),
        "message_too_large"
    );
}

#[test]
fn internal_worker_cannot_be_used_as_an_unconfined_reader() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_letterkeeper"))
        .arg("__worker")
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn parser_network_namespace_blocks_http_dns_and_host_files() {
    use std::net::{TcpListener, TcpStream, UdpSocket};
    let http = TcpListener::bind("127.0.0.1:0").unwrap();
    let dns = UdpSocket::bind("127.0.0.1:0").unwrap();
    let http_addr = http.local_addr().unwrap();
    let dns_addr = dns.local_addr().unwrap();
    // Positive controls: both observers can receive traffic in the parent.
    let connection = TcpStream::connect(http_addr).unwrap();
    let _ = http.accept().unwrap();
    drop(connection);
    let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
    sender
        .send_to(b"synthetic-positive-control", dns_addr)
        .unwrap();
    let mut packet = [0; 512];
    assert!(dns.recv_from(&mut packet).unwrap().0 > 0);
    http.set_nonblocking(true).unwrap();
    dns.set_nonblocking(true).unwrap();
    let private = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(private.path(), b"synthetic-host-sentinel").unwrap();
    let raw = format!(
        "Subject: canary\r\nContent-Type: text/html\r\n\r\n<p>safe text</p><img src=\"http://{http_addr}/pixel\"><link rel=\"dns-prefetch\" href=\"//tracker.example.test\"><style>@import url(http://{http_addr}/style);</style>"
    );
    let report = sandbox()
        .probe(raw.as_bytes(), http_addr, dns_addr, private.path())
        .unwrap();
    assert!(report.parsed);
    assert!(!report.http_connected);
    assert!(!report.dns_reply);
    assert!(!report.host_file_readable);
    assert_eq!(
        http.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(
        dns.recv_from(&mut packet).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
