// SPDX-License-Identifier: GPL-3.0-only
use crate::{AccountConfig, AccountError, Authentication, validate_account_id};
use rustix::fs::{self as fs, FileType, Mode, OFlags};
use std::{
    collections::HashMap,
    fmt,
    path::{Component, Path, PathBuf},
};
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
    /// Shared vault-account lock namespace. Test stores need no system coordination.
    /// The application must hold an exclusive lock across the whole token operation.
    fn coordination_directory(&self, _account: &str) -> Result<Option<PathBuf>, AccountError> {
        Ok(None)
    }
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
fn check_runtime_directory(path: &Path, uid: u32) -> Result<(), AccountError> {
    if !path.is_absolute() {
        return Err(AccountError::UnsafeStorage);
    }
    let mut fd = fs::open(
        "/",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| AccountError::UnsafeStorage)?;
    for component in path.components() {
        match component {
            Component::RootDir => (),
            Component::Normal(part) => {
                fd = fs::openat(
                    &fd,
                    part,
                    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                )
                .map_err(|_| AccountError::UnsafeStorage)?
            }
            _ => return Err(AccountError::UnsafeStorage),
        }
    }
    let stat = fs::fstat(&fd).map_err(|_| AccountError::UnsafeStorage)?;
    if FileType::from_raw_mode(stat.st_mode) != FileType::Directory
        || stat.st_uid != uid
        || stat.st_mode & 0o7777 != 0o700
    {
        return Err(AccountError::UnsafeStorage);
    }
    Ok(())
}
impl CredentialStore for SystemCredentialStore {
    fn coordination_directory(&self, account: &str) -> Result<Option<PathBuf>, AccountError> {
        validate_account_id(account)?;
        let uid = rustix::process::geteuid().as_raw();
        let runtime = PathBuf::from(format!("/run/user/{uid}"));
        // Never create the login-session runtime directory or fall back to state/XDG paths.
        check_runtime_directory(&runtime, uid)?;
        Ok(Some(
            runtime.join("letterkeeper/credential-locks").join(account),
        ))
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};
    #[test]
    fn runtime_parent_must_exist_private_owned_and_without_symlinks() {
        let temp = tempfile::tempdir().unwrap();
        let uid = rustix::process::geteuid().as_raw();
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        check_runtime_directory(temp.path(), uid).unwrap();
        assert_eq!(
            check_runtime_directory(temp.path(), uid + 1).unwrap_err(),
            AccountError::UnsafeStorage
        );
        let absent = temp.path().join("absent");
        assert_eq!(
            check_runtime_directory(&absent, uid).unwrap_err(),
            AccountError::UnsafeStorage
        );
        assert!(!absent.exists());
        let link = temp.path().join("link");
        symlink(temp.path(), &link).unwrap();
        assert_eq!(
            check_runtime_directory(&link, uid).unwrap_err(),
            AccountError::UnsafeStorage
        );
        for mode in [0o740, 0o755, 0o1700] {
            std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(mode)).unwrap();
            assert_eq!(
                check_runtime_directory(temp.path(), uid).unwrap_err(),
                AccountError::UnsafeStorage
            );
        }
    }
}
