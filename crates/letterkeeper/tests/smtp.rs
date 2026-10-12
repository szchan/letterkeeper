// SPDX-License-Identifier: GPL-3.0-only
use letterkeeper::transport::{prepare, send_test};
use letterkeeper_core::{DraftInput, SendOutcome, Submission, SubmissionState};
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

fn submission() -> Submission {
    Submission {
        id: 1, draft_id: 1, revision: 1, digest: "synthetic".into(),
        message_id: "prototype-1@letterkeeper.invalid".into(), state: SubmissionState::Submitting,
        input: DraftInput { account: "local-demo".into(), from: "sender@example.test".into(), to: vec!["to@example.test".into()], cc: vec![], bcc: vec!["hidden@example.test".into()], subject: "中文测试".into(), body: "A synthetic message.\n.leading dot".into(), attachments: vec![] },
    }
}

#[derive(Clone, Copy)]
enum Behavior { Accept, Reject, DropAfterData }
struct Capture { recipients: Vec<String>, body: String }
fn server(behavior: Behavior) -> (SocketAddr, Receiver<Capture>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        stream.write_all(b"220 fixture.example.test ESMTP\r\n").unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut recipients = Vec::new();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 { break; }
            if line.starts_with("EHLO ") {
                stream.write_all(b"250-fixture.example.test\r\n250 8BITMIME\r\n").unwrap();
            } else if line.starts_with("MAIL FROM:") {
                stream.write_all(b"250 sender ok\r\n").unwrap();
            } else if line.starts_with("RCPT TO:") {
                recipients.push(line.trim().to_owned());
                stream.write_all(b"250 recipient ok\r\n").unwrap();
            } else if line.trim() == "DATA" {
                stream.write_all(b"354 send data\r\n").unwrap();
                let mut body = String::new();
                loop {
                    let mut part = String::new();
                    if reader.read_line(&mut part).unwrap_or(0) == 0 { return; }
                    if part == ".\r\n" { break; }
                    body.push_str(&part);
                }
                tx.send(Capture { recipients: recipients.clone(), body }).unwrap();
                match behavior {
                    Behavior::Accept => stream.write_all(b"250 accepted\r\n").unwrap(),
                    Behavior::Reject => stream.write_all(b"550 rejected\r\n").unwrap(),
                    Behavior::DropAfterData => return,
                }
            } else if line.trim() == "QUIT" {
                let _ = stream.write_all(b"221 bye\r\n"); break;
            } else { stream.write_all(b"250 ok\r\n").unwrap(); }
        }
    });
    (address, rx)
}

#[test]
fn prepared_envelope_includes_bcc_but_headers_do_not() {
    let message = prepare(&submission()).unwrap();
    assert_eq!(message.envelope().to().len(), 2);
    let rendered = String::from_utf8(message.formatted()).unwrap();
    assert!(!rendered.to_lowercase().contains("bcc:"));
    assert!(!rendered.contains("hidden@example.test"));
    assert!(rendered.contains("prototype-1@letterkeeper.invalid"));
}
#[test]
fn smtp_acceptance_and_actual_envelope_are_recorded() {
    let (endpoint, rx) = server(Behavior::Accept);
    assert!(matches!(send_test(&submission(), endpoint), SendOutcome::Accepted));
    let capture = rx.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(capture.recipients.len(), 2);
    assert!(capture.recipients.iter().any(|r| r.contains("hidden@example.test")));
    assert!(!capture.body.to_lowercase().contains("bcc:"));
}
#[test]
fn explicit_smtp_rejection_is_a_definite_failure() {
    let (endpoint, _) = server(Behavior::Reject);
    assert!(matches!(send_test(&submission(), endpoint), SendOutcome::Failed));
}
#[test]
fn disconnect_after_data_is_uncertain_and_not_retried() {
    let (endpoint, rx) = server(Behavior::DropAfterData);
    assert!(matches!(send_test(&submission(), endpoint), SendOutcome::Uncertain));
    assert!(rx.recv_timeout(Duration::from_secs(3)).is_ok());
    assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
}
#[test]
fn test_transport_rejects_non_loopback_endpoints_before_network() {
    assert!(matches!(send_test(&submission(), "192.0.2.1:25".parse().unwrap()), SendOutcome::Failed));
}
