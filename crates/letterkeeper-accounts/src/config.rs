// SPDX-License-Identifier: GPL-3.0-only
use crate::AccountError;
use rustix::fs::{self as fs, AtFlags, FileType, Mode, OFlags, RenameFlags};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{Read, Write},
    os::fd::OwnedFd,
    path::{Component, Path, PathBuf},
};
pub const MAX_CONFIG_BYTES: usize = 64 * 1024;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TlsEndpoint {
    pub host: String,
    pub port: u16,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationPermissions {
    pub read: bool,
    pub draft: bool,
    pub send: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SentCopyStrategy {
    ServerManaged,
    Append,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Authentication {
    Password,
    #[serde(alias = "gmail_oauth")]
    GmailOAuth {
        client_id: String,
        client_secret_ref: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountConfig {
    pub id: String,
    pub email: String,
    pub username: String,
    pub imap: TlsEndpoint,
    pub smtp: TlsEndpoint,
    pub allowed_folders: Vec<String>,
    pub permissions: OperationPermissions,
    pub drafts_folder: String,
    pub sent_folder: String,
    pub sent_copy: SentCopyStrategy,
    pub authentication: Authentication,
    pub extra_ca_pem: Option<PathBuf>,
}
pub fn validate_account_id(id: &str) -> Result<(), AccountError> {
    if id.is_empty()
        || id.len() > 64
        || !id.as_bytes()[0].is_ascii_lowercase()
        || !id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err(AccountError::InvalidConfiguration);
    }
    Ok(())
}
fn safe_text(s: &str, max: usize) -> bool {
    !s.is_empty()
        && s.len() <= max
        && !s
            .chars()
            .any(|c| c.is_control() || matches!(c,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}'))
}
impl AccountConfig {
    pub fn validate(&self) -> Result<(), AccountError> {
        validate_account_id(&self.id)?;
        let valid_host = |e: &TlsEndpoint| {
            e.port != 0
                && safe_text(&e.host, 253)
                && e.host
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b".-:".contains(&c))
                && !e.host.starts_with('-')
                && !e.host.contains("..")
        };
        if !safe_text(&self.email, 320)
            || !self.email.contains('@')
            || self.email.chars().any(char::is_whitespace)
            || !safe_text(&self.username, 320)
            || !valid_host(&self.imap)
            || !valid_host(&self.smtp)
            || self.allowed_folders.is_empty()
            || self.allowed_folders.len() > 128
            || self.allowed_folders.iter().any(|f| !safe_text(f, 512))
            || !safe_text(&self.drafts_folder, 512)
            || !safe_text(&self.sent_folder, 512)
        {
            return Err(AccountError::InvalidConfiguration);
        }
        if self.permissions.draft && !self.allowed_folders.contains(&self.drafts_folder)
            || self.permissions.send && !self.allowed_folders.contains(&self.sent_folder)
        {
            return Err(AccountError::InvalidConfiguration);
        }
        if let Authentication::GmailOAuth { client_id, .. } = &self.authentication
            && (!safe_text(client_id, 512) || client_id.chars().any(char::is_whitespace))
        {
            return Err(AccountError::InvalidConfiguration);
        }
        if let Some(p) = &self.extra_ca_pem
            && (!p.is_absolute() || !safe_text(&p.to_string_lossy(), 4096))
        {
            return Err(AccountError::InvalidConfiguration);
        }
        Ok(())
    }
}
pub struct AccountRepository {
    config_dir: PathBuf,
    data_dir: PathBuf,
    config_fd: OwnedFd,
    data_fd: OwnedFd,
}
fn private_dir(path: &Path) -> Result<OwnedFd, AccountError> {
    if !path.is_absolute() {
        return Err(AccountError::UnsafeStorage);
    }
    let mut dir = fs::open(
        "/",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| AccountError::UnsafeStorage)?;
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(name) => {
                match fs::mkdirat(&dir, name, Mode::from_raw_mode(0o700)) {
                    Ok(()) => {}
                    Err(e) if e == rustix::io::Errno::EXIST => {}
                    Err(_) => return Err(AccountError::UnsafeStorage),
                }
                dir = fs::openat(
                    &dir,
                    name,
                    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                )
                .map_err(|_| AccountError::UnsafeStorage)?;
            }
            _ => return Err(AccountError::UnsafeStorage),
        }
    }
    check_dir(&dir)?;
    Ok(dir)
}
fn check_dir(dir: &OwnedFd) -> Result<(), AccountError> {
    let stat = fs::fstat(dir).map_err(|_| AccountError::UnsafeStorage)?;
    if FileType::from_raw_mode(stat.st_mode) != FileType::Directory
        || stat.st_mode & 0o7777 != 0o700
        || stat.st_uid != rustix::process::geteuid().as_raw()
    {
        return Err(AccountError::UnsafeStorage);
    }
    Ok(())
}
fn check_file(file: &OwnedFd) -> Result<(), AccountError> {
    let s = fs::fstat(file).map_err(|_| AccountError::UnsafeStorage)?;
    if FileType::from_raw_mode(s.st_mode) != FileType::RegularFile
        || s.st_mode & 0o7777 != 0o600
        || s.st_nlink != 1
        || s.st_uid != rustix::process::geteuid().as_raw()
        || s.st_size < 0
        || s.st_size as usize > MAX_CONFIG_BYTES
    {
        return Err(AccountError::UnsafeStorage);
    }
    Ok(())
}
impl AccountRepository {
    pub fn new(config_dir: PathBuf, data_dir: PathBuf) -> Result<Self, AccountError> {
        let config_fd = private_dir(&config_dir)?;
        let data_fd = private_dir(&data_dir)?;
        Ok(Self {
            config_dir,
            data_dir,
            config_fd,
            data_fd,
        })
    }
    pub fn default_paths() -> Result<(PathBuf, PathBuf), AccountError> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .ok_or(AccountError::UnsafeStorage)?;
        let base = |key: &str, fallback: PathBuf| {
            std::env::var_os(key)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or(fallback)
        };
        Ok((
            base("XDG_CONFIG_HOME", home.join(".config")).join("letterkeeper"),
            base("XDG_DATA_HOME", home.join(".local/share")).join("letterkeeper"),
        ))
    }
    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }
    fn verify_dirs(&self) -> Result<(), AccountError> {
        check_dir(&self.config_fd)?;
        check_dir(&self.data_fd)
    }
    pub fn load(&self, id: &str) -> Result<AccountConfig, AccountError> {
        self.verify_dirs()?;
        validate_account_id(id)?;
        let fd = fs::openat(
            &self.config_fd,
            format!("{id}.json"),
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        )
        .map_err(|e| {
            if e == rustix::io::Errno::NOENT {
                AccountError::AccountNotFound
            } else {
                AccountError::UnsafeStorage
            }
        })?;
        check_file(&fd)?;
        let mut body = Vec::new();
        File::from(fd)
            .take(MAX_CONFIG_BYTES as u64 + 1)
            .read_to_end(&mut body)
            .map_err(|_| AccountError::UnsafeStorage)?;
        if body.len() > MAX_CONFIG_BYTES {
            return Err(AccountError::UnsafeStorage);
        }
        let account: AccountConfig =
            serde_json::from_slice(&body).map_err(|_| AccountError::InvalidConfiguration)?;
        account.validate()?;
        if account.id != id {
            return Err(AccountError::InvalidConfiguration);
        }
        Ok(account)
    }
    /// Publishes a complete private configuration only if its name does not exist.
    /// Concurrent creators have exactly one winner; no existing inode is replaced.
    /// A post-publication fsync failure reports uncertainty: reload before retrying.
    pub fn create(&self, account: &AccountConfig) -> Result<(), AccountError> {
        self.verify_dirs()?;
        account.validate()?;
        let bytes =
            serde_json::to_vec_pretty(account).map_err(|_| AccountError::InvalidConfiguration)?;
        if bytes.len() > MAX_CONFIG_BYTES {
            return Err(AccountError::InvalidConfiguration);
        }
        let name = format!("{}.json", account.id);
        let nonce = oauth2::CsrfToken::new_random();
        let tmp = format!(".tmp-{}", nonce.secret());
        let fd = fs::openat(
            &self.config_fd,
            &tmp,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )
        .map_err(|_| AccountError::UnsafeStorage)?;
        let result = (|| {
            check_file(&fd)?;
            let mut file = File::from(fd);
            file.write_all(&bytes)
                .map_err(|_| AccountError::UnsafeStorage)?;
            file.sync_all().map_err(|_| AccountError::UnsafeStorage)?;
            fs::renameat_with(
                &self.config_fd,
                &tmp,
                &self.config_fd,
                &name,
                RenameFlags::NOREPLACE,
            )
            .map_err(|error| {
                if error == rustix::io::Errno::EXIST {
                    AccountError::AccountAlreadyExists
                } else {
                    AccountError::UnsafeStorage
                }
            })?;
            fs::fsync(&self.config_fd).map_err(|_| AccountError::AccountPersistenceUncertain)
        })();
        if result.is_err() {
            let _ = fs::unlinkat(&self.config_fd, &tmp, AtFlags::empty());
        }
        result
    }
    pub fn save(&self, account: &AccountConfig) -> Result<(), AccountError> {
        self.verify_dirs()?;
        account.validate()?;
        let bytes =
            serde_json::to_vec_pretty(account).map_err(|_| AccountError::InvalidConfiguration)?;
        if bytes.len() > MAX_CONFIG_BYTES {
            return Err(AccountError::InvalidConfiguration);
        }
        let name = format!("{}.json", account.id);
        match fs::openat(
            &self.config_fd,
            &name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        ) {
            Ok(fd) => check_file(&fd)?,
            Err(e) if e == rustix::io::Errno::NOENT => {}
            Err(_) => return Err(AccountError::UnsafeStorage),
        }
        let nonce = oauth2::CsrfToken::new_random();
        let tmp = format!(".tmp-{}", nonce.secret());
        let fd = fs::openat(
            &self.config_fd,
            &tmp,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )
        .map_err(|_| AccountError::UnsafeStorage)?;
        let result = (|| {
            let mut file = File::from(fd);
            file.write_all(&bytes)
                .map_err(|_| AccountError::UnsafeStorage)?;
            file.sync_all().map_err(|_| AccountError::UnsafeStorage)?;
            fs::renameat(&self.config_fd, &tmp, &self.config_fd, &name)
                .map_err(|_| AccountError::UnsafeStorage)?;
            fs::fsync(&self.config_fd).map_err(|_| AccountError::UnsafeStorage)
        })();
        if result.is_err() {
            let _ = fs::unlinkat(&self.config_fd, &tmp, AtFlags::empty());
        }
        result
    }
    pub fn list(&self) -> Result<Vec<String>, AccountError> {
        self.verify_dirs()?;
        let mut accounts = Vec::new();
        let entries =
            std::fs::read_dir(&self.config_dir).map_err(|_| AccountError::UnsafeStorage)?;
        for entry in entries {
            let entry = entry.map_err(|_| AccountError::UnsafeStorage)?;
            if let Some(id) = entry
                .file_name()
                .to_str()
                .and_then(|s| s.strip_suffix(".json"))
            {
                self.load(id)?;
                accounts.push(id.to_owned());
                if accounts.len() > 128 {
                    return Err(AccountError::UnsafeStorage);
                }
            }
        }
        accounts.sort();
        Ok(accounts)
    }
}
