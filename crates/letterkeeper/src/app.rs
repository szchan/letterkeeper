// SPDX-License-Identifier: GPL-3.0-only
use letterkeeper_adapters::{
    DraftInput as AdapterDraft, FolderSummary, Host, HostError, MessageSummary,
    workspace::{
        ApprovalInfo, DraftSummary, HumanWorkspace, ReplyMode, ReviewSnapshot, SentEvidence,
        SubmissionInfo, SyncInfo, WorkflowError, WorkflowResult, WorkspaceCapabilities,
    },
};
use letterkeeper_content::SafeDocument;
use letterkeeper_core::{AccessPolicy, Draft, DraftInput, Permissions, Store, Submission};
use serde_json::{Value, json};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
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
    json!({"id":d.id,"revision":d.revision,"digest":d.digest,"account":d.input.account,"from":d.input.from,"to":d.input.to,"cc":d.input.cc,"bcc":d.input.bcc,"subject":d.input.subject,"body":d.input.body,"in_reply_to":d.input.in_reply_to,"references":d.input.references,"attachments":d.input.attachments.iter().map(|a|json!({"name":a.name,"content_type":a.content_type,"bytes":a.bytes.len()})).collect::<Vec<_>>()})
}
pub fn submission_view(s: &Submission) -> Value {
    json!({"id":s.id,"draft_id":s.draft_id,"revision":s.revision,"digest":s.digest,"message_id":s.message_id,"state":s.state,"state_recorded":true,"diagnostic":null})
}
/// Preserve a readable submission even when optional diagnostic storage fails.
pub fn submission_status_view(store: &Store, sub: &Submission) -> Value {
    let mut value = submission_view(sub);
    match store.get_submission_diagnostic(sub.id) {
        Ok(diagnostic) => {
            value["diagnostic"] = json!(diagnostic);
            value["diagnostic_query_succeeded"] = json!(true);
        }
        Err(_) => {
            value["diagnostic_query_succeeded"] = json!(false);
        }
    }
    value
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
    state_dir: PathBuf,
    read_only: bool,
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
            state_dir: state_dir.into(),
            read_only,
            documents,
            smtp_test,
        })
    }
}
impl HumanWorkspace for AppHost {
    fn capabilities(&self) -> WorkspaceCapabilities {
        WorkspaceCapabilities {
            account: ACCOUNT.into(),
            from: SENDER.into(),
            read: true,
            draft: !self.read_only,
            send: !self.read_only,
        }
    }
    fn list_drafts(&self, offset: usize, limit: usize) -> WorkflowResult<Vec<DraftSummary>> {
        Ok(self
            .store
            .lock()
            .map_err(|_| WorkflowError::Unavailable)?
            .list_drafts(offset, limit)
            .map_err(crate::workflow::core_error)?
            .into_iter()
            .map(crate::workflow::draft_summary)
            .collect())
    }
    fn load_draft(&self, id: i64) -> WorkflowResult<Draft> {
        self.store
            .lock()
            .map_err(|_| WorkflowError::Unavailable)?
            .get_draft(id)
            .map_err(crate::workflow::core_error)
    }
    fn create(&self, input: DraftInput) -> WorkflowResult<Draft> {
        if self.read_only {
            return Err(WorkflowError::Denied);
        }
        validate_demo(&input).map_err(|_| WorkflowError::Denied)?;
        self.store
            .lock()
            .map_err(|_| WorkflowError::Unavailable)?
            .create_draft(input)
            .map_err(crate::workflow::core_error)
    }
    fn save(&self, id: i64, expected_revision: i64, input: DraftInput) -> WorkflowResult<Draft> {
        if self.read_only {
            return Err(WorkflowError::Denied);
        }
        validate_demo(&input).map_err(|_| WorkflowError::Denied)?;
        self.store
            .lock()
            .map_err(|_| WorkflowError::Unavailable)?
            .update_draft(id, expected_revision, input)
            .map_err(crate::workflow::core_error)
    }
    fn reply(&self, _: &str, _: ReplyMode, _: &str, _: &[String]) -> WorkflowResult<Draft> {
        if self.read_only {
            Err(WorkflowError::Denied)
        } else {
            Err(WorkflowError::Unsupported)
        }
    }
    fn add_attachment(
        &self,
        id: i64,
        expected_revision: i64,
        path: &Path,
        content_type: &str,
    ) -> WorkflowResult<Draft> {
        if self.read_only {
            return Err(WorkflowError::Denied);
        }
        let mut draft = self.load_draft(id)?;
        if draft.revision != expected_revision {
            return Err(WorkflowError::Conflict);
        }
        validate_demo(&draft.input).map_err(|_| WorkflowError::Denied)?;
        let attachment = crate::files::import_attachment(path, content_type)
            .map_err(|_| WorkflowError::InvalidInput)?;
        draft.input.attachments.push(attachment);
        self.save(id, expected_revision, draft.input)
    }
    fn remove_attachment(
        &self,
        id: i64,
        expected_revision: i64,
        index: usize,
    ) -> WorkflowResult<Draft> {
        if self.read_only {
            return Err(WorkflowError::Denied);
        }
        let mut draft = self.load_draft(id)?;
        if draft.revision != expected_revision {
            return Err(WorkflowError::Conflict);
        }
        validate_demo(&draft.input).map_err(|_| WorkflowError::Denied)?;
        if index >= draft.input.attachments.len() {
            return Err(WorkflowError::InvalidInput);
        }
        draft.input.attachments.remove(index);
        self.save(id, expected_revision, draft.input)
    }
    fn export_attachment(&self, _: &str, _: usize, _: &Path) -> WorkflowResult<String> {
        if self.read_only {
            Err(WorkflowError::Denied)
        } else {
            Err(WorkflowError::Unsupported)
        }
    }
    fn review(&self, id: i64) -> WorkflowResult<ReviewSnapshot> {
        Ok(crate::workflow::review_snapshot(self.load_draft(id)?))
    }
    fn approve(
        &self,
        id: i64,
        expected_digest: &str,
        typed_digest: &str,
    ) -> WorkflowResult<ApprovalInfo> {
        if self.read_only {
            return Err(WorkflowError::Denied);
        }
        let mut store = self.store.lock().map_err(|_| WorkflowError::Unavailable)?;
        let d = store.get_draft(id).map_err(crate::workflow::core_error)?;
        validate_demo(&d.input).map_err(|_| WorkflowError::Denied)?;
        crate::workflow::approve(&mut store, id, expected_digest, typed_digest)
    }
    fn send(&self, id: i64) -> WorkflowResult<SubmissionInfo> {
        if self.read_only {
            return Err(WorkflowError::Denied);
        }
        let endpoint = self.smtp_test.ok_or(WorkflowError::Unavailable)?;
        let _sender =
            crate::files::private_lock(&self.state_dir, "send.lock", std::time::Duration::ZERO)
                .map_err(|_| WorkflowError::Unavailable)?;
        let sub = {
            let mut store = self.store.lock().map_err(|_| WorkflowError::Unavailable)?;
            let d = store.get_draft(id).map_err(crate::workflow::core_error)?;
            validate_demo(&d.input).map_err(|_| WorkflowError::Denied)?;
            store
                .claim_send(id, now().map_err(|_| WorkflowError::Unavailable)?)
                .map_err(crate::workflow::core_error)?
        };
        let outcome = crate::transport::send_test(&sub, endpoint);
        match self
            .store
            .lock()
            .map_err(|_| WorkflowError::Unavailable)?
            .finish_submission(sub.id, outcome)
        {
            Ok(done) => Ok(crate::workflow::submission_info(done)),
            Err(_) => Ok(SubmissionInfo {
                id: sub.id,
                draft_id: id,
                state: letterkeeper_core::SubmissionState::Uncertain,
                state_recorded: false,
            }),
        }
    }
    fn list_submissions(&self, offset: usize, limit: usize) -> WorkflowResult<Vec<SubmissionInfo>> {
        Ok(self
            .store
            .lock()
            .map_err(|_| WorkflowError::Unavailable)?
            .list_submissions(offset, limit)
            .map_err(crate::workflow::core_error)?
            .into_iter()
            .map(crate::workflow::submission_entry_info)
            .collect())
    }
    fn submission_status(&self, id: i64) -> WorkflowResult<SubmissionInfo> {
        self.store
            .lock()
            .map_err(|_| WorkflowError::Unavailable)?
            .get_submission(id)
            .map(crate::workflow::submission_info)
            .map_err(crate::workflow::core_error)
    }
    fn sent_evidence(&self, _: i64) -> WorkflowResult<SentEvidence> {
        Err(WorkflowError::Unsupported)
    }
    fn recover_submission(&self, id: i64) -> WorkflowResult<SubmissionInfo> {
        if self.read_only {
            return Err(WorkflowError::Denied);
        }
        let _sender =
            crate::files::private_lock(&self.state_dir, "send.lock", std::time::Duration::ZERO)
                .map_err(|_| WorkflowError::Unavailable)?;
        self.store
            .lock()
            .map_err(|_| WorkflowError::Unavailable)?
            .recover_interrupted(id)
            .map(crate::workflow::submission_info)
            .map_err(crate::workflow::core_error)
    }
    fn sync_draft(&self, _: i64) -> WorkflowResult<SyncInfo> {
        Err(WorkflowError::Unsupported)
    }
    fn sync_sent(&self, _: i64) -> WorkflowResult<SyncInfo> {
        Err(WorkflowError::Unsupported)
    }
    fn sync_status(&self, _: i64) -> WorkflowResult<SyncInfo> {
        Err(WorkflowError::Unsupported)
    }
}
impl Host for AppHost {
    fn list_accounts(&self) -> Result<Value, HostError> {
        Ok(json!({"accounts":[{"id":ACCOUNT,"email":SENDER}]}))
    }
    fn list_folders(&self) -> Result<Vec<FolderSummary>, HostError> {
        Ok(vec![FolderSummary {
            id: "local".into(),
            name: "合成演示".into(),
        }])
    }
    fn list_folder_messages(
        &self,
        folder: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<MessageSummary>, HostError> {
        if folder.is_some_and(|f| f != "local") {
            return Err(host_error("folder_scope_denied"));
        }
        self.list_messages(offset, limit)
    }
    fn search_messages(
        &self,
        folder: Option<&str>,
        query: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<MessageSummary>, HostError> {
        if folder.is_some_and(|f| f != "local") || query.len() > 1024 || limit == 0 || limit > 50 {
            return Err(host_error("invalid_search"));
        }
        let query = query.to_lowercase();
        Ok(self
            .documents
            .iter()
            .enumerate()
            .filter(|(_, d)| {
                format!("{} {} {}", d.subject, d.from, d.text)
                    .to_lowercase()
                    .contains(&query)
            })
            .skip(offset)
            .take(limit)
            .map(|(i, d)| MessageSummary {
                id: format!("mail-{}", i + 1),
                subject: d.subject.clone(),
                from: d.from.clone(),
            })
            .collect())
    }
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
        let info = HumanWorkspace::send(self, numeric_id(id)?)
            .map_err(|error| HostError(error.to_string()))?;
        if info.state_recorded {
            let sub = self
                .store
                .lock()
                .map_err(host_error)?
                .get_submission(info.id)
                .map_err(host_error)?;
            Ok(submission_view(&sub))
        } else {
            Ok(
                json!({"id":info.id,"draft_id":info.draft_id,"state":"Uncertain","state_recorded":false,"diagnostic":null,"detail":"query_persisted_submission_before_any_further_action"}),
            )
        }
    }
    fn send_status(&self, id: &str) -> Result<Value, HostError> {
        let store = self.store.lock().map_err(host_error)?;
        let sub = store.get_submission(numeric_id(id)?).map_err(host_error)?;
        Ok(submission_status_view(&store, &sub))
    }
}
