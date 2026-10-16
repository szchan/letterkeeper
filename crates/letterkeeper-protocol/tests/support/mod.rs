// SPDX-License-Identifier: GPL-3.0-only
//! Client-specific conversions stay in this crate's tests.
use letterkeeper_protocol::{Credentials, Endpoint, SmtpStage, TlsOptions};
use letterkeeper_test_support::mail;
pub use mail::{MESSAGE, PageMode, Scenario, page_uid};

pub struct Server {
    pub endpoint: Endpoint,
    pub tls: TlsOptions,
    inner: mail::Server,
}
impl Server {
    pub fn start_shared() -> Self {
        Self::start(Scenario::Mailbox)
    }
    pub fn start(scenario: Scenario) -> Self {
        Self::with_name(scenario, "localhost")
    }
    pub fn with_name(scenario: Scenario, name: &str) -> Self {
        let inner = mail::Server::with_name(scenario, name);
        Self {
            endpoint: Endpoint {
                host: inner.endpoint.host.clone(),
                port: inner.endpoint.port,
            },
            tls: TlsOptions {
                roots_pem: inner.tls.roots_pem.clone(),
            },
            inner,
        }
    }
    pub fn credentials() -> Credentials {
        Credentials {
            username: "person@example.test".into(),
            secret: "synthetic-secret".into(),
            oauth: false,
        }
    }
    pub fn finish(self) -> Vec<String> {
        self.inner.finish()
    }
}
pub fn smtp_stage(stage: SmtpStage) -> mail::SmtpStage {
    match stage {
        SmtpStage::Preparation => mail::SmtpStage::Preparation,
        SmtpStage::TlsConfiguration => mail::SmtpStage::TlsConfiguration,
        SmtpStage::Connection => mail::SmtpStage::Connection,
        SmtpStage::TlsHandshake => mail::SmtpStage::TlsHandshake,
        SmtpStage::Greeting => mail::SmtpStage::Greeting,
        SmtpStage::Authentication => mail::SmtpStage::Authentication,
        SmtpStage::MailFrom => mail::SmtpStage::MailFrom,
        SmtpStage::Recipient => mail::SmtpStage::Recipient,
        SmtpStage::DataCommand => mail::SmtpStage::DataCommand,
        SmtpStage::Submission => mail::SmtpStage::Submission,
    }
}
