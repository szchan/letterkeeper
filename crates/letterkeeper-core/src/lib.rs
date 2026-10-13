// SPDX-License-Identifier: GPL-3.0-only
//! Shared draft, authorization and submission state.
use rusqlite::{
    Connection, OpenFlags, OptionalExtension, TransactionBehavior, params, params_from_iter,
    types::Value,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::Path, time::Duration};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    pub name: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftInput {
    pub account: String,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub body: String,
    pub attachments: Vec<Attachment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_reply_to: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draft {
    pub id: i64,
    pub revision: i64,
    pub digest: String,
    pub input: DraftInput,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DraftListEntry {
    pub id: i64,
    pub revision: i64,
    pub digest: String,
    pub subject: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubmissionState {
    Submitting,
    Accepted,
    Failed,
    Uncertain,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendOutcome {
    Accepted,
    Failed,
    Uncertain,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Submission {
    pub id: i64,
    pub draft_id: i64,
    pub revision: i64,
    pub digest: String,
    pub message_id: String,
    pub input: DraftInput,
    pub state: SubmissionState,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmissionListEntry {
    pub id: i64,
    pub draft_id: i64,
    pub state: SubmissionState,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncState {
    Pending,
    Confirmed,
    Failed,
    Uncertain,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncKind {
    Draft,
    Sent,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncRecord {
    pub id: i64,
    pub kind: SyncKind,
    pub local_id: i64,
    pub revision: i64,
    pub account: String,
    pub folder: String,
    pub message_id: String,
    pub state: SyncState,
    pub digest: String,
    pub input: DraftInput,
}
#[derive(Clone, Copy, Debug)]
pub struct Permissions {
    pub read: bool,
    pub draft: bool,
    pub send: bool,
}
/// Constructed by the application host from user configuration, never tool arguments.
pub struct AccessPolicy {
    accounts: BTreeSet<String>,
    permissions: Permissions,
}
impl AccessPolicy {
    pub fn new(accounts: Vec<String>, permissions: Permissions) -> Self {
        Self {
            accounts: accounts.into_iter().collect(),
            permissions,
        }
    }
    fn check(&self, account: &str, operation: Operation) -> Result<()> {
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
enum Operation {
    Read,
    Draft,
    Send,
}
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
    fn initialize(mut conn: Connection) -> Result<Self> {
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "journal_mode", "DELETE")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let version: i64 = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
        match version {
            0 => {
                let tables: i64 = tx.query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'", [], |r| r.get(0))?;
                if tables != 0 {
                    return Err(Error::SchemaVersion);
                }
                tx.execute_batch("CREATE TABLE drafts(id INTEGER PRIMARY KEY AUTOINCREMENT, revision INTEGER NOT NULL, digest TEXT NOT NULL, content TEXT NOT NULL);
                    CREATE TABLE approvals(draft_id INTEGER PRIMARY KEY REFERENCES drafts(id), revision INTEGER NOT NULL, digest TEXT NOT NULL, expires_at INTEGER NOT NULL);
                    CREATE TABLE submissions(id INTEGER PRIMARY KEY AUTOINCREMENT, draft_id INTEGER NOT NULL REFERENCES drafts(id), revision INTEGER NOT NULL, digest TEXT NOT NULL, message_id TEXT NOT NULL UNIQUE, content TEXT NOT NULL, state TEXT NOT NULL CHECK(state IN ('submitting','accepted','failed','uncertain')));
                    CREATE INDEX submission_draft ON submissions(draft_id); PRAGMA user_version=1;")?;
            }
            1 | 2 => {}
            _ => return Err(Error::SchemaVersion),
        }
        if version < 2 {
            tx.execute_batch("CREATE TABLE sync_records(id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL CHECK(kind IN ('draft','sent')), local_id INTEGER NOT NULL, revision INTEGER NOT NULL, folder TEXT NOT NULL, message_id TEXT NOT NULL, digest TEXT NOT NULL, content TEXT NOT NULL, state TEXT NOT NULL CHECK(state IN ('pending','confirmed','failed','uncertain')), UNIQUE(kind,local_id,revision));
                CREATE TABLE submission_wire(submission_id INTEGER PRIMARY KEY REFERENCES submissions(id), wire BLOB NOT NULL); PRAGMA user_version=2;")?;
        }
        tx.commit()?;
        Ok(Self {
            conn,
            policy: AccessPolicy::new(
                vec![],
                Permissions {
                    read: false,
                    draft: false,
                    send: false,
                },
            ),
        })
    }
    pub fn with_policy(mut self, policy: AccessPolicy) -> Self {
        self.policy = policy;
        self
    }
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
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut sub = load_submission(&tx, id)?;
        self.policy.check(&sub.input.account, Operation::Send)?;
        if sub.state != SubmissionState::Submitting {
            return Err(Error::InvalidState);
        }
        let (state, label) = match outcome {
            SendOutcome::Accepted => (SubmissionState::Accepted, "accepted"),
            SendOutcome::Failed => (SubmissionState::Failed, "failed"),
            SendOutcome::Uncertain => (SubmissionState::Uncertain, "uncertain"),
        };
        tx.execute(
            "UPDATE submissions SET state=?1 WHERE id=?2 AND state='submitting'",
            params![label, id],
        )?;
        tx.commit()?;
        sub.state = state;
        Ok(sub)
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
fn check_page(offset: usize, limit: usize) -> Result<()> {
    if offset > 100_000 || !(1..=50).contains(&limit) {
        Err(Error::InvalidInput)
    } else {
        Ok(())
    }
}
fn validate_folder(folder: &str) -> Result<()> {
    if folder.is_empty() || !header(folder, 512) {
        Err(Error::InvalidInput)
    } else {
        Ok(())
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
fn require_editable(conn: &Connection, id: i64) -> Result<()> {
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
fn load_draft(conn: &Connection, id: i64) -> Result<Draft> {
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
fn load_submission(conn: &Connection, id: i64) -> Result<Submission> {
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
fn content_digest(revision: i64, input: &DraftInput) -> Result<String> {
    let bytes = serde_json::to_vec(&("letterkeeper-draft-v1", revision, input))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
fn dangerous(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200e}' | '\u{200f}' | '\u{061c}')
}
fn header(value: &str, max: usize) -> bool {
    value.len() <= max && !value.chars().any(dangerous)
}
fn address(value: &str) -> bool {
    if !header(value, 254) || !value.is_ascii() {
        return false;
    }
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && local.len() <= 64
        && !domain.is_empty()
        && !domain.contains('@')
        && local
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".!#$%&'*+-/=?^_`{|}~".contains(&c))
        && domain
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".-".contains(&c))
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !local.contains("..")
        && !domain.starts_with(['.', '-'])
        && !domain.ends_with(['.', '-'])
        && !domain.contains("..")
}
fn validate(input: &DraftInput) -> Result<()> {
    let recipients = input
        .to
        .len()
        .checked_add(input.cc.len())
        .and_then(|n| n.checked_add(input.bcc.len()))
        .ok_or(Error::InvalidInput)?;
    if input.account.is_empty()
        || !header(&input.account, 128)
        || !address(&input.from)
        || recipients == 0
        || recipients > 100
        || !input
            .to
            .iter()
            .chain(&input.cc)
            .chain(&input.bcc)
            .all(|s| address(s))
        || !header(&input.subject, 998)
        || input.body.len() > 1024 * 1024
        || input
            .body
            .chars()
            .any(|c| dangerous(c) && c != '\n' && c != '\t')
        || input.attachments.len() > 16
        || input
            .in_reply_to
            .as_deref()
            .is_some_and(|id| !message_id(id))
        || input.references.len() > 32
        || !input.references.iter().all(|id| message_id(id))
    {
        return Err(Error::InvalidInput);
    }
    let mut total = 0usize;
    for a in &input.attachments {
        if a.name.is_empty()
            || !header(&a.name, 255)
            || matches!(a.name.as_str(), "." | "..")
            || a.name.contains(['/', '\\'])
            || !header(&a.content_type, 127)
            || a.content_type.split('/').count() != 2
            || a.content_type.split('/').any(str::is_empty)
            || !a
                .content_type
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"!#$&^_.+-/".contains(&c))
        {
            return Err(Error::InvalidInput);
        }
        total = total
            .checked_add(a.bytes.len())
            .ok_or(Error::InvalidInput)?;
        if total > 10 * 1024 * 1024 {
            return Err(Error::InvalidInput);
        }
    }
    Ok(())
}
fn message_id(value: &str) -> bool {
    if !value.is_ascii() || value.len() > 256 {
        return false;
    }
    let Some(inner) = value.strip_prefix('<').and_then(|v| v.strip_suffix('>')) else {
        return false;
    };
    let Some((left, right)) = inner.split_once('@') else {
        return false;
    };
    let atom = |s: &str| {
        !s.is_empty()
            && s.split('.').all(|part| {
                !part.is_empty()
                    && part
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-/=?^_`{|}~".contains(&b))
            })
    };
    atom(left)
        && (atom(right)
            || right
                .strip_prefix('[')
                .and_then(|v| v.strip_suffix(']'))
                .is_some_and(|v| v.bytes().all(|b| matches!(b,33..=90|94..=126))))
}
#[cfg(unix)]
fn private_path(path: &Path) -> Result<()> {
    use std::{
        fs::{self, DirBuilder, OpenOptions},
        os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    };
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or(Error::UnsafePath)?;
    if !parent.exists() {
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
            .map_err(|_| Error::UnsafePath)?;
    }
    let metadata = fs::symlink_metadata(parent).map_err(|_| Error::UnsafePath)?;
    if !metadata.is_dir() || metadata.permissions().mode() & 0o777 != 0o700 {
        return Err(Error::UnsafePath);
    }
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_file() && m.nlink() == 1 && m.permissions().mode() & 0o777 == 0o600 => Ok(()),
        Ok(_) => Err(Error::UnsafePath),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path)
            {
                Ok(_) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => private_path(path),
                Err(_) => Err(Error::UnsafePath),
            }
        }
        Err(_) => Err(Error::UnsafePath),
    }
}
#[cfg(not(unix))]
fn private_path(_: &Path) -> Result<()> {
    Err(Error::UnsafePath)
}
