// SPDX-License-Identifier: GPL-3.0-only
//! Shared scoped remote backend. Network operations never hold SQLite locks.
use crate::{
    app::{draft_view, now, submission_view},
    files::private_lock,
    sandbox::Sandbox,
};
use letterkeeper_accounts::{
    AccountConfig, AccountRepository, Authentication, CredentialStore, GmailOAuth,
    SentCopyStrategy, password_auth_material, unix_now,
};
use letterkeeper_adapters::{
    DraftInput as AdapterDraft, FolderSummary, Host, HostError, MessageSummary,
};
use letterkeeper_content::{ComposeKind, DecodedAttachment, InspectedMail, SafeDocument, compose};
use letterkeeper_core::{
    AccessPolicy, Draft, DraftInput, Permissions, SendOutcome, Store, Submission, SubmissionState,
    SyncRecord,
};
use letterkeeper_protocol::{Credentials, Endpoint, ImapClient, TlsOptions, WriteOutcome};
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
    fn folder(&self, folder: Option<&str>) -> Result<String, HostError> {
        let folder = match folder {
            Some(key) => self
                .config
                .allowed_folders
                .iter()
                .find(|name| folder_id(&self.config.id, name) == key),
            None => self
                .config
                .allowed_folders
                .iter()
                .find(|name| name.eq_ignore_ascii_case("INBOX"))
                .or(self.config.allowed_folders.first()),
        }
        .ok_or_else(|| err("folder_scope_denied"))?;
        Ok(folder.clone())
    }
    fn message_key(&self, key: &str) -> Result<(String, u32, u32), HostError> {
        if key.len() > 128 {
            return Err(err("invalid_message_id"));
        }
        let parts = key.split('-').collect::<Vec<_>>();
        if parts.len() != 4 || parts[0] != "r" {
            return Err(err("invalid_message_id"));
        }
        let folder = self.folder(Some(&format!("f-{}", parts[1])))?;
        let validity = parts[2]
            .parse::<u32>()
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| err("invalid_message_id"))?;
        let uid = parts[3]
            .parse::<u32>()
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| err("invalid_message_id"))?;
        Ok((folder, validity, uid))
    }
    fn message_id(&self, folder: &str, validity: u32, uid: u32) -> String {
        format!(
            "r-{}-{validity}-{uid}",
            &folder_id(&self.config.id, folder)[2..]
        )
    }
    fn credentials(&self) -> Result<Credentials, HostError> {
        let _lock =
            credential_lock(self.vault.as_ref(), &self.state_dir, &self.config.id).map_err(err)?;
        let auth = match self.config.authentication {
            Authentication::Password => password_auth_material(&self.config, self.vault.as_ref()),
            Authentication::GmailOAuth { .. } => GmailOAuth::new(&self.config, self.vault.as_ref())
                .and_then(|g| g.auth_material(unix_now()?)),
        }
        .map_err(err)?;
        Ok(Credentials {
            username: auth.username,
            secret: auth.secret.expose().to_owned(),
            oauth: auth.oauth,
        })
    }
    fn tls(&self) -> Result<TlsOptions, HostError> {
        let roots_pem = self
            .config
            .extra_ca_pem
            .as_ref()
            .map(|path| crate::files::read_regular(path, 65536).map_err(err))
            .transpose()?;
        Ok(TlsOptions { roots_pem })
    }
    fn imap(&self) -> Result<ImapClient, HostError> {
        Ok(ImapClient::new(
            endpoint(&self.config.imap),
            self.tls()?,
            self.credentials()?,
        ))
    }
    fn page(
        &self,
        folder: Option<&str>,
        query: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<MessageSummary>, HostError> {
        self.read_allowed()?;
        let folder = self.folder(folder)?;
        let started = Instant::now();
        let page = self
            .imap()?
            .list_messages(&folder, offset, limit, query)
            .map_err(err)?;
        page.messages
            .into_iter()
            .map(|m| {
                if started.elapsed() > Duration::from_secs(30) {
                    return Err(err("mail_page_timeout"));
                }
                let doc = self.sandbox.parse(&m.headers).map_err(err)?;
                Ok(MessageSummary {
                    id: self.message_id(&folder, page.uid_validity, m.uid),
                    subject: doc.subject,
                    from: doc.from,
                })
            })
            .collect()
    }
    fn raw(&self, key: &str) -> Result<Vec<u8>, HostError> {
        self.read_allowed()?;
        let (folder, validity, uid) = self.message_key(key)?;
        self.imap()?
            .read_message(&folder, validity, uid)
            .map_err(err)
    }
    pub fn inspect(&self, key: &str) -> Result<InspectedMail, HostError> {
        self.sandbox.inspect(&self.raw(key)?).map_err(err)
    }
    pub fn attachment(&self, key: &str, part: usize) -> Result<DecodedAttachment, HostError> {
        self.sandbox.attachment(&self.raw(key)?, part).map_err(err)
    }
    pub fn mark_seen(&self, key: &str) -> Result<(), HostError> {
        self.draft_allowed()?;
        let (folder, validity, uid) = self.message_key(key)?;
        self.imap()?.add_seen(&folder, validity, uid).map_err(err)
    }
    pub fn create_full(&self, input: DraftInput) -> Result<Draft, HostError> {
        self.draft_allowed()?;
        validate_identity(&self.config, &input).map_err(err)?;
        self.store()?.create_draft(input).map_err(err)
    }
    pub fn update_full(
        &self,
        draft_id: i64,
        revision: i64,
        input: DraftInput,
    ) -> Result<Draft, HostError> {
        self.draft_allowed()?;
        validate_identity(&self.config, &input).map_err(err)?;
        self.store()?
            .update_draft(draft_id, revision, input)
            .map_err(err)
    }
    pub fn recover_sender(&self, submission_id: i64) -> Result<Value, HostError> {
        self.send_allowed()?;
        let _lock = private_lock(&self.state_dir, "send.lock", Duration::ZERO).map_err(err)?;
        Ok(submission_view(
            &self
                .store()?
                .recover_interrupted(submission_id)
                .map_err(err)?,
        ))
    }
    fn append_sync(
        &self,
        record: SyncRecord,
        raw: Vec<u8>,
        flags: &[String],
        client: ImapClient,
    ) -> Result<Value, HostError> {
        let result = if raw.len() > 2 * 1024 * 1024 {
            SendOutcome::Failed
        } else {
            outcome(client.append_message(&record.folder, &raw, flags))
        };
        match self.store()?.finish_sync(record.id, result) {
            Ok(done) => Ok(sync_view(&done)),
            Err(_) => Ok(json!({"id":record.id,"state":"Uncertain","state_recorded":false})),
        }
    }
}
impl Host for RemoteHost {
    fn list_messages(&self, offset: usize, limit: usize) -> Result<Vec<MessageSummary>, HostError> {
        self.page(None, None, offset, limit)
    }
    fn list_folder_messages(
        &self,
        folder: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<MessageSummary>, HostError> {
        self.page(folder, None, offset, limit)
    }
    fn search_messages(
        &self,
        folder: Option<&str>,
        query: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<MessageSummary>, HostError> {
        self.page(folder, Some(query), offset, limit)
    }
    fn read_message(&self, key: &str) -> Result<SafeDocument, HostError> {
        self.sandbox.parse(&self.raw(key)?).map_err(err)
    }
    fn list_accounts(&self) -> Result<Value, HostError> {
        self.read_allowed()?;
        Ok(json!({"accounts":[{"id":self.config.id,"email":self.config.email}]}))
    }
    fn list_folders(&self) -> Result<Vec<FolderSummary>, HostError> {
        self.read_allowed()?;
        Ok(self
            .imap()?
            .list_folders()
            .map_err(err)?
            .into_iter()
            .filter(|f| self.config.allowed_folders.contains(&f.name))
            .take(50)
            .map(|f| FolderSummary {
                id: folder_id(&self.config.id, &f.name),
                name: f.name,
            })
            .collect())
    }
    fn create_draft(&self, input: AdapterDraft) -> Result<Value, HostError> {
        Ok(draft_view(&self.create_full(DraftInput {
            account: self.config.id.clone(),
            from: self.config.email.clone(),
            to: input.to,
            cc: vec![],
            bcc: vec![],
            subject: input.subject,
            body: input.body,
            attachments: vec![],
            in_reply_to: None,
            references: vec![],
        })?))
    }
    fn get_draft(&self, draft_id: &str) -> Result<Value, HostError> {
        self.read_allowed()?;
        Ok(draft_view(
            &self.store()?.get_draft(id(draft_id)?).map_err(err)?,
        ))
    }
    fn reply_draft(
        &self,
        key: &str,
        mode: &str,
        body: &str,
        to: &[String],
    ) -> Result<Value, HostError> {
        self.draft_allowed()?;
        let kind = match mode {
            "reply" => ComposeKind::Reply,
            "reply_all" => ComposeKind::ReplyAll,
            "forward" => ComposeKind::Forward,
            _ => return Err(err("invalid_reply_mode")),
        };
        if (kind == ComposeKind::Forward && to.is_empty())
            || (kind != ComposeKind::Forward && !to.is_empty())
        {
            return Err(err("invalid_forward_recipients"));
        }
        let original = self.inspect(key)?;
        let mut draft = compose(&original, &self.config.email, kind, body).map_err(err)?;
        if kind == ComposeKind::Forward {
            draft.to = to.to_vec();
        }
        Ok(draft_view(&self.create_full(DraftInput {
            account: self.config.id.clone(),
            from: self.config.email.clone(),
            to: draft.to,
            cc: draft.cc,
            bcc: vec![],
            subject: draft.subject,
            body: draft.body,
            attachments: vec![],
            in_reply_to: draft.in_reply_to,
            references: draft.references,
        })?))
    }
    fn send_draft(&self, draft_id: &str) -> Result<Value, HostError> {
        self.send_allowed()?;
        let _sender = private_lock(&self.state_dir, "send.lock", Duration::ZERO).map_err(err)?;
        let (sub, raw) = {
            let mut store = self.store()?;
            let draft = store.get_draft(id(draft_id)?).map_err(err)?;
            validate_identity(&self.config, &draft.input).map_err(err)?;
            let sub = store
                .claim_send(draft.id, now().map_err(err)?)
                .map_err(err)?;
            let raw = match crate::transport::prepare(&sub) {
                Ok(message) => message.formatted(),
                Err(_) => {
                    let done = store
                        .finish_submission(sub.id, SendOutcome::Failed)
                        .map_err(err)?;
                    return Ok(submission_view(&done));
                }
            };
            if raw.len() > 2 * 1024 * 1024 {
                let done = store
                    .finish_submission(sub.id, SendOutcome::Failed)
                    .map_err(err)?;
                return Ok(submission_view(&done));
            }
            if let Err(e) = store.record_submission_wire(sub.id, &raw) {
                store
                    .finish_submission(sub.id, SendOutcome::Failed)
                    .map_err(err)?;
                return Err(err(e));
            }
            (sub, raw)
        };
        let to = sub
            .input
            .to
            .iter()
            .chain(&sub.input.cc)
            .chain(&sub.input.bcc)
            .cloned()
            .collect::<Vec<_>>();
        let result = match (self.tls(), self.credentials()) {
            (Ok(tls), Ok(credentials)) => outcome(letterkeeper_protocol::send_smtp(
                &endpoint(&self.config.smtp),
                &tls,
                &credentials,
                &sub.input.from,
                &to,
                &raw,
            )),
            _ => SendOutcome::Failed,
        };
        match self.store()?.finish_submission(sub.id, result) {
            Ok(done) => Ok(submission_view(&done)),
            Err(_) => Ok(
                json!({"id":sub.id,"draft_id":sub.draft_id,"state":"Uncertain","state_recorded":false}),
            ),
        }
    }
    fn send_status(&self, submission_id: &str) -> Result<Value, HostError> {
        self.read_allowed()?;
        Ok(submission_view(
            &self
                .store()?
                .get_submission(id(submission_id)?)
                .map_err(err)?,
        ))
    }
    fn sync_draft(&self, draft_id: &str) -> Result<Value, HostError> {
        self.draft_allowed()?;
        let draft_id = id(draft_id)?;
        let draft = self.store()?.get_draft(draft_id).map_err(err)?;
        validate_identity(&self.config, &draft.input).map_err(err)?;
        // Local credential/CA failures must not consume a remote-write claim.
        let client = self.imap()?;
        let record = self
            .store()?
            .claim_draft_sync(draft_id, &self.config.drafts_folder)
            .map_err(err)?;
        let sub = Submission {
            id: 0,
            draft_id: record.local_id,
            revision: record.revision,
            digest: record.digest.clone(),
            message_id: record.message_id.clone(),
            input: record.input.clone(),
            state: SubmissionState::Submitting,
        };
        let raw = match crate::transport::prepare_snapshot(&sub, true) {
            Ok(message) => message.formatted(),
            Err(_) => {
                return Ok(sync_view(
                    &self
                        .store()?
                        .finish_sync(record.id, SendOutcome::Failed)
                        .map_err(err)?,
                ));
            }
        };
        self.append_sync(record, raw, &["\\Draft".into()], client)
    }
    fn sync_sent(&self, submission_id: &str) -> Result<Value, HostError> {
        self.send_allowed()?;
        if self.config.sent_copy == SentCopyStrategy::ServerManaged {
            return Err(err("sent_copy_is_server_managed_use_sent_evidence"));
        }
        let submission_id = id(submission_id)?;
        let sub = self.store()?.get_submission(submission_id).map_err(err)?;
        if sub.state != SubmissionState::Accepted {
            return Err(err("smtp_acceptance_required"));
        }
        let client = self.imap()?;
        let (record, raw) = {
            let mut store = self.store()?;
            let id = submission_id;
            let raw = store.submission_wire(id).map_err(err)?;
            let record = store
                .claim_sent_sync(id, &self.config.sent_folder)
                .map_err(err)?;
            (record, raw)
        };
        self.append_sync(record, raw, &["\\Seen".into()], client)
    }
    fn sync_status(&self, record_id: &str) -> Result<Value, HostError> {
        self.read_allowed()?;
        Ok(sync_view(
            &self.store()?.get_sync(id(record_id)?).map_err(err)?,
        ))
    }
    fn sent_evidence(&self, submission_id: &str) -> Result<Value, HostError> {
        self.read_allowed()?;
        if !self
            .config
            .allowed_folders
            .contains(&self.config.sent_folder)
        {
            return Err(err("folder_scope_denied"));
        }
        let sub = self
            .store()?
            .get_submission(id(submission_id)?)
            .map_err(err)?;
        let matches = self
            .imap()?
            .lookup_message_id(&self.config.sent_folder, &sub.message_id)
            .map_err(err)?
            .into_iter()
            .map(|k| self.message_id(&self.config.sent_folder, k.uid_validity, k.uid))
            .collect::<Vec<_>>();
        Ok(
            json!({"submission_id":sub.id,"message_id":sub.message_id,"smtp_state":sub.state,"matches":matches,"evidence_only":true,"not_found_does_not_mean_failed":true,"delivery_confirmed":false}),
        )
    }
}
