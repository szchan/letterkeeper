// SPDX-License-Identifier: GPL-3.0-only
use letterkeeper_adapters::{DraftInput as AdapterDraft, Host, HostError, MessageSummary};
use letterkeeper_content::SafeDocument;
use letterkeeper_core::{AccessPolicy, Draft, DraftInput, Permissions, Store, Submission};
use serde_json::{Value, json};
use std::{
    net::SocketAddr,
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

pub const ACCOUNT: &str = "local-demo";
pub const SENDER: &str = "sender@example.test";

pub fn store_at(state_dir: &Path, read_only: bool) -> Result<Store, String> {
    Store::open(state_dir.join("state.sqlite"))
        .map(|store| {
            store.with_policy(AccessPolicy::new(
                vec![ACCOUNT.into()],
                Permissions {
                    read: true,
                    draft: !read_only,
                    send: !read_only,
                },
            ))
        })
        .map_err(|e| e.to_string())
}
pub fn now() -> Result<i64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .map_err(|_| "clock_unavailable".into())
}
pub fn draft_view(d: &Draft) -> Value {
    json!({"id":d.id,"revision":d.revision,"digest":d.digest,"account":d.input.account,"from":d.input.from,"to":d.input.to,"cc":d.input.cc,"bcc":d.input.bcc,"subject":d.input.subject,"body":d.input.body,"attachments":d.input.attachments.iter().map(|a|json!({"name":a.name,"content_type":a.content_type,"bytes":a.bytes.len()})).collect::<Vec<_>>()})
}
pub fn submission_view(s: &Submission) -> Value {
    json!({"id":s.id,"draft_id":s.draft_id,"revision":s.revision,"digest":s.digest,"message_id":s.message_id,"state":s.state,"state_recorded":true})
}
pub fn validate_demo(input: &DraftInput) -> Result<(), String> {
    if input.account != ACCOUNT
        || input.from != SENDER
        || !input
            .to
            .iter()
            .chain(&input.cc)
            .chain(&input.bcc)
            .all(|a| crate::transport::synthetic_address(a))
    {
        return Err("prototype_requires_local_demo_and_synthetic_addresses".into());
    }
    Ok(())
}
fn host_error(_: impl std::fmt::Display) -> HostError {
    HostError("operation_rejected".into())
}
fn numeric_id(id: &str) -> Result<i64, HostError> {
    id.parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| HostError("invalid_id".into()))
}

pub struct AppHost {
    store: Mutex<Store>,
    documents: Vec<SafeDocument>,
    smtp_test: Option<SocketAddr>,
}
impl AppHost {
    pub fn open(
        state_dir: &Path,
        documents: Vec<SafeDocument>,
        read_only: bool,
        smtp_test: Option<SocketAddr>,
    ) -> Result<Self, String> {
        if documents.len() > 50 || smtp_test.is_some_and(|s| !s.ip().is_loopback()) {
            return Err("prototype_scope_exceeded".into());
        }
        Ok(Self {
            store: Mutex::new(store_at(state_dir, read_only)?),
            documents,
            smtp_test,
        })
    }
}
impl Host for AppHost {
    fn list_messages(&self, offset: usize, limit: usize) -> Result<Vec<MessageSummary>, HostError> {
        Ok(self
            .documents
            .iter()
            .enumerate()
            .skip(offset)
            .take(limit.min(50))
            .map(|(i, d)| MessageSummary {
                id: format!("mail-{}", i + 1),
                subject: d.subject.clone(),
                from: d.from.clone(),
            })
            .collect())
    }
    fn read_message(&self, id: &str) -> Result<SafeDocument, HostError> {
        let index = id
            .strip_prefix("mail-")
            .ok_or_else(|| host_error("invalid_id"))?
            .parse::<usize>()
            .map_err(host_error)?;
        index
            .checked_sub(1)
            .and_then(|i| self.documents.get(i))
            .cloned()
            .ok_or_else(|| host_error("not_found"))
    }
    fn create_draft(&self, input: AdapterDraft) -> Result<Value, HostError> {
        let input = DraftInput {
            account: ACCOUNT.into(),
            from: SENDER.into(),
            to: input.to,
            cc: vec![],
            bcc: vec![],
            subject: input.subject,
            body: input.body,
            attachments: vec![],
            in_reply_to: None,
            references: vec![],
        };
        validate_demo(&input).map_err(host_error)?;
        let draft = self
            .store
            .lock()
            .map_err(host_error)?
            .create_draft(input)
            .map_err(host_error)?;
        Ok(draft_view(&draft))
    }
    fn get_draft(&self, id: &str) -> Result<Value, HostError> {
        Ok(draft_view(
            &self
                .store
                .lock()
                .map_err(host_error)?
                .get_draft(numeric_id(id)?)
                .map_err(host_error)?,
        ))
    }
    fn send_draft(&self, id: &str) -> Result<Value, HostError> {
        let endpoint = self
            .smtp_test
            .ok_or_else(|| HostError("test_transport_not_configured".into()))?;
        let sub = {
            let mut store = self.store.lock().map_err(host_error)?;
            let draft = store.get_draft(numeric_id(id)?).map_err(host_error)?;
            validate_demo(&draft.input).map_err(host_error)?;
            store
                .claim_send(draft.id, now().map_err(host_error)?)
                .map_err(host_error)?
        };
        let outcome = crate::transport::send_test(&sub, endpoint);
        let mut store = self.store.lock().map_err(host_error)?;
        match store.finish_submission(sub.id, outcome) {
            Ok(done) => Ok(submission_view(&done)),
            Err(_) => Ok(
                json!({"id":sub.id,"draft_id":sub.draft_id,"state":"Uncertain","state_recorded":false,"detail":"query_persisted_submission_before_any_further_action"}),
            ),
        }
    }
    fn send_status(&self, id: &str) -> Result<Value, HostError> {
        Ok(submission_view(
            &self
                .store
                .lock()
                .map_err(host_error)?
                .get_submission(numeric_id(id)?)
                .map_err(host_error)?,
        ))
    }
}
