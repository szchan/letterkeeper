// SPDX-License-Identifier: GPL-3.0-only
//! Shared scoped remote backend. Network operations never hold SQLite locks.
use crate::{
    app::{draft_view, now, submission_status_view, submission_view},
    files::private_lock,
    sandbox::Sandbox,
};
use letterkeeper_accounts::{
    AccountConfig, AccountRepository, Authentication, CredentialStore, GmailOAuth,
    SentCopyStrategy, password_auth_material, unix_now,
};
use letterkeeper_adapters::{
    DraftInput as AdapterDraft, FolderSummary, Host, HostError, MessageSummary,
    workspace::{
        ApprovalInfo, DraftSummary, HumanWorkspace, ReplyMode, ReviewSnapshot, SentEvidence,
        SubmissionInfo, SyncInfo, WorkflowError, WorkflowResult, WorkspaceCapabilities,
    },
};
use letterkeeper_content::{ComposeKind, DecodedAttachment, InspectedMail, SafeDocument, compose};
use letterkeeper_core::{
    AccessPolicy, Draft, DraftInput, Permissions, SendDiagnostic, SendFailureKind, SendOutcome,
    SendStage, Store, Submission, SubmissionState, SyncRecord,
};
use letterkeeper_protocol::{
    Credentials, Endpoint, ImapClient, ProtocolError, SmtpDiagnostic, SmtpFailureKind, SmtpStage,
    TlsOptions, WriteOutcome,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};
fn err(e: impl std::fmt::Display) -> HostError {
    HostError(e.to_string())
}
fn id(value: &str) -> Result<i64, HostError> {
    value
        .parse::<i64>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| err("invalid_id"))
}
fn endpoint(e: &letterkeeper_accounts::TlsEndpoint) -> Endpoint {
    Endpoint {
        host: e.host.clone(),
        port: e.port,
    }
}
fn outcome(value: WriteOutcome) -> SendOutcome {
    match value {
        WriteOutcome::Accepted => SendOutcome::Accepted,
        WriteOutcome::Failed => SendOutcome::Failed,
        WriteOutcome::Uncertain => SendOutcome::Uncertain,
    }
}
fn diagnostic(value: SmtpDiagnostic) -> SendDiagnostic {
    SendDiagnostic {
        stage: match value.stage {
            SmtpStage::Preparation => SendStage::Preparation,
            SmtpStage::TlsConfiguration => SendStage::TlsConfiguration,
            SmtpStage::Connection => SendStage::Connection,
            SmtpStage::TlsHandshake => SendStage::TlsHandshake,
            SmtpStage::Greeting => SendStage::Greeting,
            SmtpStage::Authentication => SendStage::Authentication,
            SmtpStage::MailFrom => SendStage::MailFrom,
            SmtpStage::Recipient => SendStage::Recipient,
            SmtpStage::DataCommand => SendStage::DataCommand,
            SmtpStage::Submission => SendStage::Submission,
        },
        kind: match value.kind {
            SmtpFailureKind::InvalidInput => SendFailureKind::InvalidInput,
            SmtpFailureKind::Connection => SendFailureKind::Connection,
            SmtpFailureKind::Timeout => SendFailureKind::Timeout,
            SmtpFailureKind::Rejected => SendFailureKind::Rejected,
            SmtpFailureKind::UnexpectedResponse => SendFailureKind::UnexpectedResponse,
        },
        smtp_code: value.smtp_code,
    }
}
fn local_diagnostic(stage: SendStage, kind: SendFailureKind) -> SendDiagnostic {
    SendDiagnostic {
        stage,
        kind,
        smtp_code: None,
    }
}
fn folder_id(account: &str, folder: &str) -> String {
    let hash = Sha256::digest(serde_json::to_vec(&(account, folder)).expect("strings serialize"));
    format!("f-{:x}", hash)[..18].to_owned()
}
pub fn store_for_account(config: &AccountConfig, state_dir: &Path) -> Result<Store, String> {
    config.validate().map_err(|e| e.to_string())?;
    // Reuse descriptor-walk validation before opening the private SQLite path.
    AccountRepository::new(state_dir.to_path_buf(), state_dir.to_path_buf())
        .map_err(|e| e.to_string())?;
    Store::open(state_dir.join("state.sqlite"))
        .map(|s| {
            s.with_policy(AccessPolicy::new(
                vec![config.id.clone()],
                Permissions {
                    read: config.permissions.read,
                    draft: config.permissions.draft,
                    send: config.permissions.send,
                },
            ))
        })
        .map_err(|e| e.to_string())
}
pub fn validate_identity(config: &AccountConfig, input: &DraftInput) -> Result<(), String> {
    if input.account != config.id || !input.from.eq_ignore_ascii_case(&config.email) {
        Err("draft_identity_outside_account".into())
    } else {
        Ok(())
    }
}
pub fn credential_lock(
    vault: &dyn CredentialStore,
    state_dir: &Path,
    account: &str,
) -> Result<std::fs::File, String> {
    let directory = vault
        .coordination_directory(account)
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| state_dir.to_path_buf());
    AccountRepository::new(directory.clone(), directory.clone()).map_err(|e| e.to_string())?;
    private_lock(&directory, "auth.lock", Duration::from_secs(15))
}
fn sync_view(r: &SyncRecord) -> Value {
    json!({"id":r.id,"kind":r.kind,"local_id":r.local_id,"revision":r.revision,"account":r.account,"folder":r.folder,"message_id":r.message_id,"state":r.state,"state_recorded":true})
}
mod authentication;
mod drafts;
mod mail;
mod send;
mod sync;

pub struct RemoteHost {
    pub config: AccountConfig,
    state_dir: PathBuf,
    vault: Arc<dyn CredentialStore>,
    store: Mutex<Store>,
    sandbox: Sandbox,
}
impl RemoteHost {
    pub fn open(
        config: AccountConfig,
        state_dir: &Path,
        vault: Arc<dyn CredentialStore>,
        sandbox: Sandbox,
    ) -> Result<Self, String> {
        let store = store_for_account(&config, state_dir)?;
        Ok(Self {
            config,
            state_dir: state_dir.into(),
            vault,
            store: Mutex::new(store),
            sandbox,
        })
    }
    fn store(&self) -> Result<MutexGuard<'_, Store>, HostError> {
        self.store.lock().map_err(|_| err("storage_unavailable"))
    }
    fn read_allowed(&self) -> Result<(), HostError> {
        if self.config.permissions.read {
            Ok(())
        } else {
            Err(err("read_scope_denied"))
        }
    }
    fn draft_allowed(&self) -> Result<(), HostError> {
        self.read_allowed()?;
        if self.config.permissions.draft {
            Ok(())
        } else {
            Err(err("draft_scope_denied"))
        }
    }
    fn send_allowed(&self) -> Result<(), HostError> {
        self.read_allowed()?;
        if self.config.permissions.send {
            Ok(())
        } else {
            Err(err("send_scope_denied"))
        }
    }
}
impl HumanWorkspace for RemoteHost {
    fn capabilities(&self) -> WorkspaceCapabilities {
        WorkspaceCapabilities {
            account: self.config.id.clone(),
            from: self.config.email.clone(),
            read: self.config.permissions.read,
            draft: self.config.permissions.draft,
            send: self.config.permissions.send,
        }
    }
    fn list_drafts(&self, offset: usize, limit: usize) -> WorkflowResult<Vec<DraftSummary>> {
        self.workspace_list_drafts(offset, limit)
    }
    fn load_draft(&self, id: i64) -> WorkflowResult<Draft> {
        self.workspace_load_draft(id)
    }
    fn create(&self, input: DraftInput) -> WorkflowResult<Draft> {
        self.workspace_create(input)
    }
    fn save(&self, id: i64, expected_revision: i64, input: DraftInput) -> WorkflowResult<Draft> {
        self.workspace_save(id, expected_revision, input)
    }
    fn reply(
        &self,
        message_id: &str,
        mode: ReplyMode,
        body: &str,
        to: &[String],
    ) -> WorkflowResult<Draft> {
        self.workspace_reply(message_id, mode, body, to)
    }
    fn add_attachment(
        &self,
        id: i64,
        expected_revision: i64,
        path: &Path,
        content_type: &str,
    ) -> WorkflowResult<Draft> {
        self.workspace_add_attachment(id, expected_revision, path, content_type)
    }
    fn remove_attachment(
        &self,
        id: i64,
        expected_revision: i64,
        index: usize,
    ) -> WorkflowResult<Draft> {
        self.workspace_remove_attachment(id, expected_revision, index)
    }
    fn export_attachment(
        &self,
        message_id: &str,
        part: usize,
        directory: &Path,
    ) -> WorkflowResult<String> {
        self.workspace_export_attachment(message_id, part, directory)
    }
    fn review(&self, id: i64) -> WorkflowResult<ReviewSnapshot> {
        self.workspace_review(id)
    }
    fn approve(
        &self,
        id: i64,
        expected_digest: &str,
        typed_digest: &str,
    ) -> WorkflowResult<ApprovalInfo> {
        self.workspace_approve(id, expected_digest, typed_digest)
    }
    fn send(&self, id: i64) -> WorkflowResult<SubmissionInfo> {
        self.workspace_send(id)
    }
    fn list_submissions(&self, offset: usize, limit: usize) -> WorkflowResult<Vec<SubmissionInfo>> {
        self.workspace_list_submissions(offset, limit)
    }
    fn submission_status(&self, id: i64) -> WorkflowResult<SubmissionInfo> {
        self.workspace_submission_status(id)
    }
    fn submission_diagnostic(&self, id: i64) -> WorkflowResult<Option<SendDiagnostic>> {
        self.workspace_submission_diagnostic(id)
    }
    fn sent_evidence(&self, id: i64) -> WorkflowResult<SentEvidence> {
        self.workspace_sent_evidence(id)
    }
    fn recover_submission(&self, id: i64) -> WorkflowResult<SubmissionInfo> {
        self.workspace_recover_submission(id)
    }
    fn sync_draft(&self, id: i64) -> WorkflowResult<SyncInfo> {
        self.workspace_sync_draft(id)
    }
    fn sync_sent(&self, id: i64) -> WorkflowResult<SyncInfo> {
        self.workspace_sync_sent(id)
    }
    fn sync_status(&self, id: i64) -> WorkflowResult<SyncInfo> {
        self.workspace_sync_status(id)
    }
}
impl Host for RemoteHost {
    fn list_messages(&self, offset: usize, limit: usize) -> Result<Vec<MessageSummary>, HostError> {
        self.host_list_messages(offset, limit)
    }
    fn list_folder_messages(
        &self,
        folder: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<MessageSummary>, HostError> {
        self.host_list_folder_messages(folder, offset, limit)
    }
    fn search_messages(
        &self,
        folder: Option<&str>,
        query: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<MessageSummary>, HostError> {
        self.host_search_messages(folder, query, offset, limit)
    }
    fn read_message(&self, key: &str) -> Result<SafeDocument, HostError> {
        self.host_read_message(key)
    }
    fn list_accounts(&self) -> Result<Value, HostError> {
        self.host_list_accounts()
    }
    fn list_folders(&self) -> Result<Vec<FolderSummary>, HostError> {
        self.host_list_folders()
    }
    fn create_draft(&self, input: AdapterDraft) -> Result<Value, HostError> {
        self.host_create_draft(input)
    }
    fn get_draft(&self, draft_id: &str) -> Result<Value, HostError> {
        self.host_get_draft(draft_id)
    }
    fn reply_draft(
        &self,
        key: &str,
        mode: &str,
        body: &str,
        to: &[String],
    ) -> Result<Value, HostError> {
        self.host_reply_draft(key, mode, body, to)
    }
    fn send_draft(&self, draft_id: &str) -> Result<Value, HostError> {
        self.host_send_draft(draft_id)
    }
    fn send_status(&self, submission_id: &str) -> Result<Value, HostError> {
        self.host_send_status(submission_id)
    }
    fn sync_draft(&self, draft_id: &str) -> Result<Value, HostError> {
        self.host_sync_draft(draft_id)
    }
    fn sync_sent(&self, submission_id: &str) -> Result<Value, HostError> {
        self.host_sync_sent(submission_id)
    }
    fn sync_status(&self, record_id: &str) -> Result<Value, HostError> {
        self.host_sync_status(record_id)
    }
    fn sent_evidence(&self, submission_id: &str) -> Result<Value, HostError> {
        self.host_sent_evidence(submission_id)
    }
}
#[cfg(test)]
mod send_recording_tests {
    use super::*;
    struct NoVault;
    impl CredentialStore for NoVault {
        fn get(
            &self,
            _: &str,
            _: letterkeeper_accounts::SecretKind,
        ) -> Result<Option<letterkeeper_accounts::Secret>, letterkeeper_accounts::AccountError>
        {
            unreachable!()
        }
        fn set(
            &self,
            _: &str,
            _: letterkeeper_accounts::SecretKind,
            _: &letterkeeper_accounts::Secret,
        ) -> Result<(), letterkeeper_accounts::AccountError> {
            unreachable!()
        }
        fn delete(
            &self,
            _: &str,
            _: letterkeeper_accounts::SecretKind,
        ) -> Result<(), letterkeeper_accounts::AccountError> {
            unreachable!()
        }
    }
    #[test]
    fn unavailable_state_recording_never_claims_a_persisted_diagnostic() {
        let config: AccountConfig = serde_json::from_value(json!({"id":"controlled","email":"reader@example.test","username":"person@example.test","imap":{"host":"localhost","port":9993},"smtp":{"host":"localhost","port":9465},"allowed_folders":["INBOX","Drafts","Sent"],"permissions":{"read":true,"draft":true,"send":true},"drafts_folder":"Drafts","sent_folder":"Sent","sent_copy":"server_managed","authentication":{"type":"password"},"extra_ca_pem":null})).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let host = RemoteHost::open(
            config,
            &dir.path().join("state"),
            Arc::new(NoVault),
            Sandbox {
                executable: "/nonexistent-worker".into(),
                launcher: "/usr/bin/bwrap".into(),
            },
        )
        .unwrap();
        let sub = {
            let mut store = host.store().unwrap();
            let input = serde_json::from_value(json!({"account":"controlled","from":"reader@example.test","to":["recipient@example.test"],"cc":[],"bcc":[],"subject":"synthetic","body":"synthetic","attachments":[]})).unwrap();
            let draft = store.create_draft(input).unwrap();
            store.approve(draft.id, &draft.digest, i64::MAX).unwrap();
            store.claim_send(draft.id, 1).unwrap()
        };
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = host.store.lock().unwrap();
            panic!("synthetic storage failure");
        }));
        for result in [
            SendOutcome::Accepted,
            SendOutcome::Failed,
            SendOutcome::Uncertain,
        ] {
            let value = host.finish_send(
                &sub,
                result,
                Some(local_diagnostic(
                    SendStage::Submission,
                    SendFailureKind::Connection,
                )),
            );
            assert_eq!(value["state"], "Uncertain");
            assert_eq!(value["state_recorded"], false);
            assert!(value.get("diagnostic").unwrap().is_null());
        }
    }
}
