// SPDX-License-Identifier: GPL-3.0-only
use super::Store;
use crate::{AccessPolicy, Error, Permissions, Result};
use rusqlite::{Connection, TransactionBehavior};
use std::time::Duration;
impl Store {
    pub(super) fn initialize(mut conn: Connection) -> Result<Self> {
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
            1..=3 => {}
            _ => return Err(Error::SchemaVersion),
        }
        if version < 2 {
            tx.execute_batch("CREATE TABLE sync_records(id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL CHECK(kind IN ('draft','sent')), local_id INTEGER NOT NULL, revision INTEGER NOT NULL, folder TEXT NOT NULL, message_id TEXT NOT NULL, digest TEXT NOT NULL, content TEXT NOT NULL, state TEXT NOT NULL CHECK(state IN ('pending','confirmed','failed','uncertain')), UNIQUE(kind,local_id,revision));
                CREATE TABLE submission_wire(submission_id INTEGER PRIMARY KEY REFERENCES submissions(id), wire BLOB NOT NULL); PRAGMA user_version=2;")?;
        }
        if version < 3 {
            tx.execute_batch("CREATE TABLE submission_diagnostics(
                submission_id INTEGER PRIMARY KEY REFERENCES submissions(id),
                stage TEXT NOT NULL CHECK(stage IN ('preparation','tls_configuration','credentials','connection','tls_handshake','greeting','authentication','mail_from','recipient','data_command','submission')),
                kind TEXT NOT NULL CHECK(kind IN ('invalid_input','connection','timeout','rejected','unexpected_response','unavailable')),
                smtp_code INTEGER CHECK(smtp_code IS NULL OR (typeof(smtp_code)='integer' AND smtp_code BETWEEN 100 AND 599))
            ); PRAGMA user_version=3;")?;
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
}
