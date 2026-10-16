// SPDX-License-Identifier: GPL-3.0-only
use super::Store;
use crate::permissions::Operation;
use crate::validation::{check_page, content_digest, validate};
use crate::{Draft, DraftInput, DraftListEntry, Error, Result};
use rusqlite::{
    Connection, OptionalExtension, TransactionBehavior, params, params_from_iter, types::Value,
};
impl Store {
    pub fn create_draft(&mut self, input: DraftInput) -> Result<Draft> {
        self.policy.check(&input.account, Operation::Draft)?;
        validate(&input)?;
        let digest = content_digest(1, &input)?;
        let content = serde_json::to_string(&input)?;
        self.conn.execute(
            "INSERT INTO drafts(revision,digest,content) VALUES(1,?1,?2)",
            params![digest, content],
        )?;
        Ok(Draft {
            id: self.conn.last_insert_rowid(),
            revision: 1,
            digest,
            input,
        })
    }
    pub fn get_draft(&self, id: i64) -> Result<Draft> {
        let d = load_draft(&self.conn, id)?;
        self.policy.check(&d.input.account, Operation::Read)?;
        Ok(d)
    }
    pub fn list_drafts(&self, offset: usize, limit: usize) -> Result<Vec<DraftListEntry>> {
        if !self.policy.permissions.read {
            return Err(Error::Denied);
        }
        check_page(offset, limit)?;
        if self.policy.accounts.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; self.policy.accounts.len()].join(",");
        let sql = format!(
            "SELECT id,revision,digest,json_extract(content,'$.subject') FROM drafts WHERE json_extract(content,'$.account') IN ({placeholders}) ORDER BY id DESC LIMIT ? OFFSET ?"
        );
        let mut values: Vec<Value> = self
            .policy
            .accounts
            .iter()
            .cloned()
            .map(Value::Text)
            .collect();
        values.push(Value::Integer(limit as i64));
        values.push(Value::Integer(offset as i64));
        let mut stmt = self.conn.prepare(&sql)?;
        Ok(stmt
            .query_map(params_from_iter(values), |r| {
                Ok(DraftListEntry {
                    id: r.get(0)?,
                    revision: r.get(1)?,
                    digest: r.get(2)?,
                    subject: r.get(3)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }
    pub fn update_draft(
        &mut self,
        id: i64,
        expected_revision: i64,
        input: DraftInput,
    ) -> Result<Draft> {
        self.policy.check(&input.account, Operation::Draft)?;
        validate(&input)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = load_draft(&tx, id)?;
        self.policy.check(&old.input.account, Operation::Draft)?;
        if old.revision != expected_revision {
            return Err(Error::Conflict);
        }
        require_editable(&tx, id)?;
        let revision = old.revision.checked_add(1).ok_or(Error::Conflict)?;
        let digest = content_digest(revision, &input)?;
        tx.execute(
            "UPDATE drafts SET revision=?1,digest=?2,content=?3 WHERE id=?4 AND revision=?5",
            params![
                revision,
                digest,
                serde_json::to_string(&input)?,
                id,
                expected_revision
            ],
        )?;
        tx.execute("DELETE FROM approvals WHERE draft_id=?1", [id])?;
        tx.commit()?;
        Ok(Draft {
            id,
            revision,
            digest,
            input,
        })
    }
}
pub(super) fn require_editable(conn: &Connection, id: i64) -> Result<()> {
    let blocked: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM submissions WHERE draft_id=?1 AND state!='failed')",
        [id],
        |r| r.get(0),
    )?;
    if blocked {
        Err(Error::InvalidState)
    } else {
        Ok(())
    }
}
pub(super) fn load_draft(conn: &Connection, id: i64) -> Result<Draft> {
    let row: Option<(i64, String, String)> = conn
        .query_row(
            "SELECT revision,digest,content FROM drafts WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let (revision, digest, content) = row.ok_or(Error::NotFound)?;
    Ok(Draft {
        id,
        revision,
        digest,
        input: serde_json::from_str(&content)?,
    })
}
