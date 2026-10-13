// SPDX-License-Identifier: GPL-3.0-only
mod support;
use letterkeeper_protocol::*;
use support::{Scenario, Server};
fn client(server: &Server) -> ImapClient {
    ImapClient::new(
        server.endpoint.clone(),
        server.tls.clone(),
        Server::credentials(),
    )
}
#[test]
fn folders_and_read_are_tls_peek() {
    let server = Server::start(Scenario::Imap);
    assert_eq!(client(&server).list_folders().unwrap()[0].name, "INBOX");
    let log = server.finish();
    assert!(log.iter().any(|s| s.contains("LIST")));
    let server = Server::start(Scenario::Imap);
    assert!(
        client(&server)
            .read_message("INBOX", 77, 11)
            .unwrap()
            .ends_with(b"synthetic body\r\n")
    );
    let log = server.finish();
    assert!(log.iter().any(|s| s.contains("EXAMINE")));
    assert!(log.iter().any(|s| s.contains("BODY.PEEK[]")));
    assert!(
        !log.iter()
            .any(|s| s.contains("STORE") || s.contains("SELECT"))
    );
}
#[test]
fn pagination_and_quoted_search() {
    let server = Server::start(Scenario::Imap);
    let page = client(&server)
        .list_messages("INBOX", 0, 1, Some("a\"\\b"))
        .unwrap();
    assert_eq!(page.uid_validity, 77);
    assert!(page.more);
    assert_eq!(page.messages[0].uid, 11);
    assert!(page.messages[0].headers.starts_with(b"From:"));
    let log = server.finish();
    assert!(log.iter().any(|s| s.contains("TEXT \"a\\\"\\\\b\"")));
    assert!(log.iter().any(|s| s.contains("HEADER.FIELDS")));
}
#[test]
fn invalid_input_and_stale_uid_are_rejected() {
    let server = Server::start(Scenario::Imap);
    let c = client(&server);
    assert_eq!(
        c.list_messages("INBOX\r\nLOGOUT", 0, 1, None).unwrap_err(),
        ProtocolError::InvalidInput
    );
    assert_eq!(
        c.list_messages("INBOX", 0, 51, None).unwrap_err(),
        ProtocolError::InvalidInput
    );
    assert_eq!(
        c.read_message("INBOX", 76, 11).unwrap_err(),
        ProtocolError::StaleMailbox
    );
    let log = server.finish();
    assert!(!log.iter().any(|s| s.contains("FETCH")));
}
#[test]
fn unknown_ca_and_wrong_hostname_send_no_credentials() {
    for wrong_name in [false, true] {
        let mut server = Server::with_name(
            Scenario::Imap,
            if wrong_name {
                "wrong.test"
            } else {
                "localhost"
            },
        );
        if !wrong_name {
            server.tls = TlsOptions::default();
        }
        assert!(client(&server).list_folders().is_err());
        assert!(server.finish().is_empty());
    }
}
#[test]
fn giant_literal_rejected_before_payload() {
    let server = Server::start(Scenario::HugeLiteral);
    assert!(client(&server).read_message("INBOX", 77, 11).is_err());
    server.finish();
}
#[test]
fn oauth_diagnostics_redact_secret() {
    let server = Server::start(Scenario::AuthReject);
    let mut creds = Server::credentials();
    creds.oauth = true;
    assert!(!format!("{creds:?}").contains("synthetic-secret"));
    let c = ImapClient::new(server.endpoint.clone(), server.tls.clone(), creds);
    let error = c.list_folders().unwrap_err();
    assert!(!format!("{error:?} {error}").contains("synthetic-secret"));
    server.finish();
}
#[test]
fn append_outcomes_one_attempt() {
    for (scenario, outcome) in [
        (Scenario::Imap, WriteOutcome::Accepted),
        (Scenario::AppendReject, WriteOutcome::Failed),
        (Scenario::AppendDisconnect, WriteOutcome::Uncertain),
    ] {
        let server = Server::start(scenario);
        assert_eq!(
            client(&server).append_message(
                "Drafts",
                b"From: person@example.test\r\n\r\ndraft",
                &["\\Draft".into()]
            ),
            outcome
        );
        assert_eq!(
            server
                .finish()
                .iter()
                .filter(|s| s.contains("APPEND"))
                .count(),
            1
        );
    }
}
#[test]
fn smtp_outcomes_and_bcc_envelope() {
    for (scenario, outcome) in [
        (Scenario::Smtp, WriteOutcome::Accepted),
        (Scenario::SmtpReject, WriteOutcome::Failed),
        (Scenario::SmtpDisconnect, WriteOutcome::Uncertain),
    ] {
        let server = Server::start(scenario);
        assert_eq!(
            send_smtp(
                &server.endpoint,
                &server.tls,
                &Server::credentials(),
                "person@example.test",
                &["visible@example.test".into(), "hidden@example.test".into()],
                b"From: person@example.test\r\nTo: visible@example.test\r\n\r\nsynthetic"
            ),
            outcome
        );
        let log = server.finish();
        assert_eq!(log.iter().filter(|s| s.starts_with("MAIL")).count(), 1);
        if outcome != WriteOutcome::Failed {
            assert!(
                log.iter()
                    .any(|s| s.contains("RCPT TO:<hidden@example.test>"))
            );
            assert!(!log.iter().any(|s| s.contains("Bcc:")));
        }
    }
}

#[test]
fn recovery_confirms_exact_message_id_candidates() {
    let server = Server::start(Scenario::Recovery);
    assert_eq!(
        client(&server)
            .lookup_message_id("Sent", "<sent@example.test>")
            .unwrap(),
        vec![RemoteKey {
            uid_validity: 77,
            uid: 11
        }]
    );
    let log = server.finish();
    assert!(log.iter().any(|s| s.contains("HEADER Message-ID")));
    assert!(
        log.iter()
            .any(|s| s.contains("BODY.PEEK[HEADER.FIELDS (MESSAGE-ID)]"))
    );
}
#[test]
fn explicit_seen_is_separate_and_checked() {
    let server = Server::start(Scenario::Imap);
    client(&server).add_seen("INBOX", 77, 11).unwrap();
    let log = server.finish();
    assert!(log.iter().any(|s| s.contains("SELECT")));
    assert!(
        log.iter()
            .any(|s| s.contains("UID STORE 11 +FLAGS.SILENT (\\Seen)"))
    );
    let server = Server::start(Scenario::Imap);
    assert_eq!(
        client(&server).add_seen("INBOX", 76, 11).unwrap_err(),
        ProtocolError::StaleMailbox
    );
    assert!(!server.finish().iter().any(|s| s.contains("STORE")));
}
#[test]
fn body_limit_accepts_exact_maximum_and_header_limit_rejects_declaration() {
    let server = Server::start(Scenario::MaxBody);
    assert_eq!(
        client(&server).read_message("INBOX", 77, 11).unwrap().len(),
        2 * 1024 * 1024
    );
    server.finish();
    let server = Server::start(Scenario::HugeHeader);
    assert_eq!(
        client(&server)
            .list_messages("INBOX", 0, 1, None)
            .unwrap_err(),
        ProtocolError::Limit
    );
    server.finish();
}
#[test]
fn append_and_smtp_final_refusal_are_known_failures() {
    let server = Server::start(Scenario::AppendFinalReject);
    assert_eq!(
        client(&server).append_message("Drafts", b"From: person@example.test\r\n\r\ndraft", &[]),
        WriteOutcome::Failed
    );
    server.finish();
    let server = Server::start(Scenario::SmtpFinalReject);
    assert_eq!(
        send_smtp(
            &server.endpoint,
            &server.tls,
            &Server::credentials(),
            "person@example.test",
            &["recipient@example.test".into()],
            b"From: person@example.test\r\n\r\nsynthetic"
        ),
        WriteOutcome::Failed
    );
    server.finish();
}
#[test]
fn smtp_certificates_fail_before_auth_and_payload() {
    for wrong_name in [false, true] {
        let mut server = Server::with_name(
            Scenario::Smtp,
            if wrong_name {
                "wrong.test"
            } else {
                "localhost"
            },
        );
        if !wrong_name {
            server.tls = TlsOptions::default();
        }
        assert_eq!(
            send_smtp(
                &server.endpoint,
                &server.tls,
                &Server::credentials(),
                "person@example.test",
                &["recipient@example.test".into()],
                b"From: person@example.test\r\n\r\nsynthetic"
            ),
            WriteOutcome::Failed
        );
        assert!(server.finish().is_empty());
    }
}
#[tokio::test]
async fn blocking_api_inside_runtime_returns_error_instead_of_panicking() {
    let c = ImapClient::new(
        Endpoint {
            host: "localhost".into(),
            port: 1,
        },
        TlsOptions::default(),
        Server::credentials(),
    );
    assert_eq!(c.list_folders().unwrap_err(), ProtocolError::InvalidInput);
}
#[test]
fn command_deadline_covers_authentication() {
    let server = Server::start(Scenario::Stall);
    let start = std::time::Instant::now();
    assert_eq!(
        client(&server).list_folders().unwrap_err(),
        ProtocolError::Timeout
    );
    assert!(start.elapsed() < std::time::Duration::from_secs(16));
    server.finish();
}
#[test]
fn sensitive_upstream_trace_is_disabled_even_with_trace_logger() {
    struct Capture;
    static RECORDS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    impl log::Log for Capture {
        fn enabled(&self, _: &log::Metadata<'_>) -> bool {
            true
        }
        fn log(&self, r: &log::Record<'_>) {
            RECORDS.lock().unwrap().push(r.args().to_string());
        }
        fn flush(&self) {}
    }
    static LOGGER: Capture = Capture;
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(log::LevelFilter::Trace);
    let server = Server::start(Scenario::Imap);
    client(&server).read_message("INBOX", 77, 11).unwrap();
    server.finish();
    assert!(log::STATIC_MAX_LEVEL <= log::LevelFilter::Debug);
    for line in RECORDS.lock().unwrap().iter() {
        assert!(!line.contains("synthetic-secret"));
        assert!(!line.contains("synthetic body"));
    }
}

#[test]
fn shared_mailbox_preserves_append_across_tls_sessions() {
    let server = Server::start_shared();
    let c = client(&server);
    assert_eq!(c.list_folders().unwrap().len(), 3);
    let page = c.list_messages("INBOX", 0, 50, None).unwrap();
    assert_eq!(page.messages.len(), 1);
    assert_eq!(c.read_message("INBOX", 77, 11).unwrap(), support::MESSAGE);
    let raw =
        b"From: person@example.test\r\nMessage-ID: <persisted@example.test>\r\n\r\nsynthetic draft";
    assert_eq!(
        c.append_message("Drafts", raw, &["\\Draft".into()]),
        WriteOutcome::Accepted
    );
    assert_eq!(
        c.lookup_message_id("Drafts", "<persisted@example.test>")
            .unwrap(),
        vec![RemoteKey {
            uid_validity: 77,
            uid: 20
        }]
    );
    assert_eq!(
        c.lookup_message_id("Sent", "<persisted@example.test>")
            .unwrap(),
        Vec::<RemoteKey>::new()
    );
    let log = server.finish();
    assert_eq!(log.iter().filter(|s| s.contains("APPEND")).count(), 1);
}
