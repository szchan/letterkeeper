// SPDX-License-Identifier: GPL-3.0-only
use crate::runtime::run;
use crate::tls::connect_with_progress;
use crate::validation::{credentials_valid, valid};
use crate::wire::{MAX_RAW, State};
use crate::{
    Credentials, Endpoint, ProtocolError, SmtpDiagnostic, SmtpFailureKind, SmtpReport, SmtpStage,
    TlsOptions, WriteOutcome,
};
use std::sync::{Arc, atomic::Ordering};
// Only fixed enum discriminants are stored. The scoped runtime bridge joins
// before reading this value, and never holds a lock across network awaits.
pub(crate) struct SmtpProgress(std::sync::atomic::AtomicUsize);
impl SmtpProgress {
    fn new() -> Self {
        Self(std::sync::atomic::AtomicUsize::new(
            SmtpStage::Preparation as usize,
        ))
    }
    pub(crate) fn set(&self, stage: SmtpStage) {
        self.0.store(stage as usize, Ordering::Relaxed);
    }
    fn stage(&self) -> SmtpStage {
        const STAGES: [SmtpStage; 10] = [
            SmtpStage::Preparation,
            SmtpStage::TlsConfiguration,
            SmtpStage::Connection,
            SmtpStage::TlsHandshake,
            SmtpStage::Greeting,
            SmtpStage::Authentication,
            SmtpStage::MailFrom,
            SmtpStage::Recipient,
            SmtpStage::DataCommand,
            SmtpStage::Submission,
        ];
        STAGES[self.0.load(Ordering::Relaxed)]
    }
    fn diagnostic(&self, kind: SmtpFailureKind, smtp_code: Option<u16>) -> SmtpDiagnostic {
        SmtpDiagnostic {
            stage: self.stage(),
            kind,
            smtp_code: smtp_code.filter(|code| (100..600).contains(code)),
        }
    }
    fn protocol_error(&self, error: ProtocolError) -> SmtpDiagnostic {
        self.diagnostic(
            match error {
                ProtocolError::InvalidInput => SmtpFailureKind::InvalidInput,
                ProtocolError::Timeout => SmtpFailureKind::Timeout,
                ProtocolError::Connection => SmtpFailureKind::Connection,
                _ => SmtpFailureKind::UnexpectedResponse,
            },
            None,
        )
    }
    fn smtp_error(&self, error: lettre::transport::smtp::Error) -> SmtpDiagnostic {
        let kind = if error.is_timeout() {
            SmtpFailureKind::Timeout
        } else if error.is_transient() || error.is_permanent() {
            SmtpFailureKind::Rejected
        } else if error.is_response() || error.is_client() {
            SmtpFailureKind::UnexpectedResponse
        } else {
            SmtpFailureKind::Connection
        };
        // status() is a typed Code; never inspect Display/source/server text.
        self.diagnostic(kind, error.status().map(u16::from))
    }
}
/// One strict implicit-TLS attempt with fixed, non-sensitive diagnostic metadata.
/// Greeting includes EHLO; Submission includes content transfer and final reply.
/// Callers must enforce authorization before invoking. No retries are performed.
pub fn send_smtp_report(
    endpoint: &Endpoint,
    tls: &TlsOptions,
    credentials: &Credentials,
    from: &str,
    to: &[String],
    raw: &[u8],
) -> SmtpReport {
    use lettre::transport::smtp::{
        authentication::{Credentials as SmtpCredentials, Mechanism},
        client::AsyncSmtpConnection,
        commands::{Data, Mail, Rcpt},
        extension::{ClientId, Extension, MailBodyParameter, MailParameter},
    };
    let progress = SmtpProgress::new();
    let invalid = || SmtpReport {
        outcome: WriteOutcome::Failed,
        diagnostic: Some(progress.diagnostic(SmtpFailureKind::InvalidInput, None)),
    };
    if !credentials_valid(credentials)
        || raw.is_empty()
        || raw.len() > MAX_RAW
        || to.is_empty()
        || to.len() > 100
        || !valid(from, 320)
        || to.iter().any(|s| !valid(s, 320))
        || !valid(&endpoint.host, 253)
        || endpoint.port == 0
    {
        return invalid();
    }
    let Ok(sender) = from.parse::<lettre::Address>() else {
        return invalid();
    };
    let Ok(recipients) = to
        .iter()
        .map(|s| s.parse::<lettre::Address>())
        .collect::<Result<Vec<_>, _>>()
    else {
        return invalid();
    };
    let state = Arc::new(State::default());
    let operation = async {
        let wire = connect_with_progress(endpoint, tls, state, false, Some(&progress))
            .await
            .map_err(|error| progress.protocol_error(error))?;
        progress.set(SmtpStage::Greeting);
        let mut s = AsyncSmtpConnection::connect_with_transport(
            Box::new(wire),
            &ClientId::Domain("letterkeeper.invalid".into()),
        )
        .await
        .map_err(|error| progress.smtp_error(error))?;
        progress.set(SmtpStage::Authentication);
        let response = s
            .auth(
                &[if credentials.oauth {
                    Mechanism::Xoauth2
                } else {
                    Mechanism::Plain
                }],
                &SmtpCredentials::new(credentials.username.clone(), credentials.secret.clone()),
            )
            .await
            .map_err(|error| progress.smtp_error(error))?;
        if !response.has_code(235) {
            return Err(progress.diagnostic(
                SmtpFailureKind::UnexpectedResponse,
                Some(response.code().into()),
            ));
        }
        progress.set(SmtpStage::MailFrom);
        let mut options = Vec::new();
        if !raw.is_ascii() {
            if !s.server_info().supports_feature(Extension::EightBitMime) {
                return Err(progress.diagnostic(SmtpFailureKind::Rejected, None));
            }
            options.push(MailParameter::Body(MailBodyParameter::EightBitMime));
        }
        if !from.is_ascii() || to.iter().any(|s| !s.is_ascii()) {
            if !s.server_info().supports_feature(Extension::SmtpUtfEight) {
                return Err(progress.diagnostic(SmtpFailureKind::Rejected, None));
            }
            options.push(MailParameter::SmtpUtfEight);
        }
        s.command(Mail::new(Some(sender), options))
            .await
            .map_err(|error| progress.smtp_error(error))?;
        progress.set(SmtpStage::Recipient);
        for recipient in recipients {
            s.command(Rcpt::new(recipient, vec![]))
                .await
                .map_err(|error| progress.smtp_error(error))?;
        }
        progress.set(SmtpStage::DataCommand);
        let response = s
            .command(Data)
            .await
            .map_err(|error| progress.smtp_error(error))?;
        if !response.has_code(354) {
            return Err(progress.diagnostic(
                SmtpFailureKind::UnexpectedResponse,
                Some(response.code().into()),
            ));
        }
        // Preserve the existing conservative boundary: once 354 is received,
        // all non-rejection failures are Uncertain, even during content writes.
        progress.set(SmtpStage::Submission);
        let response = s
            .message(raw)
            .await
            .map_err(|error| progress.smtp_error(error))?;
        if response.has_code(250) {
            Ok(WriteOutcome::Accepted)
        } else {
            Err(progress.diagnostic(
                SmtpFailureKind::UnexpectedResponse,
                Some(response.code().into()),
            ))
        }
    };
    // Keep the same runtime bridge and single deadline as IMAP. The inner
    // Result preserves typed SMTP failures; the outer Result captures timeout
    // or runtime failure while the atomic progress remains available.
    let result = match run(async { Ok(operation.await) }) {
        Ok(result) => result,
        Err(error) => Err(progress.protocol_error(error)),
    };
    match result {
        Ok(outcome) => SmtpReport {
            outcome,
            diagnostic: None,
        },
        Err(diagnostic) => SmtpReport {
            outcome: if diagnostic.stage == SmtpStage::Submission
                && diagnostic.kind != SmtpFailureKind::Rejected
            {
                WriteOutcome::Uncertain
            } else {
                WriteOutcome::Failed
            },
            diagnostic: Some(diagnostic),
        },
    }
}
/// Compatibility API: the same strict single attempt and three-state outcome.
pub fn send_smtp(
    endpoint: &Endpoint,
    tls: &TlsOptions,
    credentials: &Credentials,
    from: &str,
    to: &[String],
    raw: &[u8],
) -> WriteOutcome {
    send_smtp_report(endpoint, tls, credentials, from, to, raw).outcome
}
