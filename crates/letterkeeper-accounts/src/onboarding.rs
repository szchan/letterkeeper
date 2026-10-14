// SPDX-License-Identifier: GPL-3.0-only
//! Offline, user-owned setup inputs. This module never authenticates or opens storage.
use crate::{AccountConfig, AccountError, SentCopyStrategy};

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
    pub fn build(self) -> Result<AccountConfig, AccountError> {
        Err(AccountError::InvalidConfiguration)
    }
}
