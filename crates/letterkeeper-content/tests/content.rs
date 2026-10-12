// SPDX-License-Identifier: GPL-3.0-only
use letterkeeper_content::{ContentError, Limits, parse_message, sanitize_text};

fn eml(kind: &str, body: &str) -> Vec<u8> {
    format!("MIME-Version: 1.0\r\nContent-Type: {kind}; charset=utf-8\r\n\r\n{body}").into_bytes()
}
fn html(body: &str) -> letterkeeper_content::SafeDocument {
    parse_message(&eml("text/html", body), Limits::default()).unwrap()
}
#[test]
fn decoded_chinese_subject_sender_and_body() {
    let raw = "Subject: =?UTF-8?B?5Lit5paH?=\r\nFrom: =?UTF-8?B?5Lit5paH?= <fiction@example.invalid>\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n你好世界";
    let doc = parse_message(raw.as_bytes(), Limits::default()).unwrap();
    assert_eq!(doc.subject, "中文");
    assert_eq!(doc.from, "中文 <fiction@example.invalid>");
    assert_eq!(doc.text, "你好世界");
}
#[test]
fn legacy_charset_and_quoted_printable() {
    let raw = b"Content-Type: text/plain; charset=iso-8859-1\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\ncaf=E9";
    assert_eq!(parse_message(raw, Limits::default()).unwrap().text, "café");
}
#[test]
fn multipart_alternative_prefers_plain_without_html_links() {
    let raw = b"Content-Type: multipart/alternative; boundary=a\r\n\r\n--a\r\nContent-Type: text/html\r\n\r\n<a href='https://example.invalid'>HTML</a>\r\n--a\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nplain preferred\r\n--a--\r\n";
    let doc = parse_message(raw, Limits::default()).unwrap();
    assert_eq!(doc.text.trim(), "plain preferred");
    assert!(doc.links.is_empty());
}
#[test]
fn nested_alternative_keeps_only_plain_from_selected_branch() {
    let raw = b"Content-Type: multipart/alternative; boundary=outer\r\n\r\n--outer\r\nContent-Type: multipart/mixed; boundary=inner\r\n\r\n--inner\r\nContent-Type: text/plain\r\n\r\nplain preferred\r\n--inner\r\nContent-Type: text/html\r\n\r\n<a href='https://tracker.invalid'>html duplicate</a>\r\n--inner--\r\n--outer\r\nContent-Type: text/html\r\n\r\n<a href='https://tracker.invalid'>html fallback</a>\r\n--outer--\r\n";
    let doc = parse_message(raw, Limits::default()).unwrap();
    assert_eq!(doc.text, "plain preferred");
    assert!(doc.links.is_empty());
}
#[test]
fn alternative_skips_attachment_when_html_is_only_display_body() {
    let raw = b"Content-Type: multipart/alternative; boundary=a\r\n\r\n--a\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=x.bin\r\n\r\nbytes\r\n--a\r\nContent-Type: text/html\r\n\r\n<p>fallback</p>\r\n--a--\r\n";
    let doc = parse_message(raw, Limits::default()).unwrap();
    assert_eq!(doc.text, "fallback");
}
#[test]
fn alternative_skips_unsupported_text_when_html_is_available() {
    let raw = b"Content-Type: multipart/alternative; boundary=a\r\n\r\n--a\r\nContent-Type: text/calendar\r\n\r\nBEGIN:VCALENDAR\r\n--a\r\nContent-Type: text/html\r\n\r\n<p>fallback</p>\r\n--a--\r\n";
    let doc = parse_message(raw, Limits::default()).unwrap();
    assert_eq!(doc.text, "fallback");
}
#[test]
fn html_entities_blocks_lists_and_tables_are_text() {
    let doc = html(
        "<p>你好 &amp; &#x4e16;&#30028;</p><ul><li>one</li><li>two</li></ul><table><tr><td>A</td><td>B</td></tr></table>",
    );
    assert!(doc.text.contains("你好 & 世界"));
    assert!(doc.text.contains("- one\n- two"));
    assert!(doc.text.contains("A | B"));
    assert!(!doc.text.contains("<p>"));
}
#[test]
fn active_subtrees_and_resource_attributes_are_discarded() {
    let doc = html(
        "<link rel='dns-prefetch' href='//tracker.invalid'><meta http-equiv='refresh' content='0;url=https://tracker.invalid'><style>@import 'https://tracker.invalid';SECRET_STYLE</style><script src='https://tracker.invalid'>SECRET_SCRIPT</script><svg><text>SECRET_SVG</text></svg><iframe>SECRET_FRAME</iframe><object>SECRET_OBJECT</object><embed src='https://tracker.invalid'><form>SECRET_FORM<input></form><p onclick='evil()' style='background:url(https://tracker.invalid)'>visible</p>",
    );
    assert_eq!(doc.text, "visible");
    assert!(doc.links.is_empty());
}
#[test]
fn images_have_remote_cid_and_local_placeholders_no_sources() {
    let doc = html(
        "<img src='https://tracker.invalid/pixel' alt='remote'><img src='cid:inline' alt='inline'><img src='file:///etc/passwd'><img srcset='https://tracker.invalid/2x 2x'>",
    );
    assert!(doc.text.contains("[remote image blocked]"));
    assert!(doc.text.contains("[CID image omitted]"));
    assert!(doc.text.contains("[image omitted]"));
    assert!(!doc.text.contains("tracker.invalid"));
    assert!(!doc.text.contains("passwd"));
}
#[test]
fn numbered_links_reveal_actual_target_without_fetching() {
    let doc = html(
        "<a href='https://actual.example.invalid/a?x=1&amp;y=2'>claimed</a> <a href='mailto:fiction@example.invalid'>mail</a>",
    );
    assert!(doc.text.contains("claimed [1]"));
    assert_eq!(doc.links[0].id, 1);
    assert_eq!(doc.links[0].label, "claimed");
    assert_eq!(
        doc.links[0].target,
        "https://actual.example.invalid/a?x=1&y=2"
    );
    assert_eq!(doc.links[1].id, 2);
}
#[test]
fn unsafe_and_obfuscated_links_are_not_retained() {
    for target in [
        "javascript:alert(1)",
        "data:text/html,evil",
        "file:///etc/passwd",
        "//tracker.invalid",
        "relative/path",
        "java&#x09;script:evil",
        "https://actual.invalid/&#x202e;evil",
        "https://trusted.invalid\\@attacker.invalid/path",
        "https://trusted.invalid@attacker.invalid/path",
        "https://%61ttacker.invalid/path",
        "https:///missing-host",
    ] {
        let doc = html(&format!("<a href='{target}'>label</a>"));
        assert!(doc.links.is_empty(), "{target}");
        assert_eq!(doc.text, "label");
    }
}
#[test]
fn attachment_metadata_only_and_filename_cannot_be_path() {
    let raw = b"Content-Type: multipart/mixed; boundary=b\r\n\r\n--b\r\nContent-Type: text/plain\r\n\r\nhello\r\n--b\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=\"../../secret.exe\"\r\nContent-Transfer-Encoding: base64\r\n\r\nZXZpbA==\r\n--b--\r\n";
    let doc = parse_message(raw, Limits::default()).unwrap();
    assert_eq!(doc.text.trim(), "hello");
    assert_eq!(doc.attachments.len(), 1);
    assert_eq!(doc.attachments[0].name, "secret.exe");
    assert_eq!(doc.attachments[0].media_type, "application/octet-stream");
    assert_eq!(doc.attachments[0].size, 4);
    assert!(!doc.text.contains("evil"));
}
#[test]
fn attachment_metadata_uses_shared_budget_without_partial_entries() {
    let raw = b"Content-Type: multipart/mixed; boundary=b\r\n\r\n--b\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=a.bin\r\n\r\na\r\n--b\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=b.bin\r\n\r\nb\r\n--b--\r\n";
    let doc = parse_message(
        raw,
        Limits {
            max_output_chars: 40,
            ..Limits::default()
        },
    )
    .unwrap();
    assert_eq!(doc.attachments.len(), 1);
    assert_eq!(doc.attachments[0].name, "a.bin");
    assert_eq!(doc.attachments[0].media_type, "application/octet-stream");
    assert!(doc.truncated);
}
#[test]
fn control_and_bidi_removed_but_chinese_newline_and_tab_survive() {
    assert_eq!(
        sanitize_text("你\x1b[31m好\x07\u{85}\u{202e}\u{2066}\u{2069}\n\t世界\r\n"),
        "你[31m好\n\t世界\n"
    );
}
#[test]
fn all_control_codepoints_are_removed_and_sanitizing_is_idempotent() {
    let input: String = (0..=0x10ffff)
        .filter_map(char::from_u32)
        .filter(|c| c.is_control() || matches!(*c as u32, 0x202a..=0x202e | 0x2066..=0x2069))
        .collect();
    let output = sanitize_text(&input);
    assert!(
        output
            .chars()
            .all(|c| !c.is_control() || c == '\n' || c == '\t')
    );
    assert_eq!(sanitize_text(&output), output);
}
#[test]
fn raw_limit_checked_before_parsing_and_empty_rejected() {
    assert_eq!(
        parse_message(
            &[0; 11],
            Limits {
                max_raw_bytes: 10,
                ..Limits::default()
            }
        ),
        Err(ContentError::RawTooLarge)
    );
    assert_eq!(
        parse_message(b"", Limits::default()),
        Err(ContentError::InvalidMessage)
    );
}
#[test]
fn zero_limits_are_invalid() {
    for limits in [
        Limits {
            max_raw_bytes: 0,
            ..Limits::default()
        },
        Limits {
            max_output_chars: 0,
            ..Limits::default()
        },
        Limits {
            max_mime_parts: 0,
            ..Limits::default()
        },
        Limits {
            max_html_depth: 0,
            ..Limits::default()
        },
    ] {
        assert_eq!(
            parse_message(&eml("text/plain", "x"), limits),
            Err(ContentError::InvalidLimits)
        );
    }
}
#[test]
fn mime_part_limit_includes_containers_and_nested_messages() {
    let raw = b"Content-Type: multipart/mixed; boundary=a\r\n\r\n--a\r\nContent-Type: message/rfc822\r\n\r\nSubject: nested\r\nContent-Type: text/plain\r\n\r\nnested\r\n--a--\r\n";
    assert_eq!(
        parse_message(
            raw,
            Limits {
                max_mime_parts: 2,
                ..Limits::default()
            }
        ),
        Err(ContentError::MimePartsExceeded)
    );
}
#[test]
fn html_depth_limit_rejects_instead_of_rendering_partial_tree() {
    let raw = eml(
        "text/html",
        &format!("{}hello{}", "<div>".repeat(80), "</div>".repeat(80)),
    );
    assert_eq!(
        parse_message(&raw, Limits::default()),
        Err(ContentError::HtmlDepthExceeded)
    );
}
#[test]
fn html_depth_limit_counts_discarded_template_contents() {
    for prefix in ["<template>", "<template><template>"] {
        let suffix = "</template>".repeat(prefix.matches("<template>").count());
        let body = format!(
            "{prefix}{}hidden{}{}<p>visible</p>",
            "<div>".repeat(80),
            "</div>".repeat(80),
            suffix
        );
        assert_eq!(
            parse_message(&eml("text/html", &body), Limits::default()),
            Err(ContentError::HtmlDepthExceeded)
        );
    }
}
#[test]
fn attachment_name_is_normalized_after_control_removal() {
    for filename in ["utf-8''%2E%1B%2E", "utf-8''%1B"] {
        let raw = format!(
            "Content-Type: multipart/mixed; boundary=b\r\n\r\n--b\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename*={filename}\r\n\r\nx\r\n--b--\r\n"
        );
        let doc = parse_message(raw.as_bytes(), Limits::default()).unwrap();
        assert_eq!(doc.attachments[0].name, "unnamed", "{filename}");
    }
    let raw = "Content-Type: multipart/mixed; boundary=b\r\n\r\n--b\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename*=utf-8''%E4%B8%AD%E6%96%87.txt\r\n\r\nx\r\n--b--\r\n";
    let doc = parse_message(raw.as_bytes(), Limits::default()).unwrap();
    assert_eq!(doc.attachments[0].name, "中文.txt");
}
#[test]
fn output_truncates_on_unicode_boundaries_with_explicit_warning() {
    let doc = parse_message(
        &eml("text/plain", "你好世界你好世界"),
        Limits {
            max_output_chars: 3,
            ..Limits::default()
        },
    )
    .unwrap();
    assert_eq!(doc.text, "你好世");
    assert!(doc.truncated);
    assert!(!doc.warnings.is_empty());
}
#[test]
fn document_display_fields_share_total_character_budget() {
    let raw = format!(
        "Subject: {}\r\nFrom: fictional@example.invalid\r\nContent-Type: text/html\r\n\r\n<a href='https://example.invalid'>{}</a>",
        "s".repeat(100),
        "字".repeat(100)
    );
    let doc = parse_message(
        raw.as_bytes(),
        Limits {
            max_output_chars: 40,
            ..Limits::default()
        },
    )
    .unwrap();
    let total = doc.subject.chars().count()
        + doc.from.chars().count()
        + doc.text.chars().count()
        + doc
            .links
            .iter()
            .map(|x| x.label.chars().count() + x.target.chars().count())
            .sum::<usize>()
        + doc
            .attachments
            .iter()
            .map(|x| x.name.chars().count() + x.media_type.chars().count())
            .sum::<usize>();
    assert!(total <= 40);
    assert!(doc.truncated);
}
#[test]
fn malformed_html_still_has_no_active_subtrees() {
    let doc = html(
        "<p>safe<a href='javascript:evil'>label<p><script>evil</script><!-- secret --><svg/onload=evil>bad</svg>",
    );
    assert!(doc.text.contains("safe"));
    assert!(doc.links.is_empty());
    assert!(!doc.text.contains("evil"));
    assert!(!doc.text.contains("bad"));
}
