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
fn smtp_report_preserves_auth_rejection_stage_without_server_text() {
    let server = Server::start(Scenario::SmtpAuthReject);
    let mut credentials = Server::credentials();
    credentials.oauth = true;
    let report = send_smtp_report(
        &server.endpoint,
        &server.tls,
        &credentials,
        "person@example.test",
        &["recipient@example.test".into()],
        b"From: person@example.test\r\n\r\nsynthetic",
    );
    let transcript = server.finish();
    assert!(
        transcript
            .iter()
            .any(|line| line.starts_with("AUTH XOAUTH2"))
    );
    assert!(!transcript.iter().any(|line| line.starts_with("MAIL")));
    assert_eq!(report.outcome, WriteOutcome::Failed);
    assert_eq!(
        report.diagnostic,
        Some(SmtpDiagnostic {
            stage: SmtpStage::Authentication,
            kind: SmtpFailureKind::Rejected,
            smtp_code: Some(535),
        }),
        "SMTP AUTH refusal must retain its fixed stage, kind and numeric code",
    );
    let displayed = format!("{report:?}");
    for sensitive in [
        "synthetic-private-response",
        "person@example.test",
        "synthetic-secret",
    ] {
        assert!(!displayed.contains(sensitive));
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
async fn blocking_api_inside_runtime_uses_safe_bridge_without_panicking() {
    let server = Server::start(Scenario::Imap);
    assert_eq!(client(&server).list_folders().unwrap()[0].name, "INBOX");
    server.finish();
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

#[tokio::test]
async fn tls_operations_work_from_std_thread_and_tokio_spawn_blocking() {
    let server = Server::start(Scenario::Imap);
    let c = client(&server);
    let direct = std::thread::spawn(move || c.list_messages("INBOX", 0, 1, None))
        .join()
        .unwrap()
        .unwrap();
    assert_eq!(direct.messages[0].uid, 11);
    server.finish();
    let server = Server::start(Scenario::Imap);
    let c = client(&server);
    let blocked = tokio::task::spawn_blocking(move || c.list_messages("INBOX", 0, 1, None))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(blocked.messages[0].uid, 11);
    server.finish();
}

#[test]
fn unicode_search_declares_utf8_and_preserves_quotes_backslashes() {
    let server = Server::start(Scenario::Utf8Search);
    let page = client(&server)
        .list_messages("INBOX", 0, 1, Some("中文\"\\查询"))
        .unwrap();
    assert_eq!(page.messages[0].uid, 11);
    let transcript = server.finish();
    assert_eq!(
        transcript
            .iter()
            .filter(|s| s.contains("UID SEARCH"))
            .count(),
        1
    );
    assert!(
        transcript
            .iter()
            .any(|s| s.contains("UID SEARCH CHARSET UTF-8 1:3 TEXT"))
    );
}
#[test]
fn unsupported_utf8_charset_returns_fixed_error_without_fallback_or_retry() {
    let server = Server::start(Scenario::BadCharset);
    let error = client(&server)
        .list_messages("INBOX", 0, 1, Some("中文\"\\private-search"))
        .unwrap_err();
    assert_eq!(error, ProtocolError::Rejected);
    assert_eq!(error.to_string(), "protocol operation failed");
    let transcript = server.finish();
    assert_eq!(
        transcript
            .iter()
            .filter(|s| s.contains("UID SEARCH"))
            .count(),
        1
    );
    assert!(
        transcript
            .iter()
            .any(|s| s.contains("UID SEARCH CHARSET UTF-8 1:3 TEXT"))
    );
    assert!(!transcript.iter().any(|s| s.contains("UID FETCH")));
    assert!(
        !transcript
            .iter()
            .any(|s| s.contains("search-literal") || s.contains("search-continuation"))
    );
}

#[test]
fn ascii_search_refusal_is_not_a_successful_empty_page() {
    let server = Server::start(Scenario::BadCharset);
    assert_eq!(
        client(&server)
            .list_messages("INBOX", 0, 1, Some("ascii"))
            .unwrap_err(),
        ProtocolError::Rejected
    );
    let log = server.finish();
    assert!(
        log.iter()
            .any(|s| s.contains("UID SEARCH 1:3 TEXT \"ascii\""))
    );
    assert_eq!(log.iter().filter(|s| s.contains("UID SEARCH")).count(), 1);
}

#[test]
fn large_mailbox_first_page_uses_bounded_search_instead_of_all_uids() {
    use support::{PageMode, page_uid};
    // The old ALL response is 220 KiB: valid synthetic UIDs, over the 64 KiB
    // line budget, well under the existing 8 MiB operation budget.
    let server = Server::start(Scenario::Paged {
        count: 20_000,
        mode: PageMode::All,
    });
    let page = client(&server).list_messages("INBOX", 0, 50, None).unwrap();
    assert_eq!(
        page.messages.iter().map(|m| m.uid).collect::<Vec<_>>(),
        (1..=50).map(page_uid).collect::<Vec<_>>()
    );
    assert!(page.more);
    let log = server.finish();
    assert_eq!(log.iter().filter(|s| s.contains("UID SEARCH")).count(), 1);
    assert!(log.iter().any(|s| s.contains("UID SEARCH 1:4096 ALL")));
    assert!(log.iter().any(|s| s.contains("EXAMINE")));
    assert!(log.iter().any(|s| s.contains("BODY.PEEK[HEADER.FIELDS")));
    assert!(
        !log.iter()
            .any(|s| s.contains("SELECT") || s.contains("STORE"))
    );
}

#[test]
fn bounded_pages_preserve_offsets_boundary_more_and_empty_mailbox() {
    use support::{PageMode, page_uid};
    for (count, offset, limit, expected, more) in [
        (4100, 4094, 3, vec![4095, 4096, 4097], true),
        (4100, 4097, 3, vec![4098, 4099, 4100], false),
        (4100, 4100, 3, vec![], false),
        (4096, 4095, 1, vec![4096], false),
        (0, 0, 50, vec![], false),
    ] {
        let server = Server::start(Scenario::Paged {
            count,
            mode: PageMode::All,
        });
        let page = client(&server)
            .list_messages("INBOX", offset, limit, None)
            .unwrap();
        assert_eq!(
            page.messages.iter().map(|m| m.uid).collect::<Vec<_>>(),
            expected.into_iter().map(page_uid).collect::<Vec<_>>()
        );
        assert_eq!(page.more, more);
        let log = server.finish();
        if count == 0 {
            assert!(
                !log.iter()
                    .any(|s| s.contains("SEARCH") || s.contains("FETCH"))
            );
        }
    }
}

#[test]
fn bounded_sparse_unicode_search_keeps_literal_and_scans_in_order() {
    use support::{PageMode, page_uid};
    let server = Server::start(Scenario::Paged {
        count: 20_000,
        mode: PageMode::Sparse,
    });
    let page = client(&server)
        .list_messages("INBOX", 3, 2, Some("中文\"\\查询"))
        .unwrap();
    assert_eq!(
        page.messages.iter().map(|m| m.uid).collect::<Vec<_>>(),
        vec![page_uid(4000), page_uid(5000)]
    );
    assert!(page.more);
    let log = server.finish();
    assert_eq!(log.iter().filter(|s| s.contains("UID SEARCH")).count(), 2);
    assert_eq!(
        log.iter()
            .filter(|s| s.as_str() == "search-literal:中文\"\\查询")
            .count(),
        2
    );
    assert!(
        log.iter()
            .any(|s| s.contains("CHARSET UTF-8 4097:8192 TEXT"))
    );
}

#[test]
fn bounded_search_rejects_unsafe_results_and_mailbox_drift() {
    use support::PageMode;
    for (mode, expected) in [
        (PageMode::Rejected, ProtocolError::Rejected),
        (PageMode::Duplicate, ProtocolError::Rejected),
        (PageMode::Zero, ProtocolError::Rejected),
        (PageMode::TooMany, ProtocolError::Limit),
        (PageMode::Expunge, ProtocolError::StaleMailbox),
        (PageMode::ChangedExists, ProtocolError::StaleMailbox),
        (PageMode::ChangedValidity, ProtocolError::StaleMailbox),
        (PageMode::WrongTag, ProtocolError::Rejected),
        (PageMode::MissingSearch, ProtocolError::Rejected),
        (PageMode::MultipleSearch, ProtocolError::Rejected),
        (PageMode::HugeLine, ProtocolError::Limit),
    ] {
        let server = Server::start(Scenario::Paged {
            count: 20_000,
            mode,
        });
        assert_eq!(
            client(&server)
                .list_messages("INBOX", 0, 1, None)
                .unwrap_err(),
            expected
        );
        assert!(!server.finish().iter().any(|s| s.contains("FETCH")));
    }
}

#[test]
fn bounded_search_budget_applies_when_no_messages_match() {
    use support::PageMode;
    let server = Server::start(Scenario::Paged {
        count: 524_289,
        mode: PageMode::Empty,
    });
    assert_eq!(
        client(&server)
            .list_messages("INBOX", 0, 1, Some("no matches"))
            .unwrap_err(),
        ProtocolError::Limit
    );
    let log = server.finish();
    assert_eq!(log.iter().filter(|s| s.contains("UID SEARCH")).count(), 128);
    assert!(!log.iter().any(|s| s.contains("FETCH")));
}

#[test]
fn bounded_page_rejects_missing_duplicate_and_unrequested_fetches() {
    use support::PageMode;
    for mode in [
        PageMode::MissingFetch,
        PageMode::DuplicateFetch,
        PageMode::UnexpectedFetch,
    ] {
        let server = Server::start(Scenario::Paged {
            count: 20_000,
            mode,
        });
        assert_eq!(
            client(&server)
                .list_messages("INBOX", 0, 2, None)
                .unwrap_err(),
            ProtocolError::Rejected
        );
        server.finish();
    }
}

#[test]
fn page_fetch_requires_successful_exact_tagged_completion() {
    use support::PageMode;
    for mode in [PageMode::FetchRejected, PageMode::FetchWrongTag] {
        // Small enough that the baseline ALL search succeeds and reaches FETCH.
        let server = Server::start(Scenario::Paged { count: 3, mode });
        assert_eq!(
            client(&server)
                .list_messages("INBOX", 0, 2, None)
                .unwrap_err(),
            ProtocolError::Rejected
        );
        assert!(server.finish().iter().any(|s| s.contains("UID FETCH")));
    }
}
#[test]
fn page_fetch_rejects_mailbox_drift_after_complete_headers() {
    use support::PageMode;
    let server = Server::start(Scenario::Paged {
        count: 3,
        mode: PageMode::FetchDrift,
    });
    assert_eq!(
        client(&server)
            .list_messages("INBOX", 0, 2, None)
            .unwrap_err(),
        ProtocolError::StaleMailbox
    );
    assert!(server.finish().iter().any(|s| s.contains("UID FETCH")));
}

#[test]
fn bounded_search_rejects_cross_window_uid_regression() {
    use support::PageMode;
    let server = Server::start(Scenario::Paged {
        count: 8192,
        mode: PageMode::CrossWindowOverlap,
    });
    assert_eq!(
        client(&server)
            .list_messages("INBOX", 4096, 1, None)
            .unwrap_err(),
        ProtocolError::Rejected
    );
    let log = server.finish();
    assert_eq!(log.iter().filter(|s| s.contains("UID SEARCH")).count(), 2);
    assert!(!log.iter().any(|s| s.contains("FETCH")));
}

#[test]
fn bounded_ascii_matches_and_empty_search_complete_within_budget() {
    use support::{PageMode, page_uid};
    let server = Server::start(Scenario::Paged {
        count: 20_000,
        mode: PageMode::Sparse,
    });
    let page = client(&server)
        .list_messages("INBOX", 7, 2, Some("matched text"))
        .unwrap();
    assert_eq!(
        page.messages.iter().map(|m| m.uid).collect::<Vec<_>>(),
        vec![page_uid(8000), page_uid(9000)]
    );
    assert!(page.more);
    let log = server.finish();
    assert_eq!(log.iter().filter(|s| s.contains("UID SEARCH")).count(), 3);
    assert!(
        log.iter()
            .any(|s| s.contains("8193:12288 TEXT \"matched text\""))
    );
    let server = Server::start(Scenario::Paged {
        count: 8193,
        mode: PageMode::Empty,
    });
    let page = client(&server)
        .list_messages("INBOX", 0, 50, Some("absent text"))
        .unwrap();
    assert!(page.messages.is_empty());
    assert!(!page.more);
    let log = server.finish();
    assert_eq!(log.iter().filter(|s| s.contains("UID SEARCH")).count(), 3);
    assert!(!log.iter().any(|s| s.contains("FETCH")));
}

#[test]
fn bounded_search_sorts_arbitrary_wire_order_before_paging() {
    use support::{PageMode, page_uid};
    let server = Server::start(Scenario::Paged {
        count: 100,
        mode: PageMode::ReverseOrder,
    });
    let page = client(&server).list_messages("INBOX", 5, 2, None).unwrap();
    assert_eq!(
        page.messages.iter().map(|m| m.uid).collect::<Vec<_>>(),
        vec![page_uid(6), page_uid(7)]
    );
    assert!(page.more);
    server.finish();
}
