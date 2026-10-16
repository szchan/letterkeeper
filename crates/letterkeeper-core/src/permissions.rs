// SPDX-License-Identifier: GPL-3.0-only
use crate::{Error, Result};
use std::collections::BTreeSet;
#[derive(Clone, Copy, Debug)]
pub struct Permissions {
    pub read: bool,
    pub draft: bool,
    pub send: bool,
}
/// Constructed by the application host from user configuration, never tool arguments.
pub struct AccessPolicy {
    pub(crate) accounts: BTreeSet<String>,
    pub(crate) permissions: Permissions,
}
impl AccessPolicy {
    pub fn new(accounts: Vec<String>, permissions: Permissions) -> Self {
        Self {
            accounts: accounts.into_iter().collect(),
            permissions,
        }
    }
    pub(crate) fn check(&self, account: &str, operation: Operation) -> Result<()> {
        let allowed = match operation {
            Operation::Read => self.permissions.read,
            Operation::Draft => self.permissions.draft,
            Operation::Send => self.permissions.send,
        };
        if allowed && self.accounts.contains(account) {
            Ok(())
        } else {
            Err(Error::Denied)
        }
    }
}
pub(crate) enum Operation {
    Read,
    Draft,
    Send,
}
