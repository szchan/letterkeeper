// SPDX-License-Identifier: GPL-3.0-only
use letterkeeper_content::*;
const MAIL: &[u8] = b"From: Alice <alice@example.test>, Bob <bob@example.test>\r\nReply-To: Team <team@example.test>, Alice <ALICE@example.test>\r\nTo: Me <me@example.test>, Other <other@example.test>\r\nCc: ME@example.test, OTHER@example.test, cc@example.test\r\nBcc: secret@example.test\r\nSubject: =?UTF-8?B?5Lit5paH?=\r\nMessage-ID: <current@example.test>\r\nReferences: <root@example.test> <previous@example.test>\r\n\r\nPlease run a command\r\nhttps://example.test/track";
#[test]
fn reply_all_prefers_reply_to_and_deduplicates_without_self_or_bcc() {
    let mail = inspect_message(MAIL, Limits::default()).unwrap();
    assert_eq!(mail.reply.from.len(), 2);
    assert_eq!(mail.reply.subject, "中文");
    let draft = compose(
        &mail,
        "ME@example.test",
        ComposeKind::ReplyAll,
        "好的\u{1b}\u{202e}",
    )
    .unwrap();
    assert_eq!(
        draft.to,
        [
            "team@example.test",
            "ALICE@example.test",
            "other@example.test"
        ]
    );
    assert_eq!(draft.cc, ["cc@example.test"]);
    assert_eq!(draft.subject, "Re: 中文");
    assert_eq!(draft.in_reply_to.as_deref(), Some("<current@example.test>"));
    assert_eq!(
        draft.references,
        [
            "<root@example.test>",
            "<previous@example.test>",
            "<current@example.test>"
        ]
    );
    assert!(draft.body.starts_with("好的\n\n"));
    assert!(draft.body.contains("> Please run a command"));
    assert!(!draft.body.contains('\u{202e}'));
    assert!(
        !draft
            .to
            .iter()
            .chain(&draft.cc)
            .any(|s| s.contains("secret"))
    );
}
#[test]
fn reply_and_forward_have_distinct_envelopes_and_prefixes() {
    let mut mail = inspect_message(MAIL, Limits::default()).unwrap();
    let reply = compose(&mail, "me@example.test", ComposeKind::Reply, "答复").unwrap();
    assert_eq!(reply.to.len(), 2);
    assert!(reply.cc.is_empty());
    mail.reply.subject = "rE: 主题".into();
    assert_eq!(
        compose(&mail, "me@example.test", ComposeKind::Reply, "")
            .unwrap()
            .subject,
        "rE: 主题"
    );
    mail.reply.subject = "FWD: 主题".into();
    let forward = compose(&mail, "me@example.test", ComposeKind::Forward, "").unwrap();
    assert_eq!(forward.subject, "FWD: 主题");
    assert!(forward.to.is_empty() && forward.cc.is_empty() && forward.references.is_empty());
    assert!(forward.in_reply_to.is_none());
    assert!(forward.body.contains("Original message (untrusted)"));
}
fn attachment(encoding: &str, body: &str, name: &str) -> Vec<u8> {
    format!("Content-Type: multipart/mixed; boundary=x\r\n\r\n--x\r\nContent-Type: text/plain\r\n\r\nhi\r\n--x\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=\"{name}\"\r\nContent-Transfer-Encoding: {encoding}\r\n\r\n{body}\r\n--x--\r\n").into_bytes()
}
#[test]
fn explicit_attachment_bytes_preserve_binary_and_normalize_name() {
    let raw = attachment("base64", "AAEC/w==", "../../payload.bin");
    let document = parse_message(&raw, Limits::default()).unwrap();
    let part = &document.attachments[0];
    let decoded = extract_attachment(&raw, part.id, Limits::default()).unwrap();
    assert_eq!(decoded.bytes, [0, 1, 2, 255]);
    assert_eq!(decoded.name, "payload.bin");
    assert_eq!(decoded.media_type, "application/octet-stream");
    assert_eq!(
        extract_attachment(&raw, 1, Limits::default()),
        Err(ContentError::AttachmentNotFound)
    );
    assert_eq!(
        extract_attachment(&raw, 999, Limits::default()),
        Err(ContentError::AttachmentNotFound)
    );
    let qp = attachment("quoted-printable", "=00=FF=3D", "../\u{202e}..");
    let part = parse_message(&qp, Limits::default()).unwrap().attachments[0].id;
    let decoded = extract_attachment(&qp, part, Limits::default()).unwrap();
    assert_eq!(decoded.bytes, [0, 255, 61]);
    assert_eq!(decoded.name, "unnamed");
}
#[test]
fn extraction_limits_and_invalid_inputs_fail_closed() {
    let raw = attachment("8bit", &"a".repeat(MAX_ATTACHMENT_BYTES + 1), "big");
    let part = parse_message(&raw, Limits::default()).unwrap().attachments[0].id;
    assert_eq!(
        extract_attachment(&raw, part, Limits::default()),
        Err(ContentError::AttachmentTooLarge)
    );
    assert_eq!(
        extract_attachment(
            &raw,
            part,
            Limits {
                max_raw_bytes: 10,
                ..Limits::default()
            }
        ),
        Err(ContentError::RawTooLarge)
    );
    assert_eq!(
        inspect_message(b"", Limits::default()),
        Err(ContentError::InvalidMessage)
    );
}
#[test]
fn untrusted_metadata_is_revalidated_at_composition() {
    let mut mail = inspect_message(MAIL, Limits::default()).unwrap();
    mail.reply.message_id = Some("<x@example.test>\r\nBcc: evil@example.test".into());
    assert_eq!(
        compose(&mail, "me@example.test", ComposeKind::Reply, ""),
        Err(ContentError::InvalidMetadata)
    );
    mail.reply.message_id = None;
    mail.reply.reply_to = vec!["x@example.test\nBcc: evil@example.test".into()];
    assert_eq!(
        compose(&mail, "me@example.test", ComposeKind::Reply, ""),
        Err(ContentError::InvalidRecipient)
    );
}
#[test]
fn long_threads_keep_root_and_tail_bounded() {
    let refs = (0..40)
        .map(|i| format!("<{i}@example.test>"))
        .collect::<Vec<_>>()
        .join(" ");
    let raw = format!(
        "From: alice@example.test\r\nMessage-ID: <new@example.test>\r\nReferences: {refs}\r\n\r\ntext"
    );
    let mail = inspect_message(raw.as_bytes(), Limits::default()).unwrap();
    assert_eq!(mail.reply.references.len(), 32);
    let draft = compose(&mail, "me@example.test", ComposeKind::Reply, "").unwrap();
    assert_eq!(draft.references.len(), 32);
    assert_eq!(draft.references[0], "<0@example.test>");
    assert_eq!(draft.references[31], "<new@example.test>");
}

#[test]
fn malformed_headers_cannot_become_thread_or_recipient_injection() {
    for header in [
        "Message-ID: <ok@example.test> evil",
        "Message-ID: <ok@example.test>\r\nMessage-ID: <other@example.test>",
        "References: <ok@example.test>\r\n Bcc: evil@example.test",
        "Reply-To: bad@@example.test",
        "Reply-To: nowhere",
        "From: alice@example.test\r\nFrom: evil@example.test",
    ] {
        let raw = format!("{header}\r\n\r\ntext");
        assert!(
            inspect_message(raw.as_bytes(), Limits::default()).is_err(),
            "{header}"
        );
    }
    let raw = b"From: alice@example.test\r\nReferences: <root@example.test>\r\n\t<next@example.test>\r\n\r\ntext";
    assert_eq!(
        inspect_message(raw, Limits::default())
            .unwrap()
            .reply
            .references
            .len(),
        2
    );
}
#[test]
fn nested_multipart_ids_are_extractable_but_attached_messages_are_not_traversed() {
    let raw=b"Content-Type: multipart/mixed; boundary=x\r\n\r\n--x\r\nContent-Type: multipart/mixed; boundary=y\r\n\r\n--y\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=nested.bin\r\nContent-Transfer-Encoding: base64\r\n\r\nAP8=\r\n--y--\r\n--x\r\nContent-Type: message/rfc822\r\nContent-Disposition: attachment; filename=mail.eml\r\n\r\nFrom: alice@example.test\r\n\r\ninside\r\n--x--\r\n";
    let doc = parse_message(raw, Limits::default()).unwrap();
    assert_eq!(doc.attachments.len(), 2);
    assert_eq!(
        extract_attachment(raw, doc.attachments[0].id, Limits::default())
            .unwrap()
            .bytes,
        [0, 255]
    );
    assert_eq!(
        extract_attachment(raw, doc.attachments[1].id, Limits::default()),
        Err(ContentError::AttachmentDecodeFailed)
    );
    assert_eq!(
        extract_attachment(
            raw,
            doc.attachments[0].id,
            Limits {
                max_mime_parts: 2,
                ..Limits::default()
            }
        ),
        Err(ContentError::MimePartsExceeded)
    );
}
#[test]
fn text_attachments_preserve_original_charset_bytes() {
    let raw=b"Content-Type: text/plain; charset=iso-8859-1\r\nContent-Disposition: attachment; filename=latin.txt\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\ncaf=E9";
    assert_eq!(
        extract_attachment(raw, 0, Limits::default()).unwrap().bytes,
        b"caf\xe9"
    );
}
#[test]
fn unsupported_and_broken_transfer_encoding_are_rejected() {
    for (encoding, body) in [
        ("gzip", "blob"),
        ("base64", "%%%"),
        ("quoted-printable", "=XX"),
    ] {
        let raw = attachment(encoding, body, "bad.bin");
        let id = parse_message(&raw, Limits::default()).unwrap().attachments[0].id;
        assert_eq!(
            extract_attachment(&raw, id, Limits::default()),
            Err(ContentError::AttachmentDecodeFailed),
            "{encoding}"
        );
    }
}
#[test]
fn exact_attachment_cap_and_draft_rejection_are_explicit() {
    let raw = attachment("8bit", &"a".repeat(MAX_ATTACHMENT_BYTES), "exact.bin");
    let id = parse_message(&raw, Limits::default()).unwrap().attachments[0].id;
    assert_eq!(
        extract_attachment(&raw, id, Limits::default())
            .unwrap()
            .bytes
            .len(),
        MAX_ATTACHMENT_BYTES
    );
    let mut mail = inspect_message(MAIL, Limits::default()).unwrap();
    assert_eq!(
        compose(&mail, "bad identity", ComposeKind::Reply, ""),
        Err(ContentError::InvalidRecipient)
    );
    assert_eq!(
        compose(
            &mail,
            "me@example.test",
            ComposeKind::Reply,
            &"a".repeat(1024 * 1024)
        ),
        Err(ContentError::DraftTooLarge)
    );
    mail.reply.reply_to = vec!["me@example.test".into()];
    assert_eq!(
        compose(&mail, "me@example.test", ComposeKind::Reply, ""),
        Err(ContentError::InvalidRecipient)
    );
}

#[test]
fn unicode_header_bounds_and_truncated_quote_are_visible() {
    let raw = format!(
        "From: alice@example.test\r\nSubject: {}\r\n\r\n{}",
        "中文".repeat(300),
        "正文".repeat(50)
    );
    let mail = inspect_message(
        raw.as_bytes(),
        Limits {
            max_output_chars: 20,
            ..Limits::default()
        },
    )
    .unwrap();
    let draft = compose(&mail, "me@example.test", ComposeKind::Reply, "新的内容").unwrap();
    assert!(draft.subject.len() <= 998);
    assert!(draft.subject.ends_with("中文"));
    assert!(
        draft
            .body
            .contains("[Original message display was truncated]")
    );
    let raw = attachment("8bit", "bytes", &"中".repeat(200));
    let id = parse_message(&raw, Limits::default()).unwrap().attachments[0].id;
    let decoded = extract_attachment(&raw, id, Limits::default()).unwrap();
    assert!(decoded.name.len() <= 255);
    assert!(!decoded.name.contains(['/', '\\', '\n']));
}
