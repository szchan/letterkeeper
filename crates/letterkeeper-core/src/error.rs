// SPDX-License-Identifier: GPL-3.0-only

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("account or operation is outside the configured scope")]
    Denied,
    #[error("draft or submission was not found")]
    NotFound,
    #[error("draft revision or approved content changed")]
    Conflict,
    #[error("a current, unexpired human approval is required")]
    ApprovalRequired,
    #[error("submission state prevents this operation")]
    InvalidState,
    #[error("input violates field or size limits")]
    InvalidInput,
    #[error("private database path or permissions are unsafe")]
    UnsafePath,
    #[error("database schema version is unsupported")]
    SchemaVersion,
    #[error("private storage operation failed")]
    Storage,
}
pub type Result<T> = std::result::Result<T, Error>;
impl From<rusqlite::Error> for Error {
    fn from(_: rusqlite::Error) -> Self {
        Self::Storage
    }
}
impl From<serde_json::Error> for Error {
    fn from(_: serde_json::Error) -> Self {
        Self::Storage
    }
}
