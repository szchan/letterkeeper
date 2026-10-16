// SPDX-License-Identifier: GPL-3.0-only
use super::{
    Store,
    drafts::{load_draft, require_editable},
};
use crate::permissions::Operation;
use crate::validation::check_page;
use crate::{
    Error, Result, SendDiagnostic, SendOutcome, Submission, SubmissionListEntry, SubmissionState,
};
use rusqlite::{
    Connection, OptionalExtension, TransactionBehavior, params, params_from_iter, types::Value,
};
impl Store {
    /// Atomically consumes approval and persists an immutable sending snapshot.
    pub fn claim_send(&mut self, id: i64, now: i64) -> Result<Submission> {
        if now < 0 {
            return Err(Error::InvalidInput);
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let d = load_draft(&tx, id)?;
        self.policy.check(&d.input.account, Operation::Send)?;
        require_editable(&tx, id)?;
        let valid: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM approvals WHERE draft_id=?1 AND revision=?2 AND digest=?3 AND expires_at>?4)", params![id,d.revision,d.digest,now], |r| r.get(0))?;
        if !valid {
            return Err(Error::ApprovalRequired);
        }
        let nonce: String = tx.query_row("SELECT lower(hex(randomblob(16)))", [], |r| r.get(0))?;
        let message_id = format!("<lk-{nonce}@letterkeeper.invalid>");
        tx.execute("INSERT INTO submissions(draft_id,revision,digest,message_id,content,state) VALUES(?1,?2,?3,?4,?5,'submitting')", params![id,d.revision,d.digest,message_id,serde_json::to_string(&d.input)?])?;
        let submission_id = tx.last_insert_rowid();
        tx.execute("DELETE FROM approvals WHERE draft_id=?1", [id])?;
        tx.commit()?;
        Ok(Submission {
            id: submission_id,
            draft_id: id,
            revision: d.revision,
            digest: d.digest,
            message_id,
            input: d.input,
            state: SubmissionState::Submitting,
        })
    }
    pub fn finish_submission(&mut self, id: i64, outcome: SendOutcome) -> Result<Submission> {
        self.finish_submission_with_diagnostic(id, outcome, None)
    }
    /// Atomically persists a terminal outcome and optional non-sensitive diagnostic.
    /// An accepted submission cannot carry a failure diagnostic.
    pub fn finish_submission_with_diagnostic(
        &mut self,
        id: i64,
        outcome: SendOutcome,
        diagnostic: Option<SendDiagnostic>,
    ) -> Result<Submission> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut sub = load_submission(&tx, id)?;
        self.policy.check(&sub.input.account, Operation::Send)?;
        if sub.state != SubmissionState::Submitting {
            return Err(Error::InvalidState);
        }
        if diagnostic.is_some_and(|detail| {
            outcome == SendOutcome::Accepted
                || detail
                    .smtp_code
                    .is_some_and(|code| !(100..=599).contains(&code))
        }) {
            return Err(Error::InvalidInput);
        }
        let (state, label) = match outcome {
            SendOutcome::Accepted => (SubmissionState::Accepted, "accepted"),
            SendOutcome::Failed => (SubmissionState::Failed, "failed"),
            SendOutcome::Uncertain => (SubmissionState::Uncertain, "uncertain"),
        };
        let changed = tx.execute(
            "UPDATE submissions SET state=?1 WHERE id=?2 AND state='submitting'",
            params![label, id],
        )?;
        if changed != 1 {
            return Err(Error::InvalidState);
        }
        if let Some(detail) = diagnostic {
            // Both enums have a closed snake_case string representation.
            let stage = serde_json::to_value(detail.stage)?;
            let kind = serde_json::to_value(detail.kind)?;
            tx.execute(
                "INSERT INTO submission_diagnostics(submission_id,stage,kind,smtp_code) VALUES(?1,?2,?3,?4)",
                params![id, stage.as_str().ok_or(Error::Storage)?, kind.as_str().ok_or(Error::Storage)?, detail.smtp_code],
            )?;
        }
        tx.commit()?;
        sub.state = state;
        Ok(sub)
    }
    /// Returns diagnostics only after checking this submission's account scope.
    /// Missing and out-of-scope IDs share a denial so existence is not disclosed.
    pub fn get_submission_diagnostic(&self, id: i64) -> Result<Option<SendDiagnostic>> {
        if !self.policy.permissions.read {
            return Err(Error::Denied);
        }
        let account: Option<String> = self
            .conn
            .query_row(
                "SELECT json_extract(content,'$.account') FROM submissions WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        self.policy
            .check(account.as_deref().ok_or(Error::Denied)?, Operation::Read)?;
        let row = self
            .conn
            .query_row(
                "SELECT stage,kind,smtp_code FROM submission_diagnostics WHERE submission_id=?1",
                [id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<u16>>(2)?,
                    ))
                },
            )
            .optional()?;
        row.map(|(stage, kind, smtp_code)| {
            if smtp_code.is_some_and(|code| !(100..=599).contains(&code)) {
                return Err(Error::Storage);
            }
            Ok(SendDiagnostic {
                stage: serde_json::from_value(serde_json::Value::String(stage))?,
                kind: serde_json::from_value(serde_json::Value::String(kind))?,
                smtp_code,
            })
        })
        .transpose()
    }
    pub fn get_submission(&self, id: i64) -> Result<Submission> {
        let sub = load_submission(&self.conn, id)?;
        self.policy.check(&sub.input.account, Operation::Read)?;
        Ok(sub)
    }
    pub fn list_submissions(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<SubmissionListEntry>> {
        if !self.policy.permissions.read {
            return Err(Error::Denied);
        }
        check_page(offset, limit)?;
        if self.policy.accounts.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; self.policy.accounts.len()].join(",");
        let sql = format!(
            "SELECT id,draft_id,state FROM submissions WHERE json_extract(content,'$.account') IN ({placeholders}) ORDER BY id DESC LIMIT ? OFFSET ?"
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
                let state: String = r.get(2)?;
                let state = match state.as_str() {
                    "submitting" => SubmissionState::Submitting,
                    "accepted" => SubmissionState::Accepted,
                    "failed" => SubmissionState::Failed,
                    "uncertain" => SubmissionState::Uncertain,
                    _ => return Err(rusqlite::Error::InvalidQuery),
                };
                Ok(SubmissionListEntry {
                    id: r.get(0)?,
                    draft_id: r.get(1)?,
                    state,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }
    /// Explicit recovery after the host has established that this sender exited.
    /// Never call based solely on age or opening a second connection.
    pub fn recover_interrupted(&mut self, id: i64) -> Result<Submission> {
        self.finish_submission(id, SendOutcome::Uncertain)
    }
    pub fn record_submission_wire(&mut self, id: i64, wire: &[u8]) -> Result<()> {
        if wire.is_empty() || wire.len() > 16 * 1024 * 1024 {
            return Err(Error::InvalidInput);
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let sub = load_submission(&tx, id)?;
        self.policy.check(&sub.input.account, Operation::Send)?;
        if sub.state != SubmissionState::Submitting {
            return Err(Error::InvalidState);
        }
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM submission_wire WHERE submission_id=?1)",
            [id],
            |r| r.get(0),
        )?;
        if exists {
            return Err(Error::InvalidState);
        }
        tx.execute(
            "INSERT INTO submission_wire(submission_id,wire) VALUES(?1,?2)",
            params![id, wire],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn submission_wire(&self, id: i64) -> Result<Vec<u8>> {
        let sub = load_submission(&self.conn, id)?;
        self.policy.check(&sub.input.account, Operation::Read)?;
        self.conn
            .query_row(
                "SELECT wire FROM submission_wire WHERE submission_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or(Error::NotFound)
    }
}
pub(super) fn load_submission(conn: &Connection, id: i64) -> Result<Submission> {
    let row = conn
        .query_row(
            "SELECT draft_id,revision,digest,message_id,content,state FROM submissions WHERE id=?1",
            [id],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                ))
            },
        )
        .optional()?;
    let (draft_id, revision, digest, message_id, content, state) = row.ok_or(Error::NotFound)?;
    let state = match state.as_str() {
        "submitting" => SubmissionState::Submitting,
        "accepted" => SubmissionState::Accepted,
        "failed" => SubmissionState::Failed,
        "uncertain" => SubmissionState::Uncertain,
        _ => return Err(Error::Storage),
    };
    Ok(Submission {
        id,
        draft_id,
        revision,
        digest,
        message_id,
        input: serde_json::from_str(&content)?,
        state,
    })
}
