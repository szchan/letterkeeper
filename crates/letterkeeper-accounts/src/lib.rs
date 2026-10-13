// SPDX-License-Identifier: GPL-3.0-only
//! User-owned account configuration and credentials; no MCP policy mutation surface.
mod config;
mod credentials;
mod oauth;
pub use config::*;
pub use credentials::*;
pub use oauth::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AccountError {
    #[error("invalid account configuration")]
    InvalidConfiguration,
    #[error("private account storage unavailable or unsafe")]
    UnsafeStorage,
    #[error("account not found")]
    AccountNotFound,
    #[error("credential service unavailable, locked, or ambiguous")]
    CredentialUnavailable,
    #[error("credential missing; log in again")]
    MissingCredential,
    #[error("OAuth network request failed")]
    OAuthRequestFailed,
    #[error("OAuth authorization denied")]
    AuthorizationDenied,
    #[error("invalid OAuth callback")]
    InvalidCallback,
    #[error("OAuth callback timed out")]
    Timeout,
    #[error("OAuth login already consumed")]
    AlreadyConsumed,
    #[error("OAuth grant does not include required mail scope")]
    InsufficientScope,
    #[error("OAuth grant invalid; log in again")]
    ReauthenticationRequired,
}
