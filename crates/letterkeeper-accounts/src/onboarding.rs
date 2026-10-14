// SPDX-License-Identifier: GPL-3.0-only
//! Offline, user-owned setup inputs. This module never authenticates or opens storage.
use crate::{
    AccountConfig, AccountError, Authentication, OperationPermissions, SentCopyStrategy,
    TlsEndpoint,
};
use std::{collections::HashSet, net::IpAddr};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OnboardingScope {
    #[default]
    ReadOnly,
    Draft,
    Send,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnboardingEndpoint {
    pub host: String,
    pub port: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnboardingProvider {
    Password {
        imap: OnboardingEndpoint,
        smtp: OnboardingEndpoint,
        sent_copy: SentCopyStrategy,
    },
    Gmail {
        client_id: String,
        client_secret_ref: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnboardingInput {
    pub id: String,
    pub email: String,
    pub username: Option<String>,
    pub allowed_folders: Vec<String>,
    pub scope: OnboardingScope,
    pub drafts_folder: Option<String>,
    pub sent_folder: Option<String>,
    pub provider: OnboardingProvider,
}

impl OnboardingInput {
    /// Builds configuration without storage, credential, DNS or protocol operations.
    /// Permission expansion is possible only through the caller's explicit scope choice.
    pub fn build(self) -> Result<AccountConfig, AccountError> {
        if !valid_email(&self.email)
            || self.allowed_folders.is_empty()
            || self.allowed_folders.len() > 128
            || self.allowed_folders.iter().any(|f| !safe_field(f, 512))
            || self.allowed_folders.iter().collect::<HashSet<_>>().len()
                != self.allowed_folders.len()
        {
            return Err(AccountError::InvalidConfiguration);
        }
        let username = self.username.unwrap_or_else(|| self.email.clone());
        if !safe_field(&username, 320) {
            return Err(AccountError::InvalidConfiguration);
        }
        let (imap, smtp, sent_copy, authentication, default_drafts, default_sent) =
            match self.provider {
                OnboardingProvider::Password {
                    imap,
                    smtp,
                    sent_copy,
                } => (
                    endpoint(imap, 993)?,
                    endpoint(smtp, 465)?,
                    sent_copy,
                    Authentication::Password,
                    "Drafts",
                    "Sent",
                ),
                OnboardingProvider::Gmail {
                    client_id,
                    client_secret_ref,
                } => {
                    if !safe_field(&client_id, 512) || client_id.chars().any(char::is_whitespace) {
                        return Err(AccountError::InvalidConfiguration);
                    }
                    (
                        TlsEndpoint {
                            host: "imap.gmail.com".into(),
                            port: 993,
                        },
                        TlsEndpoint {
                            host: "smtp.gmail.com".into(),
                            port: 465,
                        },
                        SentCopyStrategy::ServerManaged,
                        Authentication::GmailOAuth {
                            client_id,
                            client_secret_ref,
                        },
                        "[Gmail]/Drafts",
                        "[Gmail]/Sent Mail",
                    )
                }
            };
        let drafts_folder = self.drafts_folder.unwrap_or_else(|| default_drafts.into());
        let sent_folder = self.sent_folder.unwrap_or_else(|| default_sent.into());
        if !safe_field(&drafts_folder, 512) || !safe_field(&sent_folder, 512) {
            return Err(AccountError::InvalidConfiguration);
        }
        let permissions = OperationPermissions {
            read: true,
            draft: matches!(self.scope, OnboardingScope::Draft | OnboardingScope::Send),
            send: self.scope == OnboardingScope::Send,
        };
        let mut allowed_folders = self.allowed_folders;
        for (enabled, folder) in [
            (permissions.draft, &drafts_folder),
            (permissions.send, &sent_folder),
        ] {
            if enabled && !allowed_folders.contains(folder) {
                allowed_folders.push(folder.clone());
            }
        }
        let config = AccountConfig {
            id: self.id,
            email: self.email,
            username,
            imap,
            smtp,
            allowed_folders,
            permissions,
            drafts_folder,
            sent_folder,
            sent_copy,
            authentication,
            extra_ca_pem: None,
        };
        config.validate()?;
        Ok(config)
    }
}

fn safe_field(value: &str, max: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= max
        && !value.chars().any(|c| {
            c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
}

fn valid_dns_name(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

fn endpoint(input: OnboardingEndpoint, default_port: u16) -> Result<TlsEndpoint, AccountError> {
    let port = input.port.unwrap_or(default_port);
    if port == 0
        || !safe_field(&input.host, 253)
        || !(input.host.parse::<IpAddr>().is_ok() || valid_dns_name(&input.host))
    {
        return Err(AccountError::InvalidConfiguration);
    }
    Ok(TlsEndpoint {
        host: input.host,
        port,
    })
}

// Setup accepts unambiguous ASCII dot-atom mailboxes. Quoted/localized mailbox
// syntax requires a separate design; Unicode IMAP folder names remain supported.
fn valid_email(email: &str) -> bool {
    if !safe_field(email, 320) {
        return false;
    }
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && local.len() <= 64
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !local.contains("..")
        && local
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".!#$%&'*+-/=?^_`{|}~".contains(&b))
        && valid_dns_name(domain)
}
