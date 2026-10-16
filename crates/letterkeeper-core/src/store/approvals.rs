// SPDX-License-Identifier: GPL-3.0-only
use super::{
    Store,
    drafts::{load_draft, require_editable},
};
use crate::permissions::Operation;
use crate::{Error, Result};
use rusqlite::{TransactionBehavior, params};
impl Store {
    /// Only the trusted human CLI/TUI approval path may call this method.
    /// `send` permission permits consuming approval; it does not itself create approval.
    /// MCP must never expose this method, or an equivalent model-writable flag.
    pub fn approve(&mut self, id: i64, expected_digest: &str, expires_at: i64) -> Result<()> {
        if expires_at <= 0 {
            return Err(Error::InvalidInput);
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let d = load_draft(&tx, id)?;
        self.policy.check(&d.input.account, Operation::Send)?;
        require_editable(&tx, id)?;
        if d.digest != expected_digest {
            return Err(Error::Conflict);
        }
        tx.execute("INSERT INTO approvals(draft_id,revision,digest,expires_at) VALUES(?1,?2,?3,?4) ON CONFLICT(draft_id) DO UPDATE SET revision=excluded.revision,digest=excluded.digest,expires_at=excluded.expires_at", params![id,d.revision,d.digest,expires_at])?;
        tx.commit()?;
        Ok(())
    }
}
