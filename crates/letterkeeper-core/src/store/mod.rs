// SPDX-License-Identifier: GPL-3.0-only
//! Durable SQLite state and its transaction boundaries.
mod approvals;
mod drafts;
mod migrations;
mod paths;
mod submissions;
mod sync;

use crate::{AccessPolicy, Result};
use paths::private_path;
use rusqlite::{Connection, OpenFlags};
use std::path::Path;
/// SQLite owns durable state only. No network operations run inside transactions.
pub struct Store {
    conn: Connection,
    policy: AccessPolicy,
}
impl Store {
    pub fn memory() -> Result<Self> {
        Self::initialize(Connection::open_in_memory()?)
    }
    /// The parent directory must be private (0700), and existing files must be 0600.
    /// Missing directories/files are created privately. Symlinks are rejected.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        private_path(path)?;
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        Self::initialize(conn)
    }
    pub fn with_policy(mut self, policy: AccessPolicy) -> Self {
        self.policy = policy;
        self
    }
}
