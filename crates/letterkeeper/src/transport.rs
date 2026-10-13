// SPDX-License-Identifier: GPL-3.0-only
//! Deliberately test-only SMTP adapter. No production account configuration yet.
use letterkeeper_core::{SendOutcome, Submission};
use lettre::{
    Message, SmtpTransport, Transport,
    message::{Attachment, MultiPart, SinglePart, header::ContentType},
};
use std::net::SocketAddr;
use std::time::Duration;

pub fn prepare(submission: &Submission) -> Result<Message, String> {
    prepare_snapshot(submission, false)
}
pub fn prepare_snapshot(submission: &Submission, keep_bcc: bool) -> Result<Message, String> {
    let input = &submission.input;
    let mut builder = Message::builder()
        .from(input.from.parse().map_err(|_| "invalid_sender")?)
        .subject(&input.subject)
        .message_id(Some(submission.message_id.clone()));
    if let Some(id) = &input.in_reply_to {
        builder = builder.in_reply_to(id.clone());
    }
    if !input.references.is_empty() {
        builder = builder.references(input.references.join(" "));
    }
    if keep_bcc {
        builder = builder.keep_bcc();
    }
    for address in &input.to {
        builder = builder.to(address.parse().map_err(|_| "invalid_recipient")?);
    }
    for address in &input.cc {
        builder = builder.cc(address.parse().map_err(|_| "invalid_recipient")?);
    }
    for address in &input.bcc {
        builder = builder.bcc(address.parse().map_err(|_| "invalid_recipient")?);
    }
    let text = SinglePart::builder()
        .header(ContentType::TEXT_PLAIN)
        .body(input.body.clone());
    if input.attachments.is_empty() {
        builder
            .singlepart(text)
            .map_err(|_| "invalid_message".into())
    } else {
        let mut multipart = MultiPart::mixed().singlepart(text);
        for item in &input.attachments {
            let content_type =
                ContentType::parse(&item.content_type).map_err(|_| "invalid_attachment_type")?;
            multipart = multipart.singlepart(
                Attachment::new(item.name.clone()).body(item.bytes.clone(), content_type),
            );
        }
        builder
            .multipart(multipart)
            .map_err(|_| "invalid_message".into())
    }
}

pub fn synthetic_address(value: &str) -> bool {
    value
        .parse::<lettre::Address>()
        .is_ok_and(|a| a.domain().ends_with(".test") || a.domain().ends_with(".invalid"))
}

/// One SMTP attempt, with an immutable authorized snapshot. Loopback and
/// synthetic recipients are checked before constructing any transport.
pub fn send_test(submission: &Submission, endpoint: SocketAddr) -> SendOutcome {
    let input = &submission.input;
    if !endpoint.ip().is_loopback()
        || !synthetic_address(&input.from)
        || !input
            .to
            .iter()
            .chain(&input.cc)
            .chain(&input.bcc)
            .all(|a| synthetic_address(a))
    {
        return SendOutcome::Failed;
    }
    let Ok(message) = prepare(submission) else {
        return SendOutcome::Failed;
    };
    let transport = SmtpTransport::builder_dangerous(endpoint.ip().to_string())
        .port(endpoint.port())
        .timeout(Some(Duration::from_secs(2)))
        .build();
    match transport.send(&message) {
        Ok(_) => SendOutcome::Accepted,
        Err(error) if error.is_permanent() || error.is_transient() => SendOutcome::Failed,
        Err(_) => SendOutcome::Uncertain,
    }
}
