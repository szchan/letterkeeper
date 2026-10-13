// SPDX-License-Identifier: GPL-3.0-only
use crate::{AccountConfig, AccountError, Authentication, validate_account_id};
use std::{collections::HashMap, fmt};
use zeroize::Zeroizing;
/// Explicit exposure is restricted to trusted authentication/CLI adapters.
pub struct Secret(Zeroizing<String>);
impl Secret {
    pub fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }
    pub fn expose(&self) -> &str {
        self.0.as_str()
    }
}
impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret([REDACTED])")
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecretKind {
    Password,
    ClientSecret,
    OAuthTokens,
}
impl SecretKind {
    fn key(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::ClientSecret => "client-secret",
            Self::OAuthTokens => "oauth-tokens",
        }
    }
}
pub trait CredentialStore: Send + Sync {
    fn get(&self, account: &str, kind: SecretKind) -> Result<Option<Secret>, AccountError>;
    fn set(&self, account: &str, kind: SecretKind, secret: &Secret) -> Result<(), AccountError>;
    fn delete(&self, account: &str, kind: SecretKind) -> Result<(), AccountError>;
}
/// Linux Secret Service, encrypted DH session only. No file/plain fallback.
#[derive(Debug, Default)]
pub struct SystemCredentialStore;
impl SystemCredentialStore {
    pub fn new() -> Self {
        Self
    }
}
fn attributes(account: &str, kind: SecretKind) -> HashMap<&str, &str> {
    HashMap::from([
        ("application", "org.letterkeeper"),
        ("account", account),
        ("kind", kind.key()),
    ])
}
impl CredentialStore for SystemCredentialStore {
    fn get(&self, account: &str, kind: SecretKind) -> Result<Option<Secret>, AccountError> {
        validate_account_id(account)?;
        let service =
            secret_service::blocking::SecretService::connect(secret_service::EncryptionType::Dh)
                .map_err(|_| AccountError::CredentialUnavailable)?;
        let collection = service
            .get_default_collection()
            .map_err(|_| AccountError::CredentialUnavailable)?;
        if collection
            .is_locked()
            .map_err(|_| AccountError::CredentialUnavailable)?
        {
            return Err(AccountError::CredentialUnavailable);
        }
        let items = collection
            .search_items(attributes(account, kind))
            .map_err(|_| AccountError::CredentialUnavailable)?;
        if items.len() > 1 {
            return Err(AccountError::CredentialUnavailable);
        }
        let Some(item) = items.first() else {
            return Ok(None);
        };
        if item
            .is_locked()
            .map_err(|_| AccountError::CredentialUnavailable)?
        {
            return Err(AccountError::CredentialUnavailable);
        }
        let bytes = Zeroizing::new(
            item.get_secret()
                .map_err(|_| AccountError::CredentialUnavailable)?,
        );
        if bytes.len() > 64 * 1024 {
            return Err(AccountError::CredentialUnavailable);
        }
        let value = std::str::from_utf8(&bytes).map_err(|_| AccountError::CredentialUnavailable)?;
        Ok(Some(Secret::new(value.to_owned())))
    }
    fn set(&self, account: &str, kind: SecretKind, secret: &Secret) -> Result<(), AccountError> {
        validate_account_id(account)?;
        if secret.expose().len() > 64 * 1024 {
            return Err(AccountError::CredentialUnavailable);
        }
        let service =
            secret_service::blocking::SecretService::connect(secret_service::EncryptionType::Dh)
                .map_err(|_| AccountError::CredentialUnavailable)?;
        let collection = service
            .get_default_collection()
            .map_err(|_| AccountError::CredentialUnavailable)?;
        if collection
            .is_locked()
            .map_err(|_| AccountError::CredentialUnavailable)?
        {
            return Err(AccountError::CredentialUnavailable);
        }
        let items = collection
            .search_items(attributes(account, kind))
            .map_err(|_| AccountError::CredentialUnavailable)?;
        if items.len() > 1 {
            return Err(AccountError::CredentialUnavailable);
        }
        collection
            .create_item(
                "LetterKeeper credential",
                attributes(account, kind),
                secret.expose().as_bytes(),
                true,
                "text/plain",
            )
            .map_err(|_| AccountError::CredentialUnavailable)?;
        Ok(())
    }
    fn delete(&self, account: &str, kind: SecretKind) -> Result<(), AccountError> {
        validate_account_id(account)?;
        let service =
            secret_service::blocking::SecretService::connect(secret_service::EncryptionType::Dh)
                .map_err(|_| AccountError::CredentialUnavailable)?;
        let collection = service
            .get_default_collection()
            .map_err(|_| AccountError::CredentialUnavailable)?;
        if collection
            .is_locked()
            .map_err(|_| AccountError::CredentialUnavailable)?
        {
            return Err(AccountError::CredentialUnavailable);
        }
        let items = collection
            .search_items(attributes(account, kind))
            .map_err(|_| AccountError::CredentialUnavailable)?;
        for item in items {
            item.delete()
                .map_err(|_| AccountError::CredentialUnavailable)?
        }
        Ok(())
    }
}
#[derive(Debug)]
pub struct AuthMaterial {
    pub username: String,
    pub secret: Secret,
    pub oauth: bool,
}
pub fn password_auth_material(
    config: &AccountConfig,
    store: &dyn CredentialStore,
) -> Result<AuthMaterial, AccountError> {
    config.validate()?;
    if config.authentication != Authentication::Password {
        return Err(AccountError::InvalidConfiguration);
    }
    let secret = store
        .get(&config.id, SecretKind::Password)?
        .ok_or(AccountError::MissingCredential)?;
    if secret.expose().is_empty()
        || secret.expose().len() > 4096
        || secret.expose().chars().any(char::is_control)
    {
        return Err(AccountError::CredentialUnavailable);
    }
    Ok(AuthMaterial {
        username: config.username.clone(),
        secret,
        oauth: false,
    })
}
