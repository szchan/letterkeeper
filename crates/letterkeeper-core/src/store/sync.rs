// SPDX-License-Identifier: GPL-3.0-only
use super::{
    Store,
    drafts::{load_draft, require_editable},
    submissions::load_submission,
};
use crate::permissions::Operation;
use crate::validation::validate_folder;
use crate::{
    DraftInput, Error, Result, SendOutcome, SubmissionState, SyncKind, SyncRecord, SyncState,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
impl Store {
    pub fn claim_draft_sync(&mut self, id: i64, folder: &str) -> Result<SyncRecord> {
        validate_folder(folder)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let d = load_draft(&tx, id)?;
        self.policy.check(&d.input.account, Operation::Draft)?;
        require_editable(&tx, id)?;
        let nonce: String = tx.query_row("SELECT lower(hex(randomblob(16)))", [], |r| r.get(0))?;
        let message_id = format!("<draft-{nonce}@letterkeeper.invalid>");
        let record = insert_sync(
            &tx,
            SyncKind::Draft,
            id,
            d.revision,
            folder,
            &message_id,
            &d.digest,
            &d.input,
        )?;
        tx.commit()?;
        Ok(record)
    }
    pub fn claim_sent_sync(&mut self, id: i64, folder: &str) -> Result<SyncRecord> {
        validate_folder(folder)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let sub = load_submission(&tx, id)?;
        self.policy.check(&sub.input.account, Operation::Send)?;
        if sub.state != SubmissionState::Accepted {
            return Err(Error::InvalidState);
        }
        let record = insert_sync(
            &tx,
            SyncKind::Sent,
            id,
            sub.revision,
            folder,
            &sub.message_id,
            &sub.digest,
            &sub.input,
        )?;
        tx.commit()?;
        Ok(record)
    }
    pub fn finish_sync(&mut self, id: i64, outcome: SendOutcome) -> Result<SyncRecord> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let record = load_sync(&tx, id)?;
        self.policy.check(
            &record.account,
            match record.kind {
                SyncKind::Draft => Operation::Draft,
                SyncKind::Sent => Operation::Send,
            },
        )?;
        if record.state != SyncState::Pending {
            return Err(Error::InvalidState);
        }
        let state = match outcome {
            SendOutcome::Accepted => "confirmed",
            SendOutcome::Failed => "failed",
            SendOutcome::Uncertain => "uncertain",
        };
        tx.execute(
            "UPDATE sync_records SET state=?1 WHERE id=?2 AND state='pending'",
            params![state, id],
        )?;
        let record = load_sync(&tx, id)?;
        tx.commit()?;
        Ok(record)
    }
    pub fn get_sync(&self, id: i64) -> Result<SyncRecord> {
        let record = load_sync(&self.conn, id)?;
        self.policy.check(&record.account, Operation::Read)?;
        Ok(record)
    }
}
#[allow(clippy::too_many_arguments)]
fn insert_sync(
    conn: &Connection,
    kind: SyncKind,
    id: i64,
    revision: i64,
    folder: &str,
    message_id: &str,
    digest: &str,
    input: &DraftInput,
) -> Result<SyncRecord> {
    let kind = match kind {
        SyncKind::Draft => "draft",
        SyncKind::Sent => "sent",
    };
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sync_records WHERE kind=?1 AND local_id=?2 AND revision=?3)",
        params![kind, id, revision],
        |r| r.get(0),
    )?;
    if exists {
        return Err(Error::InvalidState);
    }
    conn.execute("INSERT INTO sync_records(kind,local_id,revision,folder,message_id,digest,content,state) VALUES(?1,?2,?3,?4,?5,?6,?7,'pending')",params![kind,id,revision,folder,message_id,digest,serde_json::to_string(input)?])?;
    load_sync(conn, conn.last_insert_rowid())
}
fn load_sync(conn: &Connection, id: i64) -> Result<SyncRecord> {
    let row = conn.query_row("SELECT kind,local_id,revision,folder,message_id,digest,content,state FROM sync_records WHERE id=?1",[id], |r| Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?,r.get::<_,String>(6)?,r.get::<_,String>(7)?))).optional()?.ok_or(Error::NotFound)?;
    let input: DraftInput = serde_json::from_str(&row.6)?;
    Ok(SyncRecord {
        id,
        kind: match row.0.as_str() {
            "draft" => SyncKind::Draft,
            "sent" => SyncKind::Sent,
            _ => return Err(Error::Storage),
        },
        local_id: row.1,
        revision: row.2,
        folder: row.3,
        message_id: row.4,
        digest: row.5,
        account: input.account.clone(),
        input,
        state: match row.7.as_str() {
            "pending" => SyncState::Pending,
            "confirmed" => SyncState::Confirmed,
            "failed" => SyncState::Failed,
            "uncertain" => SyncState::Uncertain,
            _ => return Err(Error::Storage),
        },
    })
}
